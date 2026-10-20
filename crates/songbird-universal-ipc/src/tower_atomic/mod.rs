// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Tower Atomic - JSON-RPC over Universal IPC
//!
//! **Tower Atomic** is the security provider-inspired pattern for JSON-RPC communication
//! over IPC. This module provides a universal adapter that works across all
//! platforms using the Universal IPC layer.
//!
//! ## Overview
//!
//! Tower Atomic enables:
//! - ✅ **Platform-agnostic JSON-RPC** (works on all platforms)
//! - ✅ **Type-safe RPC** (Rust type system)
//! - ✅ **Async/await** (modern Rust concurrency)
//! - ✅ **Zero hardcoding** (capability-based discovery)
//! - ✅ **Pure Rust** (no C dependencies)
//!
//! ## Architecture
//!
//! ```text
//! Application Layer:
//!   - Call JSON-RPC methods via Tower Atomic client
//!
//! Tower Atomic Layer (this module):
//!   - JSON-RPC 2.0 protocol handling
//!   - Request/response serialization
//!   - Error handling
//!
//! Universal IPC Layer:
//!   - Platform-agnostic transport
//!   - Connection management
//!
//! Platform Layer:
//!   - Unix sockets, Named pipes, TCP
//! ```
//!
//! ## Usage
//!
//! ### Server (Service Provider)
//!
//! ```rust,no_run
//! use songbird_universal_ipc::tower_atomic::{TowerAtomicServer, JsonRpcHandler};
//! use songbird_universal_ipc::ipc;
//! use serde_json::{json, Value};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!
//! // Define your RPC handler
//! struct MyService;
//!
//! impl JsonRpcHandler for MyService {
//!     async fn handle(&self, method: &str, params: Value) -> Result<Value, String> {
//!         match method {
//!             "add" => {
//!                 let a = params["a"].as_i64().ok_or("Missing a")?;
//!                 let b = params["b"].as_i64().ok_or("Missing b")?;
//!                 Ok(json!(a + b))
//!             }
//!             _ => Err(format!("Unknown method: {}", method))
//!         }
//!     }
//! }
//!
//! // Initialize and start server
//! ipc::init()?;
//! let endpoint = ipc::register("my-service", vec![String::from("math")]).await?;
//!
//! let server = TowerAtomicServer::new(MyService);
//! server.serve(endpoint).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Client (Service Consumer)
//!
//! ```rust,no_run
//! use songbird_universal_ipc::tower_atomic::TowerAtomicClient;
//! use songbird_universal_ipc::capability::discovery;
//! use serde_json::json;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!
//! // Discover service by capability
//! let provider = discovery::discover("math").await?;
//!
//! // Connect via Tower Atomic
//! let client = TowerAtomicClient::connect(&provider.virtual_endpoint).await?;
//!
//! // Call RPC method
//! let result = client.call("add", json!({"a": 5, "b": 3})).await?;
//! assert_eq!(result, json!(8));
//! # Ok(())
//! # }
//! ```

mod client;
mod server;
mod types;

use serde_json::Value;

pub use client::TowerAtomicClient;
pub use server::TowerAtomicServer;
pub use types::{
    JSONRPC_VERSION, JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcRequestWire,
    JsonRpcResponse, JsonRpcResponseExt,
};

/// JSON-RPC handler trait
///
/// Implement this trait to handle JSON-RPC requests in your service.
/// All implementations must produce `Send` futures for use with `tokio::spawn`.
pub trait JsonRpcHandler: Send + Sync {
    /// Handle a JSON-RPC method call
    ///
    /// # Arguments
    /// * `method` - The method name
    /// * `params` - The method parameters (JSON value)
    ///
    /// # Returns
    /// The result value or an error message
    fn handle(
        &self,
        method: &str,
        params: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send;
}

#[cfg(test)]
mod tests;
