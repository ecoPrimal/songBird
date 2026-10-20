// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Session protocol handlers: BTSP binary framing, NDJSON, and encrypted sessions.
//!
//! Extracted from `connection.rs` to isolate session lifecycle management from
//! accept loops and protocol detection. Each handler manages its connection
//! after the routing decision has been made.
//!
//! ## Session types
//!
//! - **BTSP binary**: Length-prefixed (4B BE u32) frames with optional Phase 3 upgrade.
//! - **NDJSON**: Newline-delimited JSON-RPC with inline `btsp.negotiate` upgrade path.
//! - **Encrypted (Phase 3)**: ChaCha20-Poly1305 encrypted frames after negotiate.

#[cfg(unix)]
use anyhow::bail;
use anyhow::{Context, Result};
use bytes::Bytes;
#[cfg(unix)]
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info};

use super::super::protocol::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt,
};
use super::UnixSocketServer;
#[cfg(unix)]
use crate::ipc::btsp;
use crate::ipc::btsp_phase3;

#[cfg(unix)]
impl UnixSocketServer {
    /// Run the full BTSP lifecycle (handshake → persistent framed JSON-RPC) on
    /// any bidirectional async stream. Works with both raw `UnixStream` and the
    /// `PeekedStream` adapter used after first-byte auto-detection.
    pub(super) async fn handle_btsp_on_stream<S>(
        &self,
        mut stream: S,
        caller: &super::super::method_gate::CallerContext,
    ) -> Result<()>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
    {
        debug!("New IPC connection (BTSP mode)");

        let session = btsp::perform_server_handshake(&mut stream, &self.security_client)
            .await
            .context("BTSP handshake failed")?;

        info!("BTSP session {} authenticated (cipher: {})", session.session_id, session.cipher);

        self.handle_btsp_frames(stream, &session, caller).await
    }

    /// Persistent BTSP frame loop: reads length-prefixed JSON-RPC frames until
    /// the client disconnects or a read timeout expires.
    ///
    /// Intercepts `btsp.negotiate` requests to perform Phase 3 cipher upgrade.
    /// On successful negotiation with a real cipher, transitions to encrypted
    /// framing via [`Self::handle_encrypted_session`].
    async fn handle_btsp_frames<S>(
        &self,
        mut stream: S,
        session: &btsp::BtspSession,
        caller: &super::super::method_gate::CallerContext,
    ) -> Result<()>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
    {
        let mut len_buf = [0u8; 4];
        loop {
            match tokio::time::timeout(
                songbird_types::defaults::timeouts::DEFAULT_IDLE_TIMEOUT,
                stream.read_exact(&mut len_buf),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    debug!("BTSP client disconnected (session {})", session.session_id);
                    break;
                }
                Ok(Err(e)) => {
                    return Err(e).context("BTSP frame length read error");
                }
                Err(_) => {
                    debug!("BTSP idle timeout (session {})", session.session_id);
                    break;
                }
            }

            let frame_len = u32::from_be_bytes(len_buf) as usize;
            if frame_len > songbird_types::defaults::network::BTSP_MAX_FRAME_SIZE {
                bail!(
                    "BTSP frame exceeds {} MiB limit ({frame_len})",
                    songbird_types::defaults::network::BTSP_MAX_FRAME_SIZE / (1024 * 1024)
                );
            }

            let mut payload = vec![0u8; frame_len];
            stream.read_exact(&mut payload).await.context("BTSP payload read error")?;

            let request = match serde_json::from_slice::<JsonRpcRequest>(&payload) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse::error_from(
                        JsonRpcError::parse_error(format!("Failed to parse JSON-RPC request: {e}")),
                        serde_json::Value::Null,
                    );
                    Self::write_btsp_response(&mut stream, &resp).await?;
                    continue;
                }
            };

            if request.method == "btsp.negotiate" {
                let id = request.id.clone().unwrap_or(serde_json::Value::Null);
                let params = request.params.unwrap_or(serde_json::Value::Null);
                let (result, keys) =
                    btsp_phase3::handle_negotiate(&params, &self.security_client).await;

                let resp =
                    JsonRpcResponse::success(id, serde_json::to_value(&result).unwrap_or_default());
                Self::write_btsp_response(&mut stream, &resp).await?;

                if let Some(session_keys) = keys {
                    debug!("BTSP Phase 3: switching binary-framed session to encrypted framing");
                    let (reader, writer) = tokio::io::split(stream);
                    return self
                        .handle_encrypted_session(reader, writer, session_keys, caller)
                        .await;
                }
                continue;
            }

            let is_notification = request.id.is_none();
            debug!(
                "BTSP JSON-RPC: {} (notification={}, session={})",
                request.method, is_notification, session.session_id
            );
            let response = self.handle_jsonrpc_request(request, caller).await;

            if !is_notification {
                Self::write_btsp_response(&mut stream, &response).await?;
            }
        }

        Ok(())
    }

    pub(super) async fn write_btsp_response<W: AsyncWriteExt + Unpin>(
        writer: &mut W,
        response: &JsonRpcResponse,
    ) -> Result<()> {
        let resp_bytes = serde_json::to_vec(response)?;
        let resp_len = u32::try_from(resp_bytes.len()).context("response exceeds u32::MAX")?;
        writer.write_all(&resp_len.to_be_bytes()).await?;
        writer.write_all(&resp_bytes).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Handle a pre-consumed first line as JSON-RPC, then continue with a
    /// persistent NDJSON session. Used when the first-line discrimination read
    /// the line to check for BTSP but found normal JSON-RPC instead.
    #[cfg(unix)]
    pub(super) async fn handle_ndjson_first_line_then_session<R, W>(
        &self,
        first_line: String,
        reader: R,
        mut writer: W,
        caller: &super::super::method_gate::CallerContext,
    ) -> Result<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if !first_line.trim().is_empty() {
            match serde_json::from_str::<JsonRpcRequest>(&first_line) {
                Ok(request) => {
                    let is_notification = request.id.is_none();
                    debug!(
                        "JSON-RPC request: {} (notification={})",
                        request.method, is_notification
                    );
                    let response = self.handle_jsonrpc_request(request, caller).await;
                    if !is_notification {
                        let mut payload = serde_json::to_vec(&response)?;
                        payload.push(b'\n');
                        writer.write_all(&Bytes::from(payload)).await?;
                        writer.flush().await?;
                    }
                }
                Err(e) => {
                    let resp = JsonRpcResponse::error_from(
                        JsonRpcError::parse_error(format!("Failed to parse JSON-RPC request: {e}")),
                        serde_json::Value::Null,
                    );
                    let mut payload = serde_json::to_vec(&resp)?;
                    payload.push(b'\n');
                    writer.write_all(&Bytes::from(payload)).await?;
                    writer.flush().await?;
                }
            }
        }

        self.handle_ndjson_session(reader, writer, caller).await
    }
}

impl UnixSocketServer {
    /// Persistent newline-delimited JSON-RPC session: reads requests in a loop
    /// until the client disconnects. Generic over any buffered reader + writer pair.
    ///
    /// If the client sends a `btsp.negotiate` request, the handler processes
    /// the Phase 3 negotiation, sends the NDJSON response, and (if a real
    /// cipher was negotiated) switches to the encrypted frame loop for all
    /// subsequent traffic.
    pub(super) async fn handle_ndjson_session<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
        caller: &super::super::method_gate::CallerContext,
    ) -> Result<()>
    where
        R: AsyncBufRead + AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("Client disconnected");
                    break;
                }
                Ok(_) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    let request = match serde_json::from_str::<JsonRpcRequest>(&line) {
                        Ok(req) => req,
                        Err(e) => {
                            let resp = JsonRpcResponse::error_from(
                                JsonRpcError::parse_error(format!(
                                    "Failed to parse JSON-RPC request: {e}"
                                )),
                                serde_json::Value::Null,
                            );
                            let mut payload = serde_json::to_vec(&resp)?;
                            payload.push(b'\n');
                            writer.write_all(&Bytes::from(payload)).await?;
                            writer.flush().await?;
                            continue;
                        }
                    };

                    if request.method == "btsp.negotiate" {
                        let id = request.id.clone().unwrap_or(serde_json::Value::Null);
                        let params = request.params.unwrap_or(serde_json::Value::Null);
                        let (result, keys) =
                            btsp_phase3::handle_negotiate(&params, &self.security_client).await;

                        let resp = JsonRpcResponse::success(
                            id,
                            serde_json::to_value(&result).unwrap_or_default(),
                        );
                        let mut payload = serde_json::to_vec(&resp)?;
                        payload.push(b'\n');
                        writer.write_all(&Bytes::from(payload)).await?;
                        writer.flush().await?;

                        if let Some(session_keys) = keys {
                            debug!("BTSP Phase 3: switching to encrypted framing");
                            return self
                                .handle_encrypted_session(reader, writer, session_keys, caller)
                                .await;
                        }
                        continue;
                    }

                    let is_notification = request.id.is_none();
                    debug!(
                        "JSON-RPC request: {} (notification={})",
                        request.method, is_notification
                    );
                    let response = self.handle_jsonrpc_request(request, caller).await;

                    if !is_notification {
                        let mut payload = serde_json::to_vec(&response)?;
                        payload.push(b'\n');
                        writer.write_all(&Bytes::from(payload)).await?;
                        writer.flush().await?;
                    }
                }
                Err(e) => {
                    error!("Failed to read from socket: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Persistent encrypted JSON-RPC session (BTSP Phase 3).
    ///
    /// After `btsp.negotiate` upgrades the connection, all subsequent traffic
    /// uses length-prefixed encrypted frames:
    /// `[4B len (BE u32)][12B nonce][ciphertext + Poly1305 tag]`
    pub(super) async fn handle_encrypted_session<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
        keys: btsp_phase3::SessionKeys,
        caller: &super::super::method_gate::CallerContext,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        info!("BTSP Phase 3: encrypted session active");
        loop {
            let frame = match btsp_phase3::read_encrypted_frame(&mut reader).await {
                Ok(f) => f,
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg.contains("UnexpectedEof") || msg.contains("failed to read frame length")
                    {
                        debug!("BTSP Phase 3: client disconnected");
                        break;
                    }
                    return Err(e).context("BTSP Phase 3: frame read error");
                }
            };

            let plaintext = keys.decrypt(&frame).context("BTSP Phase 3: decryption failed")?;

            let request = match serde_json::from_slice::<JsonRpcRequest>(&plaintext) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse::error_from(
                        JsonRpcError::parse_error(format!("Failed to parse JSON-RPC request: {e}")),
                        serde_json::Value::Null,
                    );
                    let resp_bytes = serde_json::to_vec(&resp)?;
                    let encrypted = keys.encrypt(&resp_bytes)?;
                    btsp_phase3::write_encrypted_frame(&mut writer, &encrypted).await?;
                    continue;
                }
            };

            let is_notification = request.id.is_none();
            debug!("BTSP Phase 3 JSON-RPC: {} (notification={})", request.method, is_notification);
            let response = self.handle_jsonrpc_request(request, caller).await;

            if !is_notification {
                let resp_bytes = serde_json::to_vec(&response)?;
                let encrypted = keys.encrypt(&resp_bytes)?;
                btsp_phase3::write_encrypted_frame(&mut writer, &encrypted).await?;
            }
        }

        Ok(())
    }
}
