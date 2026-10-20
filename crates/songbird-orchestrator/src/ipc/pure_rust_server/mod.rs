// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Pure Rust Unix Socket JSON-RPC Server (v3.22.0+)
//!
//! ## Evolution
//!
//! **v3.22.0**: Evolved from jsonrpsee to pure Rust (`security provider` pattern)
//! **v4.9.0**: Refactored into domain-driven modules
//!
//! ## Module Structure
//!
//! - `protocol`: JSON-RPC 2.0 types and error codes
//! - `server`: Core server (`UnixSocketServer`), connection accept loops, request handlers
//! - `coordination_handlers`: IPC for coordination / AI clients (capability discovery, health)
//!
//! ## Public API
//!
//! Re-exports the main types needed for using the Unix socket server:
//!
//! - `UnixSocketServer`: Main server type
//! - `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`: Protocol types

pub mod coordination_handlers;
pub mod method_gate;
pub mod protocol;
pub mod server;

// Re-export the main types for convenience
pub use protocol::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt,
};

#[cfg(unix)]
pub use server::UnixSocketServer;
