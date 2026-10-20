// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![allow(
    clippy::clone_on_ref_ptr,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test assertions"
)]
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unnecessary_wraps,
    clippy::await_holding_lock,
    clippy::float_cmp,
    clippy::absurd_extreme_comparisons,
    clippy::needless_collect,
    clippy::nonminimal_bool,
    clippy::used_underscore_binding,
    clippy::field_reassign_with_default,
    clippy::return_self_not_must_use,
    clippy::overly_complex_bool_expr,
    clippy::assertions_on_constants,
    clippy::no_effect_underscore_binding,
    clippy::items_after_statements,
    clippy::empty_line_after_doc_comments,
    clippy::const_is_empty,
    clippy::duplicated_attributes,
    deprecated,
    dead_code,
    clippy::unnecessary_literal_unwrap,
    clippy::needless_pass_by_value,
    clippy::must_use_candidate,
    reason = "test assertions and harness ergonomics"
)]

//! Coverage tests for songbird-universal-ipc::tower_atomic
//!
//! Tests the JSON-RPC 2.0 types and protocol structures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test assertions and harness ergonomics"
)]

use songbird_universal_ipc::tower_atomic::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt,
};

// ==================== JSON-RPC REQUEST ====================

#[test]
fn test_jsonrpc_request_new() {
    let req = JsonRpcRequest::new("test.method", Some(serde_json::json!({"key": "val"})), 42);
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "test.method");
    assert_eq!(req.id, Some(serde_json::json!(42)));
    assert!(req.params.is_some());
}

#[test]
fn test_jsonrpc_request_no_params() {
    let req = JsonRpcRequest::new("no_params", None, 1);
    assert_eq!(req.method, "no_params");
    assert!(req.params.is_none());
}

#[test]
fn test_jsonrpc_request_serialization() {
    let req = JsonRpcRequest::new("test", Some(serde_json::json!({"a": 1})), 99);
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"test\""));

    let de: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(de.method, "test");
    assert_eq!(de.id, Some(serde_json::json!(99)));
}

#[test]
fn test_jsonrpc_request_params_skipped_when_none() {
    let req = JsonRpcRequest::new("test", None, 1);
    let json = serde_json::to_string(&req).unwrap();
    assert!(!json.contains("\"params\""));
}

#[test]
fn test_jsonrpc_request_clone() {
    let req = JsonRpcRequest::new("clone_test", Some(serde_json::json!({"x": 42})), 7);
    let cloned = req.clone();
    assert_eq!(req.method, cloned.method);
    assert_eq!(req.id, cloned.id);
    assert_eq!(req.params, cloned.params);
}

#[test]
fn test_jsonrpc_request_debug() {
    let req = JsonRpcRequest::new("debug_test", None, 1);
    let debug = format!("{req:?}");
    assert!(debug.contains("JsonRpcRequest"));
    assert!(debug.contains("debug_test"));
}

// ==================== JSON-RPC RESPONSE ====================

#[test]
fn test_jsonrpc_response_success() {
    let resp = JsonRpcResponse::success(serde_json::json!(1), serde_json::json!(42));
    assert_eq!(resp.jsonrpc, "2.0");
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap(), serde_json::json!(42));
}

#[test]
fn test_jsonrpc_response_error() {
    let err = JsonRpcError::method_not_found("unknown.method");
    let resp = JsonRpcResponse::error_from(err, serde_json::json!(1));
    assert_eq!(resp.jsonrpc, "2.0");
    assert!(resp.result.is_none());
    assert!(resp.error.is_some());
    let error = resp.error.unwrap();
    assert_eq!(error.code, JsonRpcError::METHOD_NOT_FOUND);
}

#[test]
fn test_jsonrpc_response_serialization_success() {
    let resp = JsonRpcResponse::success(serde_json::json!(5), serde_json::json!("ok"));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"result\":\"ok\""));
    assert!(!json.contains("\"error\""));

    let de: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(de.result, Some(serde_json::json!("ok")));
}

#[test]
fn test_jsonrpc_response_serialization_error() {
    let err = JsonRpcError::internal_error("boom");
    let resp = JsonRpcResponse::error_from(err, serde_json::json!(3));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"error\""));
    assert!(!json.contains("\"result\""));

    let de: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert!(de.error.is_some());
    assert_eq!(de.error.unwrap().code, JsonRpcError::INTERNAL_ERROR);
}

#[test]
fn test_jsonrpc_response_clone() {
    let resp = JsonRpcResponse::success(serde_json::json!(1), serde_json::json!({"a": 1}));
    let cloned = resp.clone();
    assert_eq!(resp.result, cloned.result);
    assert_eq!(resp.id, cloned.id);
}

// ==================== JSON-RPC ERROR ====================

#[test]
fn test_jsonrpc_error_codes() {
    assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
    assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
    assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
    assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
    assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
}

#[test]
fn test_jsonrpc_error_method_not_found() {
    let err = JsonRpcError::method_not_found("test.missing");
    assert_eq!(err.code, -32601);
    assert!(err.message.contains("test.missing"));
    assert!(err.data.is_none());
}

#[test]
fn test_jsonrpc_error_invalid_params() {
    let err = JsonRpcError::invalid_params("expected integer, got string");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("expected integer"));
    assert!(err.data.is_none());
}

#[test]
fn test_jsonrpc_error_internal_error() {
    let err = JsonRpcError::internal_error("database connection failed");
    assert_eq!(err.code, -32603);
    assert!(err.message.contains("database connection failed"));
    assert!(err.data.is_none());
}

#[test]
fn test_jsonrpc_error_serialization() {
    let err = JsonRpcError {
        code: -32000,
        message: "custom error".to_string(),
        data: Some(serde_json::json!({"details": "more info"})),
    };
    let json = serde_json::to_string(&err).unwrap();
    let de: JsonRpcError = serde_json::from_str(&json).unwrap();
    assert_eq!(de.code, -32000);
    assert_eq!(de.message, "custom error");
    assert!(de.data.is_some());
}

#[test]
fn test_jsonrpc_error_data_skipped_when_none() {
    let err = JsonRpcError::internal_error("no data");
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.contains("\"data\""));
}

#[test]
fn test_jsonrpc_error_clone() {
    let err = JsonRpcError::method_not_found("clone_test");
    let cloned = err.clone();
    assert_eq!(err.code, cloned.code);
    assert_eq!(err.message, cloned.message);
}

#[test]
fn test_jsonrpc_error_debug() {
    let err = JsonRpcError::invalid_params("debug test");
    let debug = format!("{err:?}");
    assert!(debug.contains("JsonRpcError"));
    assert!(debug.contains("debug test"));
}

// ==================== COMPLEX SCENARIOS ====================

#[test]
fn test_jsonrpc_request_with_complex_params() {
    let params = serde_json::json!({
        "array": [1, 2, 3],
        "nested": {"a": {"b": {"c": true}}},
        "null_val": null,
        "float": 1.25
    });
    let req = JsonRpcRequest::new("complex", Some(params.clone()), 1);
    let json = serde_json::to_string(&req).unwrap();
    let de: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(de.params.unwrap(), params);
}

#[test]
fn test_jsonrpc_response_with_null_result() {
    let resp = JsonRpcResponse::success(serde_json::json!(1), serde_json::Value::Null);
    assert!(resp.result.is_some());
    assert_eq!(resp.result.unwrap(), serde_json::Value::Null);
}

#[test]
fn test_jsonrpc_batch_operations() {
    // Simulate batch processing
    let requests: Vec<JsonRpcRequest> = (1..=5)
        .map(|i| JsonRpcRequest::new(format!("method_{i}"), Some(serde_json::json!({"i": i})), i))
        .collect();

    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].method, "method_1");
    assert_eq!(requests[4].method, "method_5");

    let json = serde_json::to_string(&requests).unwrap();
    let de: Vec<JsonRpcRequest> = serde_json::from_str(&json).unwrap();
    assert_eq!(de.len(), 5);
}

#[test]
fn test_jsonrpc_roundtrip_error_response() {
    let err = JsonRpcError {
        code: -32000,
        message: "Server error".to_string(),
        data: Some(serde_json::json!({
            "stack_trace": "line 42",
            "context": {"file": "main.rs"}
        })),
    };
    let resp = JsonRpcResponse::error_from(err, serde_json::json!("str-id"));
    let json = serde_json::to_string(&resp).unwrap();
    let de: JsonRpcResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, serde_json::json!("str-id"));
    assert!(de.error.unwrap().data.is_some());
}
