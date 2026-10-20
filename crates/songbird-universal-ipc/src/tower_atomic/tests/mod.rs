// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Unit and integration tests for [`tower_atomic`](crate::tower_atomic).

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use super::types::{JsonRpcErrorExt, JsonRpcRequestWire, JsonRpcResponseExt, JsonRpcResponseWire};
use super::*;
use crate::endpoint::VirtualEndpoint;
use crate::error::IpcError;
use crate::ipc;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ─── shared handlers ─────────────────────────────────────────────────

struct MathService;

impl JsonRpcHandler for MathService {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "add" => {
                let a = params["a"].as_i64().ok_or("Missing a")?;
                let b = params["b"].as_i64().ok_or("Missing b")?;
                Ok(json!(a + b))
            }
            "multiply" => {
                let a = params["a"].as_i64().ok_or("Missing a")?;
                let b = params["b"].as_i64().ok_or("Missing b")?;
                Ok(json!(a * b))
            }
            _ => Err(format!("Unknown method: {method}")),
        }
    }
}

struct EchoHandler;

impl JsonRpcHandler for EchoHandler {
    async fn handle(&self, _method: &str, params: Value) -> Result<Value, String> {
        Ok(params)
    }
}

// ─── types & constants ───────────────────────────────────────────────

#[test]
fn jsonrpc_version_constant_matches_spec() {
    assert_eq!(JSONRPC_VERSION, "2.0");
}

#[test]
fn json_rpc_request_is_notification_only_without_id() {
    let with_id = JsonRpcRequest::new("m", None, 1);
    assert!(!with_id.is_notification());
    let notification = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: "n".into(),
        params: None,
        id: None,
    };
    assert!(notification.is_notification());
}

#[tokio::test]
async fn test_json_rpc_request_creation() {
    let req = JsonRpcRequest::new("test_method", Some(json!({"key": "value"})), 1);
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "test_method");
    assert_eq!(req.id, Some(json!(1)));
}

#[tokio::test]
async fn test_json_rpc_response_success() {
    let resp = JsonRpcResponse::success(json!(1), json!(42));
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.result, Some(json!(42)));
    assert!(resp.error.is_none());
}

#[tokio::test]
async fn test_json_rpc_response_error() {
    let err = JsonRpcError::internal_error("Test error");
    let resp = JsonRpcResponse::error_from(err, json!(1));
    assert_eq!(resp.jsonrpc, "2.0");
    assert!(resp.result.is_none());
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn test_math_service_handler() {
    let handler = MathService;

    let result = handler.handle("add", json!({"a": 5, "b": 3})).await.expect("Add should succeed");
    assert_eq!(result, json!(8));

    let result =
        handler.handle("multiply", json!({"a": 4, "b": 7})).await.expect("Multiply should succeed");
    assert_eq!(result, json!(28));

    let result = handler.handle("unknown", json!({})).await;
    assert!(result.is_err());
}

#[test]
fn json_rpc_request_serde_roundtrip() {
    let req = JsonRpcRequest::new("echo", Some(json!({"k": 1})), 42);
    let s = serde_json::to_string(&req).expect("serialize request");
    let back: JsonRpcRequest = serde_json::from_str(&s).expect("deserialize request");
    assert_eq!(back.jsonrpc, "2.0");
    assert_eq!(back.method, "echo");
    assert_eq!(back.id, Some(json!(42)));
}

#[test]
fn json_rpc_error_helpers_and_serde() {
    let e = JsonRpcError::method_not_found("m");
    assert_eq!(e.code, JsonRpcError::METHOD_NOT_FOUND);
    let e2 = JsonRpcError::invalid_params("bad");
    assert_eq!(e2.code, JsonRpcError::INVALID_PARAMS);
    let e3 = JsonRpcError::internal_error("x");
    assert_eq!(e3.code, JsonRpcError::INTERNAL_ERROR);
    let v = serde_json::to_value(&e3).expect("to value");
    let back: JsonRpcError = serde_json::from_value(v).expect("from value");
    assert_eq!(back.message, "x");
}

#[test]
fn json_rpc_response_roundtrip() {
    let ok = JsonRpcResponse::success(json!(7), json!(true));
    let s = serde_json::to_string(&ok).expect("ser");
    let back: JsonRpcResponse = serde_json::from_str(&s).expect("de");
    assert_eq!(back.result, Some(json!(true)));

    let err = JsonRpcResponse::error_from(
        JsonRpcError {
            code: JsonRpcError::PARSE_ERROR,
            message: "parse".into(),
            data: None,
        },
        json!(null),
    );
    assert!(err.error.is_some());
}

#[tokio::test]
async fn handle_request_for_test_rejects_non_2_0_jsonrpc() {
    let handler = EchoHandler;
    let req = JsonRpcRequest {
        jsonrpc: "1.0".into(),
        method: "x".into(),
        params: None,
        id: Some(json!(1)),
    };
    let resp = TowerAtomicServer::handle_request_for_test(req, &handler).await;
    let err = resp.error.expect("error response");
    assert_eq!(err.code, JsonRpcError::INVALID_REQUEST);
}

#[tokio::test]
async fn handle_request_for_test_success_wraps_handler_ok() {
    let handler = EchoHandler;
    let req = JsonRpcRequest::new("echo", Some(json!({"a": 1})), 7);
    let resp = TowerAtomicServer::handle_request_for_test(req, &handler).await;
    assert!(resp.error.is_none());
    assert_eq!(resp.result.expect("result"), json!({"a": 1}));
}

#[tokio::test]
async fn handle_request_for_test_wraps_handler_err_as_internal() {
    struct Fail;
    impl JsonRpcHandler for Fail {
        async fn handle(&self, _method: &str, _params: Value) -> Result<Value, String> {
            Err("boom".into())
        }
    }
    let req = JsonRpcRequest::new("m", None, 1);
    let resp = TowerAtomicServer::handle_request_for_test(req, &Fail).await;
    let err = resp.error.expect("rpc error");
    assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);
    assert!(err.message.contains("boom"));
}

#[tokio::test]
async fn handle_request_for_test_omitted_params_become_null() {
    struct NullCheck;
    impl JsonRpcHandler for NullCheck {
        async fn handle(&self, _method: &str, params: Value) -> Result<Value, String> {
            assert!(params.is_null());
            Ok(json!(true))
        }
    }
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: "m".into(),
        params: None,
        id: Some(json!(99)),
    };
    let resp = TowerAtomicServer::handle_request_for_test(req, &NullCheck).await;
    assert_eq!(resp.result, Some(json!(true)));
}

#[test]
fn json_rpc_error_standard_codes_match_spec() {
    assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
    assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
    assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
}

#[test]
fn json_rpc_request_new_uses_numeric_id() {
    let r = JsonRpcRequest::new("ping", None, 1001);
    assert_eq!(r.id, Some(json!(1001)));
    assert!(r.params.is_none());
}

#[tokio::test]
async fn handle_request_invalid_jsonrpc_version_message() {
    struct X;
    impl JsonRpcHandler for X {
        async fn handle(&self, _method: &str, _params: Value) -> Result<Value, String> {
            Ok(json!(0))
        }
    }
    let req = JsonRpcRequest {
        jsonrpc: "2.1".into(),
        method: "m".into(),
        params: Some(json!({})),
        id: Some(json!("abc")),
    };
    let resp = TowerAtomicServer::handle_request_for_test(req, &X).await;
    let e = resp.error.expect("err");
    assert_eq!(e.code, JsonRpcError::INVALID_REQUEST);
}

// ─── fuzz-style serde (JsonRpcRequest) ────────────────────────────────

#[test]
fn deserialize_rejects_malformed_json() {
    assert!(serde_json::from_str::<JsonRpcRequest>("{").is_err());
    assert!(serde_json::from_str::<JsonRpcRequest>("not json").is_err());
    assert!(serde_json::from_str::<JsonRpcRequest>("").is_err());
}

#[test]
fn deserialize_rejects_missing_jsonrpc_field() {
    let s = r#"{"method":"m","id":1}"#;
    assert!(serde_json::from_str::<JsonRpcRequest>(s).is_err());
}

#[test]
fn deserialize_rejects_missing_method_field() {
    let s = r#"{"jsonrpc":"2.0","id":1}"#;
    assert!(serde_json::from_str::<JsonRpcRequest>(s).is_err());
}

#[test]
fn deserialize_accepts_various_id_types() {
    let cases = [
        r#"{"jsonrpc":"2.0","method":"a","id":"s"}"#,
        r#"{"jsonrpc":"2.0","method":"a","id":true}"#,
        r#"{"jsonrpc":"2.0","method":"a","id":[1,2]}"#,
        r#"{"jsonrpc":"2.0","method":"a","id":{"x":1}}"#,
        r#"{"jsonrpc":"2.0","method":"a","id":42}"#,
    ];
    for s in cases {
        let r: JsonRpcRequest = serde_json::from_str(s).unwrap();
        assert_eq!(r.jsonrpc, "2.0");
        assert_eq!(r.method, "a");
        assert!(r.id.is_some(), "id should be present for: {s}");
    }
}

#[test]
fn deserialize_null_id_treated_as_notification() {
    let s = r#"{"jsonrpc":"2.0","method":"a","id":null}"#;
    let r: JsonRpcRequest = serde_json::from_str(s).unwrap();
    assert!(r.is_notification(), "null id is indistinguishable from absent id via serde");
}

#[test]
fn deserialize_notification_omits_id() {
    let s = r#"{"jsonrpc":"2.0","method":"notify.event"}"#;
    let r: JsonRpcRequest = serde_json::from_str(s).unwrap();
    assert!(r.is_notification());
    assert!(r.id.is_none());
}

#[test]
fn deserialize_nested_json_deep_structure() {
    let mut inner = json!({});
    for _ in 0..120 {
        inner = json!({ "k": inner });
    }
    let v = json!({
        "jsonrpc": "2.0",
        "method": "deep",
        "id": 1,
        "params": inner
    });
    let s = v.to_string();
    let r: JsonRpcRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(r.method, "deep");
    assert!(r.params.is_some());
}

#[test]
fn deserialize_very_long_method_name() {
    let long = "x".repeat(50_000);
    let v = json!({
        "jsonrpc": "2.0",
        "method": long,
        "id": 0
    });
    let s = v.to_string();
    let r: JsonRpcRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(r.method.len(), 50_000);
}

#[test]
fn deserialize_unicode_method_name() {
    let s = r#"{"jsonrpc":"2.0","method":"ping.тест.😀","id":1}"#;
    let r: JsonRpcRequest = serde_json::from_str(s).unwrap();
    assert_eq!(r.method, "ping.тест.😀");
}

// ─── integration: real IPC (unique primal per test, concurrent-safe) ───

async fn e2e_setup_math_server() -> (tokio::task::JoinHandle<()>, String, VirtualEndpoint) {
    ipc::init().expect("ipc init");
    let name = format!("ta-math-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec!["math".to_string()]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(MathService);
    let handle = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("server ready");
    (handle, name, endpoint)
}

#[tokio::test]
async fn e2e_client_call_and_call_no_params_round_trip() {
    let (h, name, _ep) = e2e_setup_math_server().await;
    let path = format!("/primal/{name}");
    let client = TowerAtomicClient::connect(&path).await.expect("connect");

    let sum = client.call("add", json!({"a": 10, "b": 20})).await.expect("add");
    assert_eq!(sum, json!(30));

    let product = client.call("multiply", json!({"a": 3, "b": 4})).await.expect("multiply");
    assert_eq!(product, json!(12));

    let nullish = client.call_no_params("add").await;
    assert!(nullish.is_err());

    h.abort();
}

#[tokio::test]
async fn e2e_client_maps_handler_error_to_rpc_error() {
    let (h, name, _) = e2e_setup_math_server().await;
    let path = format!("/primal/{name}");
    let client = TowerAtomicClient::connect(&path).await.expect("connect");

    let err = client.call("unknown", json!({})).await.expect_err("unknown method");
    match err {
        IpcError::RpcError(msg) => assert!(msg.contains("Unknown method")),
        other => panic!("expected RpcError, got {other:?}"),
    }

    h.abort();
}

#[tokio::test]
async fn connect_unregistered_path_fails() {
    ipc::init().expect("init");
    let ghost = format!("/primal/ghost-{}", uuid::Uuid::new_v4());
    match TowerAtomicClient::connect(&ghost).await {
        Err(e) => assert!(matches!(e, IpcError::ServiceNotFound(_))),
        Ok(_) => panic!("expected ServiceNotFound for unregistered path"),
    }
}

#[tokio::test]
async fn e2e_client_rejects_malformed_json_response() {
    ipc::init().expect("init");
    let name = format!("ta-badjson-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_listen = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let raw = tokio::spawn(async move {
        let mut listener = ipc::listen(ep_listen).await.expect("listen");
        let _ = ready_tx.send(());
        let mut stream = listener.accept().await.expect("accept");
        let mut line = String::new();
        BufReader::new(&mut stream).read_line(&mut line).await.ok();
        stream.write_all(b"NOT VALID JSON\n").await.expect("write garbage");
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let client = TowerAtomicClient::connect(&path).await.expect("connect");
    let err = client.call("x", json!({})).await.expect_err("parse");
    match err {
        IpcError::Other(msg) => assert!(msg.contains("Failed to parse response")),
        e => panic!("unexpected {e:?}"),
    }
    raw.abort();
}

#[tokio::test]
async fn e2e_client_rejects_success_without_result_field() {
    ipc::init().expect("init");
    let name = format!("ta-noresult-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_listen = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let raw = tokio::spawn(async move {
        let mut listener = ipc::listen(ep_listen).await.expect("listen");
        let _ = ready_tx.send(());
        let mut stream = listener.accept().await.expect("accept");
        let mut line = String::new();
        BufReader::new(&mut stream).read_line(&mut line).await.ok();
        stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1}\n").await.expect("write no result");
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let client = TowerAtomicClient::connect(&path).await.expect("connect");
    let err = client.call("x", json!({})).await.expect_err("missing result");
    match err {
        IpcError::Other(msg) => assert!(msg.contains("Missing result")),
        e => panic!("unexpected {e:?}"),
    }
    raw.abort();
}

#[tokio::test]
async fn e2e_server_returns_parse_error_for_invalid_request_line() {
    ipc::init().expect("init");
    let name = format!("ta-parse-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(EchoHandler);
    let h = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let mut stream = ipc::connect(&path).await.expect("connect");
    stream.write_all(b"{not json\n").await.expect("send bad");
    let mut line = String::new();
    BufReader::new(&mut stream).read_line(&mut line).await.expect("read err line");
    let resp: JsonRpcResponse = serde_json::from_str(line.trim()).expect("resp json");
    let err = resp.error.expect("parse error object");
    assert_eq!(err.code, JsonRpcError::PARSE_ERROR);
    h.abort();
}

#[tokio::test(start_paused = true)]
async fn e2e_notification_does_not_emit_response_line() {
    ipc::init().expect("init");
    let name = format!("ta-notif-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(EchoHandler);
    let h = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let mut stream = ipc::connect(&path).await.expect("connect");
    stream.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n").await.expect("notify");

    let mut reader = BufReader::new(&mut stream);
    let mut out = String::new();
    let read_fut = reader.read_line(&mut out);
    let timeout_fut = tokio::time::timeout(Duration::from_millis(500), read_fut);
    tokio::time::advance(Duration::from_millis(600)).await;
    let timed = timeout_fut.await;
    assert!(timed.is_err(), "no line should be written for notifications");
    h.abort();
}

#[tokio::test]
async fn e2e_server_skips_blank_lines_between_requests() {
    ipc::init().expect("init");
    let name = format!("ta-blank-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(MathService);
    let h = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let mut stream = ipc::connect(&path).await.expect("connect");
    stream.write_all(b"\n\n").await.expect("blanks");
    stream
        .write_all(
            serde_json::to_string(&JsonRpcRequest::new("add", Some(json!({"a":1,"b":2})), 9))
                .unwrap()
                .as_bytes(),
        )
        .await
        .expect("req");
    stream.write_all(b"\n").await.expect("nl");
    let mut line = String::new();
    BufReader::new(&mut stream).read_line(&mut line).await.expect("read");
    let resp: JsonRpcResponse = serde_json::from_str(line.trim()).expect("resp");
    assert_eq!(resp.result, Some(json!(3)));
    h.abort();
}

// ─── wire types (mirrors [`TowerAtomicClient::call`] parsing) ─────────────────

#[test]
fn json_rpc_response_wire_rejects_malformed_json() {
    assert!(serde_json::from_str::<JsonRpcResponseWire<'_>>("not json").is_err());
    assert!(serde_json::from_str::<JsonRpcResponseWire<'_>>("{").is_err());
}

#[test]
fn json_rpc_response_wire_json_null_result_maps_to_none_for_option_value() {
    let line = r#"{"jsonrpc":"2.0","result":null,"id":3}"#;
    let w: JsonRpcResponseWire<'_> = serde_json::from_str(line).expect("parse");
    assert!(w.error.is_none());
    assert!(
        w.result.is_none(),
        "serde maps JSON null to None for Option<Value> (same as omitted result field)"
    );
}

#[test]
fn json_rpc_response_wire_non_null_scalar_result_round_trips() {
    let line = r#"{"jsonrpc":"2.0","result":false,"id":"rid"}"#;
    let w: JsonRpcResponseWire<'_> = serde_json::from_str(line).expect("parse");
    assert_eq!(w.result, Some(json!(false)));
    assert_eq!(w.id, json!("rid"));
}

#[test]
fn json_rpc_response_wire_borrows_error_message() {
    let line = r#"{"jsonrpc":"2.0","error":{"code":-32000,"message":"custom fail"},"id":1}"#;
    let w: JsonRpcResponseWire<'_> = serde_json::from_str(line).expect("parse");
    assert!(w.result.is_none());
    let err = w.error.expect("error");
    assert_eq!(err.message.as_ref(), "custom fail");
}

#[test]
fn json_rpc_response_wire_omitted_result_is_none() {
    let line = r#"{"jsonrpc":"2.0","id":7}"#;
    let w: JsonRpcResponseWire<'_> = serde_json::from_str(line).expect("parse");
    assert!(w.result.is_none());
    assert!(w.error.is_none());
}

#[test]
fn json_rpc_response_wire_requires_id_field() {
    let line = r#"{"jsonrpc":"2.0","result":1}"#;
    assert!(serde_json::from_str::<JsonRpcResponseWire<'_>>(line).is_err());
}

#[test]
fn json_rpc_request_wire_borrows_method_and_params() {
    let line = r#"{"jsonrpc":"2.0","method":"m.add","params":{"a":1},"id":2}"#;
    let w: JsonRpcRequestWire<'_> = serde_json::from_str(line).expect("parse");
    assert_eq!(w.method.as_ref(), "m.add");
    assert_eq!(w.jsonrpc.as_ref(), "2.0");
    assert_eq!(w.params, Some(json!({"a": 1})));
}

#[test]
fn json_rpc_request_wire_unicode_method_name() {
    let line = r#"{"jsonrpc":"2.0","method":"π.🔧","params":[],"id":0}"#;
    let w: JsonRpcRequestWire<'_> = serde_json::from_str(line).expect("parse");
    assert_eq!(w.method.as_ref(), "π.🔧");
}

#[cfg(unix)]
#[tokio::test]
async fn connect_unix_path_missing_socket_is_connection_failed() {
    let path = std::env::temp_dir().join(format!("songbird-ta-nosock-{}", uuid::Uuid::new_v4()));
    let Err(err) = TowerAtomicClient::connect_unix_path(&path).await else {
        panic!("expected connection failure for missing socket");
    };
    match err {
        IpcError::ConnectionFailed(m) => {
            assert!(
                m.contains("Failed to connect") || m.contains("connect"),
                "unexpected message: {m}"
            );
        }
        e => panic!("expected ConnectionFailed, got {e:?}"),
    }
}

#[tokio::test]
async fn e2e_concurrent_clients_on_one_server() {
    ipc::init().expect("ipc init");
    let name = format!("ta-conc-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec!["math".to_string()]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(MathService);
    let h = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("ready");
    let path = std::sync::Arc::new(format!("/primal/{name}"));
    let futs: Vec<_> = (0..12_u64)
        .map(|i| {
            let p = path.clone();
            async move {
                let client = TowerAtomicClient::connect(p.as_str()).await.expect("connect");
                client.call("add", json!({"a": i as i64, "b": 1})).await.expect("add")
            }
        })
        .collect();
    let results = futures_util::future::join_all(futs).await;
    assert_eq!(results.len(), 12);
    for (i, v) in results.iter().enumerate() {
        assert_eq!(*v, json!(i as i64 + 1));
    }
    h.abort();
}

#[tokio::test]
async fn e2e_oversized_single_line_json_still_parses() {
    ipc::init().expect("init");
    let name = format!("ta-bigline-{}", uuid::Uuid::new_v4());
    let endpoint = ipc::register(&name, vec![]).await.expect("register");
    let ep_clone = endpoint.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server = TowerAtomicServer::new(MathService);
    let h = tokio::spawn(async move {
        let _ = server.serve_with_ready(ep_clone, ready_tx).await;
    });
    ready_rx.await.expect("ready");
    let path = format!("/primal/{name}");
    let mut stream = ipc::connect(&path).await.expect("connect");
    let pad = "z".repeat(200_000);
    let body = serde_json::to_string(&JsonRpcRequest::new(
        "add",
        Some(json!({"a": 4_i64, "b": 5_i64, "pad": pad})),
        1,
    ))
    .expect("serialize");
    assert!(body.len() > 150_000, "sanity: large request line");
    stream.write_all(body.as_bytes()).await.expect("write");
    stream.write_all(b"\n").await.expect("nl");
    let mut line = String::new();
    BufReader::new(&mut stream).read_line(&mut line).await.expect("read");
    let resp: JsonRpcResponse = serde_json::from_str(line.trim()).expect("resp");
    assert_eq!(resp.result, Some(json!(9)));
    h.abort();
}
