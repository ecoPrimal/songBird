// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! JSON-RPC 2.0 Protocol Types
//!
//! Re-exports canonical types from `cellmembrane-types` for inter-primal communication.
//! Extension traits provide factory methods for concise error construction.

// ── Canonical types ────────────────────────────────────────────────────
pub use cellmembrane_types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// JSON-RPC 2.0 version string.
pub const JSONRPC_VERSION: &str = "2.0";

// ── Extension: JsonRpcError factory methods ────────────────────────────

/// Convenience factory methods for [`JsonRpcError`] used by the IPC protocol.
pub trait JsonRpcErrorExt {
    /// Create a parse error with a custom message.
    fn parse_error(message: impl Into<String>) -> Self;
    /// Create an invalid request error.
    fn invalid_request(message: impl Into<String>) -> Self;
    /// Create a method not found error.
    fn method_not_found(method: impl Into<String>) -> Self;
    /// Create an invalid params error.
    fn invalid_params(message: impl Into<String>) -> Self;
    /// Create an internal error.
    fn internal_error(message: impl Into<String>) -> Self;
    /// Create a custom error with code, message, and optional data.
    fn custom(code: i32, message: impl Into<String>, data: Option<String>) -> Self;
}

impl JsonRpcErrorExt for JsonRpcError {
    fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: Self::PARSE_ERROR,
            message: message.into(),
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

    fn custom(code: i32, message: impl Into<String>, data: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: data.map(serde_json::Value::String),
        }
    }
}

// ── Extension: JsonRpcResponse with pre-built error ────────────────────

/// Convenience factory for [`JsonRpcResponse`] that accepts a pre-built
/// [`JsonRpcError`] (canonical `error()` inlines code+message instead).
pub trait JsonRpcResponseExt {
    /// Build an error response from a pre-constructed [`JsonRpcError`].
    fn error_from(error: JsonRpcError, id: serde_json::Value) -> Self;
}

impl JsonRpcResponseExt for JsonRpcResponse {
    fn error_from(error: JsonRpcError, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, reason = "unit tests")]

    use super::{JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt};

    #[test]
    fn json_rpc_request_roundtrip_with_params_and_string_id() {
        let json = r#"{"jsonrpc":"2.0","method":"subtract","params":[42,23],"id":"1"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "subtract");
        assert_eq!(req.params, Some(serde_json::json!([42, 23])));
        assert_eq!(req.id, Some(serde_json::json!("1")));
    }

    #[test]
    fn json_rpc_request_null_id_deserializes_to_none() {
        let json = r#"{"jsonrpc":"2.0","method":"ping","id":null}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "ping");
        assert!(req.params.is_none());
        assert!(req.id.is_none());
    }

    #[test]
    fn json_rpc_request_omitted_id_and_params() {
        let json = r#"{"jsonrpc":"2.0","method":"notify"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "notify");
        assert!(req.params.is_none());
        assert!(req.id.is_none());
    }

    #[test]
    fn json_rpc_response_success_serializes_result_only() {
        let res = JsonRpcResponse {
            jsonrpc: String::from("2.0"),
            result: Some(serde_json::json!({"capabilities": ["a"]})),
            error: None,
            id: serde_json::json!(7),
        };
        let v = serde_json::to_value(&res).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["result"]["capabilities"], serde_json::json!(["a"]));
        assert!(v.get("error").is_none());
        assert_eq!(v["id"], serde_json::json!(7));
    }

    #[test]
    fn json_rpc_response_error_serializes_error_only() {
        let res = JsonRpcResponse {
            jsonrpc: String::from("2.0"),
            result: None,
            error: Some(JsonRpcError::invalid_request("missing jsonrpc")),
            id: serde_json::Value::Null,
        };
        let v = serde_json::to_value(&res).unwrap();
        assert!(v.get("result").is_none());
        assert_eq!(v["error"]["code"], JsonRpcError::INVALID_REQUEST);
        assert_eq!(v["error"]["message"], "missing jsonrpc");
    }

    #[test]
    fn json_rpc_error_method_not_found() {
        let e = JsonRpcError::method_not_found("foo.bar");
        assert_eq!(e.code, JsonRpcError::METHOD_NOT_FOUND);
        assert!(e.message.contains("foo.bar"));
        assert!(e.data.is_none());
    }

    #[test]
    fn json_rpc_error_invalid_request() {
        let e = JsonRpcError::invalid_request("");
        assert_eq!(e.code, JsonRpcError::INVALID_REQUEST);
        assert!(e.message.is_empty());
    }

    #[test]
    fn json_rpc_error_invalid_params() {
        let e = JsonRpcError::invalid_params("expected object");
        assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
        assert_eq!(e.message, "expected object");
    }
}
