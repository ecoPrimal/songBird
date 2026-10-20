// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Tower Atomic JSON-RPC server: accepts IPC connections and dispatches methods.

use super::JsonRpcHandler;
use super::types::{
    JSONRPC_VERSION, JsonRpcError, JsonRpcErrorExt, JsonRpcRequestWire, JsonRpcResponse,
    JsonRpcResponseExt,
};
use crate::endpoint::VirtualEndpoint;
use crate::error::{IpcError, IpcResult};
use crate::ipc;
use bytes::Bytes;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error};

/// Tower Atomic Server - Serves JSON-RPC over Universal IPC
///
/// This server handles JSON-RPC 2.0 requests over the Universal IPC layer,
/// providing a platform-agnostic RPC server.
pub struct TowerAtomicServer<H: JsonRpcHandler> {
    handler: Arc<H>,
}

impl<H: JsonRpcHandler + 'static> TowerAtomicServer<H> {
    /// Create a new Tower Atomic server
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }

    /// Create from a pre-built shared handler (state unification).
    pub fn from_shared(handler: Arc<H>) -> Self {
        Self {
            handler,
        }
    }

    /// Serve JSON-RPC requests on the given endpoint.
    ///
    /// Listens for connections and handles requests until cancelled.
    pub async fn serve(&self, endpoint: VirtualEndpoint) -> IpcResult<()> {
        self.serve_inner(endpoint, None).await
    }

    /// Serve with readiness notification — signals after socket is bound.
    ///
    /// Callers can `await` the `oneshot::Receiver` to know the server is ready
    /// for connections without resorting to sleep-based polling.
    pub async fn serve_with_ready(
        &self,
        endpoint: VirtualEndpoint,
        ready: tokio::sync::oneshot::Sender<()>,
    ) -> IpcResult<()> {
        self.serve_inner(endpoint, Some(ready)).await
    }

    async fn serve_inner(
        &self,
        endpoint: VirtualEndpoint,
        ready: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> IpcResult<()> {
        debug!("Starting Tower Atomic server on {}", endpoint.path);

        let mut listener = ipc::listen(endpoint).await?;

        if let Some(tx) = ready {
            let _ = tx.send(());
        }

        loop {
            match listener.accept().await {
                Ok(stream) => {
                    let handler = Arc::clone(&self.handler);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            error!("Connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single client connection
    async fn handle_connection(stream: crate::ipc::Stream, handler: Arc<H>) -> IpcResult<()> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);
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

                    let request = match serde_json::from_str::<JsonRpcRequestWire<'_>>(&line) {
                        Ok(req) => req,
                        Err(e) => {
                            let resp = JsonRpcResponse::error(
                                Value::Null,
                                JsonRpcError::PARSE_ERROR,
                                format!("Failed to parse request: {e}"),
                            );
                            Self::write_response(&mut writer, &resp).await?;
                            continue;
                        }
                    };

                    let is_notification = request.is_notification();
                    debug!(
                        "JSON-RPC request: {} (notification={})",
                        request.method.as_ref(),
                        is_notification
                    );
                    let response = Self::handle_request(
                        request.jsonrpc.as_ref(),
                        request.method.as_ref(),
                        request.params,
                        request.id,
                        &*handler,
                    )
                    .await;

                    if is_notification {
                        continue;
                    }

                    Self::write_response(&mut writer, &response).await?;
                }
                Err(e) => {
                    error!("Failed to read from socket: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Serialize and write a JSON-RPC response with safe fallback.
    ///
    /// If serialization fails (should never happen for our types), a
    /// hard-coded internal-error JSON is written so the client always
    /// sees a valid frame rather than a dropped connection.
    async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
        writer: &mut W,
        response: &JsonRpcResponse,
    ) -> IpcResult<()> {
        const FALLBACK: &[u8] =
            b"{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"Internal serialization error\"},\"id\":null}\n";

        match serde_json::to_vec(response) {
            Ok(mut buf) => {
                buf.push(b'\n');
                let payload = Bytes::from(buf);
                writer.write_all(&payload).await.map_err(|e| IpcError::Other(e.to_string()))?;
            }
            Err(e) => {
                error!("JSON-RPC response serialization failed: {e}");
                writer.write_all(FALLBACK).await.map_err(|e| IpcError::Other(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(
        jsonrpc: &str,
        method: &str,
        params: Option<Value>,
        id: Option<Value>,
        handler: &H,
    ) -> JsonRpcResponse {
        let id = id.unwrap_or(Value::Null);

        if jsonrpc != JSONRPC_VERSION {
            return JsonRpcResponse::error(
                id,
                JsonRpcError::INVALID_REQUEST,
                "Invalid JSON-RPC version (must be 2.0)",
            );
        }

        let params = params.unwrap_or(Value::Null);
        match handler.handle(method, params).await {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(message) => JsonRpcResponse::error_from(JsonRpcError::internal_error(message), id),
        }
    }
}

#[cfg(test)]
impl<H: JsonRpcHandler + 'static> TowerAtomicServer<H> {
    /// Exposes [`TowerAtomicServer::handle_request`] for unit tests (no I/O).
    pub(crate) async fn handle_request_for_test(
        request: super::types::JsonRpcRequest,
        handler: &H,
    ) -> JsonRpcResponse {
        Self::handle_request(&request.jsonrpc, &request.method, request.params, request.id, handler)
            .await
    }
}
