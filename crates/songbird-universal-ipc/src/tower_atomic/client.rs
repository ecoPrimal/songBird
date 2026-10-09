// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Tower Atomic JSON-RPC client: connect to a virtual endpoint and invoke methods.

use super::types::{JsonRpcRequest, JsonRpcResponseWire};
use crate::error::{IpcError, IpcResult};
use crate::ipc;
use bytes::Bytes;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::debug;

/// Tower Atomic Client - Call JSON-RPC methods over Universal IPC
///
/// This client makes JSON-RPC 2.0 calls over the Universal IPC layer,
/// providing a platform-agnostic RPC client.
pub struct TowerAtomicClient {
    stream: Arc<Mutex<crate::ipc::Stream>>,
    next_id: Arc<AtomicU64>,
}

impl TowerAtomicClient {
    /// Connect to a JSON-RPC peer listening on a filesystem Unix domain socket (no Universal IPC registry).
    ///
    /// This is used by the storage provider (capability: `storage.*`) and other providers that expose a raw socket path from capability discovery.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::IpcError`] if the socket cannot be reached or on non-Unix platforms.
    #[cfg(unix)]
    pub async fn connect_unix_path(path: &Path) -> IpcResult<Self> {
        debug!("Tower Atomic: connecting to Unix socket {}", path.display());

        let mut stream = tokio::net::UnixStream::connect(path).await.map_err(|e| {
            IpcError::ConnectionFailed(format!(
                "Failed to connect to Unix socket at {}: {e}",
                path.display()
            ))
        })?;

        // Send riboCipher clear-tier signal before any JSON-RPC traffic.
        // This identifies the connection as a protocol-aware client to
        // the receiving primal (bearDog, sweetGrass, etc.).
        use songbird_types::constants::ribocipher;
        stream.write_all(&ribocipher::CLEAR_PREFIX).await.map_err(|e| {
            IpcError::ConnectionFailed(format!("Failed to send riboCipher signal: {e}"))
        })?;

        Ok(Self {
            stream: Arc::new(Mutex::new(crate::ipc::Stream::from_async_stream_impl(
                crate::platform::AsyncStreamImpl::Unix(stream),
            ))),
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// [`Self::connect_unix_path`] is Unix-only; on other platforms this always fails.
    #[cfg(not(unix))]
    pub async fn connect_unix_path(_path: &Path) -> IpcResult<Self> {
        Err(crate::error::IpcError::PlatformError(String::from(
            "Tower Atomic Unix socket connections are only supported on Unix platforms",
        )))
    }

    /// Connect to a service via virtual endpoint path
    ///
    /// # Example
    /// ```rust,no_run
    /// # use songbird_universal_ipc::tower_atomic::TowerAtomicClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = TowerAtomicClient::connect("/primal/security-provider").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(virtual_path: &str) -> IpcResult<Self> {
        debug!("Connecting to {} via Tower Atomic", virtual_path);

        let stream = ipc::connect(virtual_path).await?;

        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Call a JSON-RPC method
    ///
    /// # Example
    /// ```rust,no_run
    /// # use songbird_universal_ipc::tower_atomic::TowerAtomicClient;
    /// # use serde_json::json;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = TowerAtomicClient::connect("/primal/test").await?;
    /// let result = client.call("add", json!({"a": 5, "b": 3})).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn call(&self, method: &str, params: Value) -> IpcResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest::new(method, Some(params), id);

        // Serialize request (`to_vec` avoids a UTF-8 `String` intermediate; `Bytes` shares cheaply).
        let mut request_bytes = serde_json::to_vec(&request)
            .map_err(|e| IpcError::Other(format!("Failed to serialize request: {e}")))?;

        debug!(method, id, payload_len = request_bytes.len(), "Sending JSON-RPC request");

        // Send request and read response (lock scope minimized)
        let response_line = {
            let mut stream = self.stream.lock().await;

            request_bytes.push(b'\n');
            let payload = Bytes::from(request_bytes);
            stream.write_all(&payload).await.map_err(|e| IpcError::Other(e.to_string()))?;

            let mut line = String::new();
            {
                let mut reader = BufReader::new(&mut *stream);
                reader.read_line(&mut line).await.map_err(|e| IpcError::Other(e.to_string()))?;
            }
            drop(stream);
            line
        };

        debug!("Received JSON-RPC response: {}", response_line);

        // Parse response (borrows strings from `response_line` until we move out error/result).
        let response: JsonRpcResponseWire<'_> = serde_json::from_str(&response_line)
            .map_err(|e| IpcError::Other(format!("Failed to parse response: {e}")))?;

        // Check for error
        if let Some(error) = response.error {
            return Err(IpcError::RpcError(error.message.into_owned()));
        }

        // Return result
        response.result.ok_or_else(|| IpcError::Other(String::from("Missing result in response")))
    }

    /// Call a JSON-RPC method without parameters
    pub async fn call_no_params(&self, method: &str) -> IpcResult<Value> {
        self.call(method, Value::Null).await
    }
}
