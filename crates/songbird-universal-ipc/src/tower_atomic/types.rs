// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! JSON-RPC 2.0 request and response types for Tower Atomic.
//!
//! This module re-exports canonical JSON-RPC types from `cellmembrane-types` and
//! provides borrowing wire types ([`JsonRpcRequestWire`], [`JsonRpcResponseWire`])
//! that avoid allocating duplicate strings on hot paths when parsing newline-delimited JSON.
//!
//! Extension traits ([`JsonRpcErrorExt`], [`JsonRpcResponseExt`]) add convenience
//! factory methods used by the Tower Atomic server and client.

use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;

/// JSON-RPC protocol version string.
pub const JSONRPC_VERSION: &str = "2.0";

// ── Canonical types (re-exported from cellmembrane-types) ──────────────────
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

// ── Extension: JsonRpcError factory methods ────────────────────────────────

/// Convenience factory methods for [`JsonRpcError`] used by songbird IPC.
///
/// These are extension methods beyond the canonical constants; import this
/// trait wherever you need `JsonRpcError::method_not_found(…)` etc.
pub trait JsonRpcErrorExt {
    /// Create a method-not-found error.
    fn method_not_found(method: impl Into<String>) -> Self;
    /// Create an invalid-params error.
    fn invalid_params(message: impl Into<String>) -> Self;
    /// Create an internal error.
    fn internal_error(message: impl Into<String>) -> Self;
}

impl JsonRpcErrorExt for JsonRpcError {
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

// ── Extension: JsonRpcResponse factory with pre-built error ────────────────

/// Convenience factory for [`JsonRpcResponse`] that accepts a pre-built
/// [`JsonRpcError`] (canonical `error()` inlines code+message instead).
pub trait JsonRpcResponseExt {
    /// Build an error response from a pre-constructed [`JsonRpcError`].
    fn error_from(error: JsonRpcError, id: Value) -> Self;
}

impl JsonRpcResponseExt for JsonRpcResponse {
    fn error_from(error: JsonRpcError, id: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

// ── Wire types (zero-copy borrowing, kept local) ───────────────────────────

/// Borrowing view of a request line — avoids allocating `jsonrpc` / `method` strings on the server hot path.
///
/// This module is not re-exported; `pub` here is crate-internal visibility for sibling `tower_atomic` modules.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequestWire<'a> {
    #[serde(borrow)]
    pub jsonrpc: Cow<'a, str>,
    #[serde(borrow)]
    pub method: Cow<'a, str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

impl JsonRpcRequestWire<'_> {
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// Client response line: borrows error messages from the read buffer when possible.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponseWire<'a> {
    #[expect(dead_code, reason = "envelope field; validated implicitly")]
    #[serde(borrow)]
    pub jsonrpc: Cow<'a, str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorWire<'a>>,
    #[allow(
        dead_code,
        reason = "JSON-RPC correlation id; reserved for future client-side validation"
    )]
    pub id: Value,
}

/// Borrowing JSON-RPC error object for client-side parsing.
#[derive(Debug, Deserialize)]
pub struct JsonRpcErrorWire<'a> {
    #[allow(
        dead_code,
        reason = "standard JSON-RPC error code; message carries user-facing detail"
    )]
    pub code: i32,
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    #[expect(dead_code, reason = "optional JSON-RPC error extension object")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_new_sets_id_and_version() {
        let req = JsonRpcRequest::new("health.check", None, 1);
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "health.check");
        assert!(!req.is_notification());
    }

    #[test]
    fn request_is_notification_when_no_id() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "event.fire".into(),
            params: None,
            id: None,
        };
        assert!(req.is_notification());
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = JsonRpcRequest::new("test.method", Some(json!({"key": "value"})), 1);
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: JsonRpcRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.method, "test.method");
        assert_eq!(deserialized.params.unwrap()["key"], "value");
    }

    #[test]
    fn request_skips_null_params_in_serialization() {
        let req = JsonRpcRequest::new("simple", None, 1);
        let serialized = serde_json::to_string(&req).unwrap();
        assert!(!serialized.contains("params"));
    }

    #[test]
    fn response_success_has_result_no_error() {
        let resp = JsonRpcResponse::success(json!(1), json!("ok"));
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.result, Some(json!("ok")));
        assert!(resp.error.is_none());
        assert_eq!(resp.id, json!(1));
    }

    #[test]
    fn response_error_has_error_no_result() {
        let err = JsonRpcError::method_not_found("unknown.method");
        let resp = JsonRpcResponse::error_from(err, json!(99));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, json!(99));
        let e = resp.error.unwrap();
        assert_eq!(e.code, JsonRpcError::METHOD_NOT_FOUND);
        assert!(e.message.contains("unknown.method"));
    }

    #[test]
    fn response_serialization_roundtrip() {
        let resp = JsonRpcResponse::success(json!(5), json!({"status": "healthy"}));
        let serialized = serde_json::to_string(&resp).unwrap();
        let deserialized: JsonRpcResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.result.unwrap()["status"], "healthy");
    }

    #[test]
    fn error_method_not_found_code() {
        let err = JsonRpcError::method_not_found("foo");
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("foo"));
        assert!(err.data.is_none());
    }

    #[test]
    fn error_invalid_params_code() {
        let err = JsonRpcError::invalid_params("missing field 'name'");
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("missing field"));
    }

    #[test]
    fn error_internal_error_code() {
        let err = JsonRpcError::internal_error("unexpected failure");
        assert_eq!(err.code, -32603);
        assert!(err.message.contains("unexpected"));
    }

    #[test]
    fn error_constants_match_spec() {
        assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
        assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
        assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
        assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
        assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
    }

    #[test]
    fn wire_request_deserialization() {
        let raw = r#"{"jsonrpc":"2.0","method":"test","params":null,"id":1}"#;
        let wire: JsonRpcRequestWire = serde_json::from_str(raw).unwrap();
        assert_eq!(wire.method.as_ref(), "test");
        assert!(!wire.is_notification());
    }

    #[test]
    fn wire_request_notification() {
        let raw = r#"{"jsonrpc":"2.0","method":"event.fire"}"#;
        let wire: JsonRpcRequestWire = serde_json::from_str(raw).unwrap();
        assert!(wire.is_notification());
    }

    #[test]
    fn wire_response_success_deserialization() {
        let raw = r#"{"jsonrpc":"2.0","result":"ok","id":1}"#;
        let wire: JsonRpcResponseWire = serde_json::from_str(raw).unwrap();
        assert_eq!(wire.result, Some(json!("ok")));
        assert!(wire.error.is_none());
    }

    #[test]
    fn wire_response_error_deserialization() {
        let raw = r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"not found"},"id":2}"#;
        let wire: JsonRpcResponseWire = serde_json::from_str(raw).unwrap();
        assert!(wire.result.is_none());
        let e = wire.error.unwrap();
        assert_eq!(e.code, -32601);
        assert_eq!(e.message.as_ref(), "not found");
    }

    #[test]
    fn jsonrpc_version_constant() {
        assert_eq!(JSONRPC_VERSION, "2.0");
    }
}
