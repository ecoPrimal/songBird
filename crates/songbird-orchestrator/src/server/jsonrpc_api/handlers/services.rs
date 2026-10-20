// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

use serde_json::Value;

use super::super::JsonRpcState;
use super::super::types::{JsonRpcError, JsonRpcErrorExt};

/// songbird.services.list
pub async fn handle_services_list(state: &JsonRpcState) -> Result<Value, JsonRpcError> {
    let services = state.service_registry.get_all_services().await;
    let service_list: Vec<Value> = services
        .iter()
        .map(|svc| {
            serde_json::json!({
                "service_id": svc.service_id,
                "name": svc.service_name,
                "type": svc.service_type,
                "endpoint": svc.endpoint,
                "tower_id": svc.tower_id,
                "capabilities": svc.capabilities,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "services": service_list,
        "count": services.len(),
    }))
}

/// songbird.services.get
pub async fn handle_service_get(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let service_id = match &params {
        Some(Value::Object(map)) => map
            .get("service_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("Missing 'service_id' parameter"))?
            .to_string(),
        Some(Value::Array(arr)) if !arr.is_empty() => arr[0]
            .as_str()
            .ok_or_else(|| JsonRpcError::invalid_params("First parameter must be string"))?
            .to_string(),
        _ => return Err(JsonRpcError::invalid_params("Missing service_id parameter")),
    };

    match state.service_registry.find_by_id(&service_id).await {
        Some(svc) => Ok(serde_json::json!({
            "service_id": svc.service_id,
            "name": svc.service_name,
            "type": svc.service_type,
            "endpoint": svc.endpoint,
            "tower_id": svc.tower_id,
            "tower_name": svc.tower_name,
            "capabilities": svc.capabilities,
            "status": "active",
        })),
        None => Err(JsonRpcError {
            code: -32001,
            message: format!("Service not found: {service_id}"),
            data: None,
        }),
    }
}

/// songbird.services.register
pub async fn handle_service_register(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let params =
        params.ok_or_else(|| JsonRpcError::invalid_params("Missing registration parameters"))?;

    let obj = params
        .as_object()
        .ok_or_else(|| JsonRpcError::invalid_params("Parameters must be an object"))?;

    let service_id = obj
        .get("service_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'service_id'"))?;
    let service_name = obj.get("name").and_then(|v| v.as_str()).unwrap_or(service_id);
    let service_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("generic");
    let endpoint = obj
        .get("endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'endpoint'"))?;
    let capabilities: Vec<String> = obj
        .get("capabilities")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let now = chrono::Utc::now();
    let registration = songbird_network_federation::service_registry::ServiceRegistration {
        service_id: service_id.to_string(),
        service_name: service_name.to_string(),
        service_type: service_type.to_string(),
        tower_id: String::from("local"),
        tower_name: String::from("local"),
        endpoint: endpoint.to_string(),
        capabilities,
        health_status: songbird_network_federation::service_registry::ServiceHealthStatus::Healthy,
        registered_at: now,
        last_seen: now,
        metadata: std::collections::HashMap::new(),
    };

    state.service_registry.register_local(registration).await;

    Ok(serde_json::json!({
        "status": "registered",
        "service_id": service_id,
    }))
}

/// `registry.register` — `POST /api/v1/services/register`
pub async fn handle_registry_register(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let req: crate::service_registry::RegistrationRequest = serde_json::from_value(
        params.ok_or_else(|| JsonRpcError::invalid_params("Missing parameters"))?,
    )
    .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;
    let response = state
        .upa_registry
        .register(req)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    serde_json::to_value(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// `registry.discover` — list services or query by capability (UPA)
pub async fn handle_registry_discover(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    if let Some(cap) = params.as_ref().and_then(|p| p.get("capability")).and_then(|v| v.as_str()) {
        let services = state.upa_registry.query_by_capability(cap).await;
        Ok(serde_json::json!({
            "capability": cap,
            "services": services,
            "count": services.len(),
        }))
    } else {
        let services = state.upa_registry.list_services().await;
        let stats = state.upa_registry.get_stats().await;
        Ok(serde_json::json!({ "services": services, "stats": stats }))
    }
}
