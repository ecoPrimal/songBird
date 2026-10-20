// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! IPC session protocol: BTSP auto-detection, JSON-RPC dispatch, and
//! encrypted framing.
//!
//! Extracted from `server.rs` to separate the per-connection protocol
//! stack from server lifecycle/bootstrap concerns.

use songbird_universal_ipc::service::IpcServiceHandler;
use songbird_universal_ipc::tower_atomic::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcHandler, JsonRpcRequest, JsonRpcResponse,
    JsonRpcResponseExt,
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::ipc::pure_rust_server::method_gate::{
    CallerContext, MethodGate, dispatch_auth_method, extract_bearer_token,
};

use super::server::BIN_GATE;

/// Dispatch a parsed JSON-RPC request through the method gate and handler.
///
/// Consolidates bearer token extraction → auth dispatch → gate check → handler
/// into a single path used by both plaintext and encrypted sessions.
pub(super) async fn dispatch_gated(
    request: &mut JsonRpcRequest,
    handler: &IpcServiceHandler,
    gate: &MethodGate,
    caller: &CallerContext,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);

    let caller = if let Some(ref mut params) = request.params {
        if let Some(token) = extract_bearer_token(params) {
            caller.clone().with_bearer_token(token)
        } else {
            caller.clone()
        }
    } else {
        caller.clone()
    };

    if let Some(auth_result) = dispatch_auth_method(&request.method, gate, &caller) {
        return JsonRpcResponse::success(id, auth_result);
    }

    if let Err(gate_err) = gate.check(&request.method, &caller) {
        return JsonRpcResponse::error_from(gate_err, id);
    }

    match handler
        .handle(&request.method, request.params.take().unwrap_or(serde_json::Value::Null))
        .await
    {
        Ok(result) => JsonRpcResponse::success(id, result),
        Err(message) => JsonRpcResponse::error_from(JsonRpcError::internal_error(message), id),
    }
}

/// Handle a single connection with riboCipher signal detection + BTSP auto-detection.
///
/// Peeks the first byte to detect riboCipher transport signals (`0xEC`/`0xED`/`0xEE`).
/// If no signal is present, falls back to line-based BTSP detection for backward
/// compatibility (legacy path — will WARN in Wave 112+).
pub(super) async fn handle_connection<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: S,
    handler: Arc<IpcServiceHandler>,
    security_client: Arc<songbird_http_client::SecurityRpcClient>,
    peer_label: &str,
    caller: &CallerContext,
) {
    use songbird_types::constants::ribocipher;

    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    // Peek first byte for riboCipher signal detection
    let first_byte = match reader.fill_buf().await {
        Ok(buf) if !buf.is_empty() => buf[0],
        Ok(_) => {
            tracing::debug!("{peer_label} disconnected before sending data");
            return;
        }
        Err(e) => {
            tracing::error!("{peer_label} read error on peek: {e}");
            return;
        }
    };

    if ribocipher::is_signal_byte(first_byte) {
        let tier = ribocipher::tier_name(first_byte);
        reader.consume(1); // consume signal byte

        // Read version byte
        let version_ok = match reader.fill_buf().await {
            Ok(buf) if !buf.is_empty() && buf[0] == ribocipher::VERSION_1 => {
                reader.consume(1);
                true
            }
            Ok(buf) if !buf.is_empty() => {
                tracing::warn!(
                    "{peer_label} riboCipher {tier}: unsupported version 0x{:02X}",
                    buf[0]
                );
                false
            }
            _ => {
                tracing::warn!("{peer_label} riboCipher {tier}: missing version byte");
                false
            }
        };

        if !version_ok {
            return;
        }

        tracing::debug!("{peer_label} riboCipher signal: tier={tier}");

        // After consuming the 2-byte signal prefix, route to JSON-RPC session
        handle_json_rpc_lines(
            &mut reader,
            &mut writer,
            &handler,
            &security_client,
            peer_label,
            caller,
        )
        .await;
        return;
    }

    // Non-riboCipher path: G65 negotiation, BTSP, or plain JSON-RPC
    tracing::debug!("{peer_label} connection without riboCipher signal");
    let mut first_line = String::new();

    match reader.read_line(&mut first_line).await {
        Ok(0) => {
            tracing::debug!("{peer_label} disconnected before sending data");
            return;
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{peer_label} read error on first line: {e}");
            return;
        }
    }

    // G65 Protocol Negotiation: client sends "PROTOCOLS: tarpc,jsonrpc\n"
    if first_line.trim().starts_with("PROTOCOLS: ") {
        handle_protocol_negotiation(first_line.trim(), &reader, &mut writer, handler, peer_label)
            .await;
        return;
    }

    if crate::ipc::btsp::is_btsp_client_hello(&first_line) {
        tracing::info!("{peer_label} BTSP ClientHello detected — starting NDJSON handshake");
        match crate::ipc::btsp::perform_server_handshake_ndjson(
            &first_line,
            &mut reader,
            &mut writer,
            &security_client,
        )
        .await
        {
            Ok(session) => {
                tracing::info!(
                    "{peer_label} BTSP handshake complete (session={}, cipher={})",
                    session.session_id,
                    session.cipher,
                );
            }
            Err(e) => {
                tracing::warn!("{peer_label} BTSP handshake failed: {e}");
                let err_frame =
                    serde_json::json!({"error":"handshake_failed","reason":e.to_string()});
                let mut bytes = serde_json::to_vec(&err_frame).unwrap_or_default();
                bytes.push(b'\n');
                let _ = writer.write_all(&bytes).await;
                let _ = writer.flush().await;
                return;
            }
        }
    } else if !first_line.trim().is_empty() {
        dispatch_json_rpc_line(&first_line, &mut writer, &handler, peer_label, caller).await;
    }

    handle_json_rpc_lines(&mut reader, &mut writer, &handler, &security_client, peer_label, caller)
        .await;
}

/// Process a stream of newline-delimited JSON-RPC requests.
///
/// Handles `btsp.negotiate` inline: sends the NDJSON response, then (if a real
/// cipher was negotiated) switches to encrypted framing.
async fn handle_json_rpc_lines<R, W>(
    reader: &mut R,
    writer: &mut W,
    handler: &Arc<IpcServiceHandler>,
    security_client: &Arc<songbird_http_client::SecurityRpcClient>,
    peer_label: &str,
    caller: &CallerContext,
) where
    R: tokio::io::AsyncBufRead + tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                tracing::debug!("{peer_label} disconnected");
                break;
            }
            Ok(_) => {
                if line.trim().is_empty() {
                    continue;
                }

                if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&line)
                    && request.method == "btsp.negotiate"
                {
                    let id = request.id.unwrap_or(serde_json::Value::Null);
                    let params = request.params.unwrap_or(serde_json::Value::Null);
                    let (result, keys) =
                        crate::ipc::btsp_phase3::handle_negotiate(&params, security_client).await;

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(&result).unwrap_or_default(),
                    );
                    if let Ok(json) = serde_json::to_string(&resp) {
                        let _ = writer.write_all(json.as_bytes()).await;
                        let _ = writer.write_all(b"\n").await;
                        let _ = writer.flush().await;
                    }

                    if let Some(session_keys) = keys {
                        tracing::debug!(
                            "{peer_label} BTSP Phase 3: switching to encrypted framing"
                        );
                        handle_encrypted_json_rpc(
                            reader,
                            writer,
                            handler,
                            session_keys,
                            peer_label,
                            caller,
                        )
                        .await;
                        return;
                    }
                    continue;
                }

                dispatch_json_rpc_line(&line, writer, handler, peer_label, caller).await;
            }
            Err(e) => {
                tracing::error!("{peer_label} read error: {e}");
                break;
            }
        }
    }
}

/// Encrypted JSON-RPC loop (BTSP Phase 3).
async fn handle_encrypted_json_rpc<R, W>(
    reader: &mut R,
    writer: &mut W,
    handler: &Arc<IpcServiceHandler>,
    keys: crate::ipc::btsp_phase3::SessionKeys,
    peer_label: &str,
    caller: &CallerContext,
) where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    tracing::info!("{peer_label} BTSP Phase 3: encrypted session active");
    loop {
        let frame = match crate::ipc::btsp_phase3::read_encrypted_frame(reader).await {
            Ok(f) => f,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("UnexpectedEof") || msg.contains("failed to read frame length") {
                    tracing::debug!("{peer_label} BTSP Phase 3: client disconnected");
                } else {
                    tracing::error!("{peer_label} BTSP Phase 3 frame read error: {e:#}");
                }
                break;
            }
        };

        let plaintext = match keys.decrypt(&frame) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("{peer_label} BTSP Phase 3 decrypt failed: {e:#}");
                break;
            }
        };

        let mut request = match serde_json::from_slice::<JsonRpcRequest>(&plaintext) {
            Ok(req) => req,
            Err(e) => {
                let resp = JsonRpcResponse::error(
                    serde_json::Value::Null,
                    JsonRpcError::PARSE_ERROR,
                    format!("Failed to parse request: {e}"),
                );
                if let Ok(resp_bytes) = serde_json::to_vec(&resp)
                    && let Ok(encrypted) = keys.encrypt(&resp_bytes)
                {
                    let _ =
                        crate::ipc::btsp_phase3::write_encrypted_frame(writer, &encrypted).await;
                }
                continue;
            }
        };

        let is_notification = request.id.is_none();
        tracing::debug!(
            "{peer_label} BTSP Phase 3 JSON-RPC: {} (notification={is_notification})",
            request.method
        );

        let response = dispatch_gated(&mut request, handler, &BIN_GATE, caller).await;

        if !is_notification
            && let Ok(resp_bytes) = serde_json::to_vec(&response)
            && let Ok(encrypted) = keys.encrypt(&resp_bytes)
            && let Err(e) = crate::ipc::btsp_phase3::write_encrypted_frame(writer, &encrypted).await
        {
            tracing::error!("{peer_label} BTSP Phase 3 write error: {e:#}");
            break;
        }
    }
}

/// Parse and dispatch a single JSON-RPC line, writing the response.
async fn dispatch_json_rpc_line<W: tokio::io::AsyncWrite + Unpin>(
    line: &str,
    writer: &mut W,
    handler: &Arc<IpcServiceHandler>,
    peer_label: &str,
    caller: &CallerContext,
) {
    let mut request = match serde_json::from_str::<JsonRpcRequest>(line) {
        Ok(req) => req,
        Err(e) => {
            let resp = JsonRpcResponse::error(
                serde_json::Value::Null,
                JsonRpcError::PARSE_ERROR,
                format!("Failed to parse request: {e}"),
            );
            if let Ok(json) = serde_json::to_string(&resp) {
                let _ = writer.write_all(json.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
            }
            return;
        }
    };

    let is_notification = request.id.is_none();
    tracing::debug!("{peer_label} JSON-RPC: {} (notification={is_notification})", request.method,);

    let response = dispatch_gated(&mut request, handler, &BIN_GATE, caller).await;

    if !is_notification && let Ok(response_json) = serde_json::to_string(&response) {
        let _ = writer.write_all(response_json.as_bytes()).await;
        let _ = writer.write_all(b"\n").await;
    }
}

/// G65 Protocol Negotiation handler.
///
/// Client sent `"PROTOCOLS: tarpc,jsonrpc\n"` — select best protocol and respond.
/// If tarpc is selected, switch to bincode length-delimited framing on this connection.
/// If jsonrpc, continue with line-delimited JSON-RPC on the same stream.
async fn handle_protocol_negotiation<R, W>(
    request_line: &str,
    _reader: &R,
    writer: &mut W,
    _handler: Arc<IpcServiceHandler>,
    peer_label: &str,
) where
    R: tokio::io::AsyncBufRead + tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use songbird_universal::protocol_negotiation::{
        IpcProtocol, NegotiationRequest, NegotiationResponse, select_protocol,
    };

    let request = match NegotiationRequest::from_wire(request_line) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("{peer_label} G65: {e}");
            return;
        }
    };

    let selected = select_protocol(&request.supported, &IpcProtocol::all_supported());
    let response = NegotiationResponse::new(selected);

    if writer.write_all(response.to_wire().as_bytes()).await.is_err() {
        return;
    }
    if writer.flush().await.is_err() {
        return;
    }

    tracing::info!("{peer_label} G65 negotiated: {selected}");
}
