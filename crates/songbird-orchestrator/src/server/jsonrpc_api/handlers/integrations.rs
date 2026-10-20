// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

use axum::{Json, extract::Path};
use base64::Engine;
use serde_json::Value;

use super::super::JsonRpcState;
use super::super::types::{JsonRpcError, JsonRpcErrorExt};
use super::common::{extract_str_param, jsonrpc_code_from_http_status, jsonrpc_from_compute_error};

/// `compute.route` / `songbird.compute.schedule` — same handler as `POST /api/compute/task`
pub async fn handle_compute_route(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing parameters"))?;
    let req: crate::server::compute_api::ComputeTaskRequest =
        serde_json::from_value(params).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;
    let json = crate::server::compute_api::submit_compute_task(
        axum::extract::State(state.compute_state.clone()),
        Json(req),
    )
    .await
    .map_err(jsonrpc_from_compute_error)?;
    serde_json::to_value(json.0).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `songbird.compute.status` — same as `GET /api/compute/task/:job_id`
pub async fn handle_compute_job_status(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let task_id_str = extract_str_param(params.as_ref(), "task_id")?;
    let job_id = uuid::Uuid::parse_str(&task_id_str)
        .map_err(|_| JsonRpcError::invalid_params("task_id must be a UUID"))?;
    let res = crate::server::compute_api::get_task_status(
        axum::extract::State(state.compute_state.clone()),
        Path(job_id),
    )
    .await
    .map_err(jsonrpc_from_compute_error)?;
    serde_json::to_value(res.0).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `deployment.create` — same deployment path as `POST /api/deployment/binary` (body as base64 in JSON)
pub async fn handle_deployment_create(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let obj = params
        .as_ref()
        .and_then(|p| p.as_object())
        .ok_or_else(|| JsonRpcError::invalid_params("Parameters must be an object"))?;
    let b64 = obj
        .get("binary_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing binary_base64"))?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid base64: {e}")))?;
    let service_name = obj.get("service_name").and_then(|v| v.as_str()).map(String::from);
    let env_vars: std::collections::HashMap<String, String> = obj
        .get("env_vars")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let auto_start = obj.get("auto_start").and_then(Value::as_bool).unwrap_or(true);
    let (status, body) = crate::server::deployment_api::deploy_binary_bytes(
        &state.deployment_state,
        axum::body::Bytes::from(raw),
        service_name,
        env_vars,
        auto_start,
    )
    .await
    .map_err(|(code, msg)| JsonRpcError {
        code: jsonrpc_code_from_http_status(code),
        message: msg,
        data: None,
    })?;
    let mut val =
        serde_json::to_value(&body).map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    if let Some(o) = val.as_object_mut() {
        o.insert(String::from("http_status"), (status.as_u16()).into());
    }
    Ok(val)
}

/// `deployment.status` — same as `GET /api/deployment/status/:id`
pub async fn handle_deployment_status(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let id = extract_str_param(params.as_ref(), "deployment_id")?;
    match crate::server::deployment_api::get_deployment_status(
        axum::extract::State(state.deployment_state.clone()),
        Path(id),
    )
    .await
    {
        Ok(Json(info)) => {
            serde_json::to_value(info).map_err(|e| JsonRpcError::internal_error(e.to_string()))
        }
        Err((code, msg)) => Err(JsonRpcError {
            code: jsonrpc_code_from_http_status(code),
            message: msg,
            data: None,
        }),
    }
}

/// `deployment.hot_swap` — stop old process → replace binary → start new.
///
/// Params: `{ "deployment_id": "...", "binary_base64": "..." }`
pub async fn handle_deployment_hot_swap(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let obj = params
        .as_ref()
        .and_then(|p| p.as_object())
        .ok_or_else(|| JsonRpcError::invalid_params("Parameters must be an object"))?;
    let deployment_id = obj
        .get("deployment_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing deployment_id"))?;
    let b64 = obj
        .get("binary_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing binary_base64"))?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid base64: {e}")))?;

    let info = crate::server::deployment_api::hot_swap_deployment(
        &state.deployment_state,
        deployment_id,
        axum::body::Bytes::from(raw),
    )
    .await
    .map_err(|(code, msg)| JsonRpcError {
        code: jsonrpc_code_from_http_status(code),
        message: msg,
        data: None,
    })?;

    serde_json::to_value(info).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `deployment.restart` — restart an existing deployment (same binary).
///
/// Params: `{ "deployment_id": "..." }`
pub async fn handle_deployment_restart(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let deployment_id = extract_str_param(params.as_ref(), "deployment_id")?;

    let info =
        crate::server::deployment_api::restart_deployment(&state.deployment_state, &deployment_id)
            .await
            .map_err(|(code, msg)| JsonRpcError {
                code: jsonrpc_code_from_http_status(code),
                message: msg,
                data: None,
            })?;

    serde_json::to_value(info).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `deployment.list` — list all active deployments.
pub async fn handle_deployment_list(state: &JsonRpcState) -> Result<Value, JsonRpcError> {
    let list = crate::server::deployment_api::list_deployments_vec(&state.deployment_state).await;
    serde_json::to_value(list).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `task.create` — same as `POST /api/v1/tasks`
pub async fn handle_task_create(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let req: crate::server::task_api::CreateTaskRequest = serde_json::from_value(
        params.ok_or_else(|| JsonRpcError::invalid_params("Missing parameters"))?,
    )
    .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;
    let owner = crate::task_lifecycle::UserId::from(req.owner);
    let task_id = state
        .task_manager
        .create_task(owner, req.spec)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    Ok(serde_json::json!({ "task_id": task_id.to_string() }))
}

/// `task.list` — same as `GET /api/v1/tasks`
pub async fn handle_task_list(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let owner = params
        .as_ref()
        .and_then(|p| p.get("owner"))
        .and_then(|v| v.as_str())
        .map(crate::task_lifecycle::UserId::from);
    let tower = params
        .as_ref()
        .and_then(|p| p.get("tower"))
        .and_then(|v| v.as_str())
        .map(crate::task_lifecycle::TowerId::from);
    let filter = crate::task_lifecycle::TaskFilter {
        owner,
        tower,
        ..Default::default()
    };
    let tasks = state
        .task_manager
        .list_tasks(&filter)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    let tasks_json: Vec<Value> = tasks
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    Ok(serde_json::json!({ "tasks": tasks_json }))
}

/// `consent.check` — load consent record (`GET /api/consent/:id`)
pub async fn handle_consent_check(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let id = extract_str_param(params.as_ref(), "consent_id")?;
    let rec = state.consent_manager.get_consent(&id).await.ok_or_else(|| JsonRpcError {
        code: -32001,
        message: String::from("Consent not found"),
        data: None,
    })?;
    serde_json::to_value(rec).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `consent.grant` — approve consent (`PUT /api/consent/:id` with approve)
pub async fn handle_consent_grant(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let obj = params
        .as_ref()
        .and_then(|p| p.as_object())
        .ok_or_else(|| JsonRpcError::invalid_params("Parameters must be an object"))?;
    let id = obj
        .get("consent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing consent_id"))?;
    let reason = obj.get("reason").and_then(|v| v.as_str()).map(std::sync::Arc::from);
    if !state.consent_manager.approve(id, reason).await {
        return Err(JsonRpcError {
            code: -32001,
            message: String::from("Consent not found"),
            data: None,
        });
    }
    Ok(serde_json::json!({
        "status": "approved",
        "consent_id": id,
    }))
}

/// `protocol.negotiate` — same as `POST /api/protocol/negotiate`
pub async fn handle_protocol_negotiate_semantic(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let req: crate::server::protocol_api::NegotiateRequest = serde_json::from_value(
        params.ok_or_else(|| JsonRpcError::invalid_params("Missing parameters"))?,
    )
    .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;
    let res = crate::server::protocol_api::protocol_negotiate_result(&state.protocol_state, &req);
    serde_json::to_value(res).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}
