// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower::ServiceExt;

use super::handlers::{
    handle_beacon_exchange, handle_compute_job_status, handle_compute_route, handle_consent_check,
    handle_consent_grant, handle_deployment_create, handle_deployment_status,
    handle_federation_join, handle_federation_peers, handle_health, handle_health_standard,
    handle_identity, handle_protocol_capabilities, handle_protocol_negotiate_semantic,
    handle_registry_discover, handle_registry_register, handle_service_get,
    handle_service_register, handle_services_list, handle_task_create, handle_task_list,
    handle_version,
};
use super::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcState, jsonrpc_routes,
};
use crate::consent_management::ConsentManager;
use crate::server::compute_api::ComputeApiState;
use crate::server::deployment_api::DeploymentState;
use crate::server::protocol_api::ProtocolApiState;
use crate::service_registry::ServiceRegistry;
use crate::task_lifecycle::{Priority, ResourceRequirements, TaskLifecycleManager, TaskSpec};
use songbird_network_federation::service_registry::FederatedServiceRegistry;
use songbird_network_federation::state::FederationState;

async fn jsonrpc_test_state() -> JsonRpcState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("tasks.db").to_string_lossy().to_string();
    let task_manager =
        Arc::new(TaskLifecycleManager::new(&db_path).await.expect("task lifecycle db"));

    let federation_state = Arc::new(FederationState::default());
    let federated_registry = Arc::new(FederatedServiceRegistry::new());

    JsonRpcState {
        federation_state: Arc::clone(&federation_state),
        service_registry: Arc::clone(&federated_registry),
        compute_state: ComputeApiState::new(
            Arc::clone(&federation_state),
            Arc::clone(&federated_registry),
        ),
        deployment_state: DeploymentState::new(),
        protocol_state: ProtocolApiState::new(),
        upa_registry: Arc::new(ServiceRegistry::new()),
        task_manager,
        consent_manager: Arc::new(ConsentManager::new()),
        start_time: Arc::new(RwLock::new(Instant::now())),
        ipc_handler: None,
    }
}

#[test]
fn test_jsonrpc_error_codes() {
    assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
    assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
    assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
    assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
    assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
}

#[test]
fn test_jsonrpc_error_constructors_roundtrip() {
    let e = JsonRpcError::parse_error();
    assert_eq!(e.code, JsonRpcError::PARSE_ERROR);

    let e = JsonRpcError::invalid_request("x");
    assert_eq!(e.code, JsonRpcError::INVALID_REQUEST);
    assert_eq!(e.message, "x");

    let e = JsonRpcError::invalid_params("p");
    assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);

    let e = JsonRpcError::internal_error("i");
    assert_eq!(e.code, JsonRpcError::INTERNAL_ERROR);
}

#[test]
fn test_jsonrpc_error_creation() {
    let error = JsonRpcError::method_not_found("test.method");
    assert_eq!(error.code, JsonRpcError::METHOD_NOT_FOUND);
    assert!(error.message.contains("test.method"));
}

#[test]
fn test_jsonrpc_request_deserialization() {
    let json = r#"{
            "jsonrpc": "2.0",
            "method": "songbird.health",
            "id": 1
        }"#;

    let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "songbird.health");
    assert!(request.id.is_some());
}

#[test]
fn test_jsonrpc_response_serialization() {
    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: Some(serde_json::json!({"status": "ok"})),
        error: None,
        id: Value::Number(1.into()),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"result\""));
    assert!(!json.contains("\"error\""));
}

#[tokio::test]
async fn compute_route_rejects_missing_params() {
    let state = jsonrpc_test_state().await;
    let err = handle_compute_route(&state, None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn compute_route_rejects_invalid_json() {
    let state = jsonrpc_test_state().await;
    let err =
        handle_compute_route(&state, Some(json!({"not": "a_compute_task"}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn compute_job_status_rejects_bad_uuid() {
    let state = jsonrpc_test_state().await;
    let err = handle_compute_job_status(&state, Some(json!({"task_id": "not-a-uuid"})))
        .await
        .unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn deployment_create_requires_object() {
    let state = jsonrpc_test_state().await;
    let err = handle_deployment_create(&state, Some(json!("array"))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn deployment_create_rejects_invalid_base64() {
    let state = jsonrpc_test_state().await;
    let err = handle_deployment_create(
        &state,
        Some(json!({
            "binary_base64": "@@@not-base64@@@",
            "service_name": "svc"
        })),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn deployment_status_requires_id() {
    let state = jsonrpc_test_state().await;
    let err = handle_deployment_status(&state, Some(json!({}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn task_create_rejects_invalid_body() {
    let state = jsonrpc_test_state().await;
    let err = handle_task_create(&state, Some(json!({"owner": 1}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn task_list_accepts_empty_params() {
    let state = jsonrpc_test_state().await;
    let v = handle_task_list(&state, None).await.expect("list");
    assert!(v.get("tasks").is_some());
}

#[tokio::test]
async fn consent_check_not_found() {
    let state = jsonrpc_test_state().await;
    let err =
        handle_consent_check(&state, Some(json!({"consent_id": "missing"}))).await.unwrap_err();
    assert_eq!(err.code, -32001);
}

#[tokio::test]
async fn consent_grant_missing_consent_id() {
    let state = jsonrpc_test_state().await;
    let err = handle_consent_grant(&state, Some(json!({"reason": "ok"}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn consent_grant_unknown_id() {
    let state = jsonrpc_test_state().await;
    let err = handle_consent_grant(&state, Some(json!({"consent_id": "nope", "reason": "ok"})))
        .await
        .unwrap_err();
    assert_eq!(err.code, -32001);
}

#[tokio::test]
async fn protocol_negotiate_semantic_happy_path() {
    let state = jsonrpc_test_state().await;
    let params = json!({
        "client_id": "c1",
        "client_protocols": ["http", "json-rpc"],
        "preferred": "json-rpc"
    });
    let v = handle_protocol_negotiate_semantic(&state, Some(params)).await.expect("negotiate");
    assert!(v.get("selected_protocol").is_some());
}

#[tokio::test]
async fn protocol_negotiate_rejects_missing_params() {
    let state = jsonrpc_test_state().await;
    let err = handle_protocol_negotiate_semantic(&state, None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn services_list_empty() {
    let state = jsonrpc_test_state().await;
    let v = handle_services_list(&state).await.expect("list");
    assert_eq!(v["count"], 0);
}

#[tokio::test]
async fn service_get_missing_id() {
    let state = jsonrpc_test_state().await;
    let err = handle_service_get(&state, None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn service_get_unknown() {
    let state = jsonrpc_test_state().await;
    let err = handle_service_get(&state, Some(json!({"service_id": "nope"}))).await.unwrap_err();
    assert_eq!(err.code, -32001);
}

#[tokio::test]
async fn service_register_validation() {
    let state = jsonrpc_test_state().await;
    let err = handle_service_register(&state, Some(json!({"service_id": "s"}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn registry_register_missing_params() {
    let state = jsonrpc_test_state().await;
    let err = handle_registry_register(&state, None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn registry_register_roundtrip() {
    let state = jsonrpc_test_state().await;
    let params = json!({
        "primal_name": "p",
        "primal_version": "1",
        "capabilities": [{
            "name": "cap",
            "type": "t",
            "metadata": {}
        }],
        "protocols": ["http"],
        "preferred_protocol": "http"
    });
    let v = handle_registry_register(&state, Some(params)).await.expect("register");
    assert_eq!(v["status"], "registered");
}

#[tokio::test]
async fn registry_discover_list_and_by_capability() {
    let state = jsonrpc_test_state().await;
    let v = handle_registry_discover(&state, None).await.expect("discover");
    assert!(v.get("services").is_some());

    let v2 = handle_registry_discover(&state, Some(json!({"capability": "ai:test"})))
        .await
        .expect("by cap");
    assert_eq!(v2["capability"], "ai:test");
}

#[tokio::test]
async fn federation_join_and_peers() {
    let state = jsonrpc_test_state().await;
    let err = handle_federation_join(&state, None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);

    let v = handle_federation_join(
        &state,
        Some(json!({
            "node_id": "n1",
            "name": "Node One",
            "address": "10.0.0.1:8080",
            "capabilities": ["x"]
        })),
    )
    .await
    .expect("join");
    assert_eq!(v["status"], "joined");

    let peers = handle_federation_peers(&state).await.expect("peers");
    assert!(peers["count"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn federation_health_handlers() {
    let state = jsonrpc_test_state().await;
    let caps = handle_protocol_capabilities().await.expect("caps");
    assert!(caps.get("protocols").is_some());

    let health = handle_health(&state).await.expect("health");
    assert_eq!(health["status"], "healthy");

    let std_health = handle_health_standard(&state).await.expect("std");
    let status = std_health["status"].as_str().expect("status is a string");
    assert!(
        status == "healthy" || status == "degraded",
        "health status should reflect real subsystem state: got {status}"
    );

    let ver = handle_version().await.expect("version");
    assert!(ver.get("version").is_some());

    let id = handle_identity().await.expect("identity");
    assert_eq!(id["primal"], "songbird");
}

#[tokio::test]
async fn beacon_exchange_paths() {
    let err = handle_beacon_exchange(None).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);

    let err = handle_beacon_exchange(Some(json!({}))).await.unwrap_err();
    assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);

    let v = handle_beacon_exchange(Some(json!({
        "beacon": {"x": 1},
        "peer_address": "10.0.0.2"
    })))
    .await
    .expect("beacon");
    assert_eq!(v["status"], "received");
}

#[tokio::test]
async fn task_create_and_list_filter() {
    let state = jsonrpc_test_state().await;
    let spec = TaskSpec {
        task_type: "test-type".into(),
        config: json!({}),
        required_capabilities: vec![],
        resources: ResourceRequirements::default(),
        priority: Priority::Standard,
    };
    let created = handle_task_create(
        &state,
        Some(json!({
            "owner": "user-a",
            "spec": spec
        })),
    )
    .await
    .expect("create");
    let task_id = created["task_id"].as_str().expect("task_id");

    let listed = handle_task_list(&state, Some(json!({"owner": "user-a"}))).await.expect("list");
    let tasks = listed["tasks"].as_array().expect("tasks");
    assert!(tasks.iter().any(|t| t["id"].as_str() == Some(task_id)), "expected task in list");
}

#[tokio::test]
async fn jsonrpc_http_wrong_version_returns_invalid_request() {
    let state = jsonrpc_test_state().await;
    let app = jsonrpc_routes().with_state(state);

    let body = json!({
        "jsonrpc": "1.0",
        "method": "songbird.health",
        "id": 42
    })
    .to_string();

    let req = Request::builder()
        .method("POST")
        .uri("/")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.expect("body");
    let resp: Value = serde_json::from_slice(&bytes).expect("json");
    let err = resp["error"].as_object().expect("error object");
    assert_eq!(err["code"], JsonRpcError::INVALID_REQUEST);
    assert!(err["message"].as_str().expect("message").to_lowercase().contains("jsonrpc"));
}

#[tokio::test]
async fn jsonrpc_unknown_method_without_ipc() {
    let state = jsonrpc_test_state().await;
    let app = jsonrpc_routes().with_state(state);

    let body = json!({
        "jsonrpc": "2.0",
        "method": "totally.unknown.method",
        "id": 7
    })
    .to_string();

    let req = Request::builder()
        .method("POST")
        .uri("/rpc")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let res = app.oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.expect("body");
    let resp: Value = serde_json::from_slice(&bytes).expect("json");
    let err = resp["error"].as_object().expect("error object");
    assert_eq!(err["code"], JsonRpcError::METHOD_NOT_FOUND);
}
