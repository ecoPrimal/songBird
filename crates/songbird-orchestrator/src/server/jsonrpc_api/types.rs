// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! JSON-RPC 2.0 types for the orchestrator's HTTP JSON-RPC API.
//!
//! Re-exports canonical types from `cellmembrane-types` and adds
//! factory methods via [`JsonRpcErrorExt`] for concise error construction.

// ── Canonical types ────────────────────────────────────────────────────
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// JSON-RPC 2.0 version string (static to avoid allocations)
pub const JSONRPC_VERSION: &str = "2.0";

// ── Extension: JsonRpcError factory methods ────────────────────────────

/// Convenience factory methods for [`JsonRpcError`].
///
/// Import this trait to use `JsonRpcError::parse_error()`,
/// `JsonRpcError::invalid_request(msg)`, etc.
pub trait JsonRpcErrorExt {
    /// Parse error (-32700).
    fn parse_error() -> Self;
    /// Invalid request (-32600).
    fn invalid_request(message: impl Into<String>) -> Self;
    /// Method not found (-32601).
    fn method_not_found(method: impl Into<String>) -> Self;
    /// Invalid params (-32602).
    fn invalid_params(message: impl Into<String>) -> Self;
    /// Internal error (-32603).
    fn internal_error(message: impl Into<String>) -> Self;
}

impl JsonRpcErrorExt for JsonRpcError {
    fn parse_error() -> Self {
        Self {
            code: Self::PARSE_ERROR,
            message: String::from("Parse error"),
            data: None,
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_REQUEST,
            message: message.into(),
            data: None,
        }
    }

    fn method_not_found(method: impl Into<String>) -> Self {
        let method = method.into();
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_PARAMS,
            message: message.into(),
            data: None,
        }
    }

    fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: message.into(),
            data: None,
        }
    }
}
