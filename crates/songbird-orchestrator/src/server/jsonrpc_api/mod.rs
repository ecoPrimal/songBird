// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

// JSON-RPC 2.0 Universal Gateway
// Progressive Protocol Enhancement - Phase 2
//
// This module implements a JSON-RPC 2.0 gateway that provides universal
// language-agnostic access to Songbird. Any language with HTTP support
// (Python, JavaScript, Java, Go, C++, etc.) can connect.
//
// Specification: https://www.jsonrpc.org/specification
// Part of: Progressive Protocol Enhancement - Week 2
// Created: November 11, 2025

mod handlers;
mod types;

#[cfg(test)]
#[path = "jsonrpc_api_tests.rs"]
mod tests;

pub use types::{JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse};

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use handlers::{
    handle_beacon_exchange, handle_compute_job_status, handle_compute_route, handle_consent_check,
    handle_consent_grant, handle_deployment_create, handle_deployment_hot_swap,
    handle_deployment_list, handle_deployment_restart, handle_deployment_status,
    handle_federation_join, handle_federation_peers, handle_health, handle_health_standard,
    handle_identity, handle_protocol_capabilities, handle_protocol_negotiate_semantic,
    handle_registry_discover, handle_registry_register, handle_service_get,
    handle_service_register, handle_services_list, handle_task_create, handle_task_list,
    handle_version,
};
use songbird_network_federation::service_registry::FederatedServiceRegistry;
use songbird_network_federation::state::FederationState;
use songbird_types::json_rpc_method::{
    CapabilitiesMethod, ComputeMethod, ConsentMethod, DeploymentMethod, FederationMethod,
    HealthMethod, JsonRpcMethod, NetworkMethod, ProtocolMethod, RegistryMethod,
    SongbirdComputeMethod, SongbirdMethod, SongbirdServicesMethod, TaskMethod,
};
use songbird_universal_ipc::service::IpcServiceHandler;
use songbird_universal_ipc::tower_atomic::JsonRpcHandler;
use std::time::Instant;
use types::JSONRPC_VERSION;

/// JSON-RPC 2.0 routes
pub fn jsonrpc_routes() -> Router<JsonRpcState> {
    Router::new()
        .route("/", post(handle_jsonrpc_request))
        .route("/rpc", post(handle_jsonrpc_request))
}

/// Shared state for JSON-RPC API
#[derive(Clone)]
pub struct JsonRpcState {
    /// Federation state for service routing
    pub federation_state: Arc<FederationState>,
    /// Federated service registry (inter-node)
    pub service_registry: Arc<FederatedServiceRegistry>,
    /// Compute API state (same as `/api/compute`)
    pub compute_state: crate::server::compute_api::ComputeApiState,
    /// Deployment API state (same as `/api/deployment`)
    pub deployment_state: crate::server::deployment_api::DeploymentState,
    /// Protocol API state (same as `/api/protocol`)
    pub protocol_state: crate::server::protocol_api::ProtocolApiState,
    /// Universal Port Authority registry (same as `/api/v1/services`)
    pub upa_registry: Arc<crate::service_registry::ServiceRegistry>,
    /// Task lifecycle manager (same as `/api/v1/tasks`)
    pub task_manager: Arc<crate::task_lifecycle::TaskLifecycleManager>,
    /// Consent manager (same as `/api/consent`)
    pub consent_manager: Arc<crate::consent_management::ConsentManager>,
    /// Server start time for uptime calculation
    pub start_time: Arc<RwLock<Instant>>,
    /// Universal IPC handler — full method table for inter-gate communication
    /// When present, unknown methods on TCP are forwarded to the same handler
    /// that serves the Unix socket, making TCP equivalent for LAN mesh.
    pub ipc_handler: Option<Arc<IpcServiceHandler>>,
}

impl JsonRpcState {
    /// Create with IPC handler for full method forwarding on TCP
    ///
    /// This enables inter-gate communication over TCP :3492 by forwarding
    /// unknown methods to the universal-ipc handler (same as Unix socket).
    /// Dark Forest gating still applies on all TCP requests.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "Single constructor wires every REST-backed JSON-RPC dependency"
    )]
    pub fn with_ipc_handler(
        federation_state: Arc<FederationState>,
        service_registry: Arc<FederatedServiceRegistry>,
        ipc_handler: Arc<IpcServiceHandler>,
        compute_state: crate::server::compute_api::ComputeApiState,
        deployment_state: crate::server::deployment_api::DeploymentState,
        protocol_state: crate::server::protocol_api::ProtocolApiState,
        upa_registry: Arc<crate::service_registry::ServiceRegistry>,
        task_manager: Arc<crate::task_lifecycle::TaskLifecycleManager>,
        consent_manager: Arc<crate::consent_management::ConsentManager>,
    ) -> Self {
        Self {
            federation_state,
            service_registry,
            compute_state,
            deployment_state,
            protocol_state,
            upa_registry,
            task_manager,
            consent_manager,
            start_time: Arc::new(RwLock::new(Instant::now())),
            ipc_handler: Some(ipc_handler),
        }
    }
}

/// POST /jsonrpc or POST /jsonrpc/rpc
///
/// Handles JSON-RPC 2.0 requests.
/// Provides universal language-agnostic access to Songbird functionality.
#[expect(clippy::too_many_lines, reason = "HTTP JSON-RPC method dispatch")]
async fn handle_jsonrpc_request(
    State(state): State<JsonRpcState>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    if request.jsonrpc != JSONRPC_VERSION {
        return Ok(Json(JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION.into(),
            result: None,
            error: Some(JsonRpcError::invalid_request("jsonrpc must be '2.0'")),
            id: request.id.unwrap_or(Value::Null),
        }));
    }

    let is_notification = request.id.is_none();
    let canonical = songbird_universal_ipc::introspection::normalize_method(&request.method);
    debug!(
        "JSON-RPC request: method={} (canonical={canonical}, notification={is_notification})",
        request.method
    );

    let parsed = JsonRpcMethod::from_wire_str(canonical);

    let params = request.params;
    let result = match parsed {
        Ok(
            JsonRpcMethod::Compute(ComputeMethod::Route)
            | JsonRpcMethod::SongbirdCompute(SongbirdComputeMethod::Schedule),
        ) => handle_compute_route(&state, params).await,
        Ok(JsonRpcMethod::Deployment(DeploymentMethod::Create)) => {
            handle_deployment_create(&state, params).await
        }
        Ok(JsonRpcMethod::Deployment(DeploymentMethod::Status)) => {
            handle_deployment_status(&state, params).await
        }
        Ok(JsonRpcMethod::Deployment(DeploymentMethod::HotSwap)) => {
            handle_deployment_hot_swap(&state, params).await
        }
        Ok(JsonRpcMethod::Deployment(DeploymentMethod::Restart)) => {
            handle_deployment_restart(&state, params).await
        }
        Ok(JsonRpcMethod::Deployment(DeploymentMethod::List)) => {
            handle_deployment_list(&state).await
        }
        Ok(JsonRpcMethod::Task(TaskMethod::Create)) => handle_task_create(&state, params).await,
        Ok(JsonRpcMethod::Task(TaskMethod::List)) => handle_task_list(&state, params).await,
        Ok(JsonRpcMethod::Consent(ConsentMethod::Check)) => {
            handle_consent_check(&state, params).await
        }
        Ok(JsonRpcMethod::Consent(ConsentMethod::Grant)) => {
            handle_consent_grant(&state, params).await
        }
        Ok(JsonRpcMethod::Registry(RegistryMethod::Register)) => {
            handle_registry_register(&state, params).await
        }
        Ok(JsonRpcMethod::Registry(RegistryMethod::Discover)) => {
            handle_registry_discover(&state, params).await
        }
        Ok(JsonRpcMethod::Protocol(ProtocolMethod::Negotiate)) => {
            handle_protocol_negotiate_semantic(&state, params).await
        }

        Ok(JsonRpcMethod::SongbirdServices(SongbirdServicesMethod::List)) => {
            handle_services_list(&state).await
        }
        Ok(JsonRpcMethod::SongbirdServices(SongbirdServicesMethod::Get)) => {
            handle_service_get(&state, params).await
        }
        Ok(JsonRpcMethod::SongbirdServices(SongbirdServicesMethod::Register)) => {
            handle_service_register(&state, params).await
        }

        Ok(JsonRpcMethod::SongbirdCompute(SongbirdComputeMethod::Status)) => {
            handle_compute_job_status(&state, params).await
        }

        Ok(JsonRpcMethod::Federation(FederationMethod::Peers)) => {
            handle_federation_peers(&state).await
        }
        Ok(JsonRpcMethod::Federation(FederationMethod::Join)) => {
            handle_federation_join(&state, params).await
        }

        Ok(JsonRpcMethod::Protocol(ProtocolMethod::Capabilities)) => {
            handle_protocol_capabilities().await
        }

        Ok(JsonRpcMethod::Songbird(SongbirdMethod::Health)) => handle_health(&state).await,
        Ok(JsonRpcMethod::Songbird(SongbirdMethod::Version)) => handle_version().await,

        Ok(JsonRpcMethod::Health(HealthMethod::Liveness | HealthMethod::Ping)) => {
            Ok(songbird_universal_ipc::introspection::health_liveness())
        }
        Ok(JsonRpcMethod::Health(HealthMethod::Readiness)) => {
            let status = songbird_universal_ipc::introspection::SubsystemStatus {
                ipc: true,
                ..Default::default()
            };
            Ok(songbird_universal_ipc::introspection::health_readiness(&status))
        }
        Ok(JsonRpcMethod::BiomeOsHealth | JsonRpcMethod::Health(HealthMethod::Check)) => {
            handle_health_standard(&state).await
        }
        Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::List)) => {
            Ok(songbird_universal_ipc::introspection::capabilities_list())
        }
        Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::Methods)) => {
            Ok(songbird_universal_ipc::introspection::capabilities_methods())
        }
        Ok(JsonRpcMethod::Identity) => handle_identity().await,
        Ok(JsonRpcMethod::IdentityGet(_)) => {
            Ok(songbird_universal_ipc::introspection::identity_get())
        }
        Ok(JsonRpcMethod::Network(NetworkMethod::BeaconExchange)) => {
            handle_beacon_exchange(params).await
        }

        Ok(_) | Err(_) => {
            if let Some(ref ipc_handler) = state.ipc_handler {
                debug!(
                    "Forwarding '{}' to universal-ipc handler (TCP to IPC bridge)",
                    request.method
                );
                match ipc_handler.handle(&request.method, params.unwrap_or(Value::Null)).await {
                    Ok(value) => Ok(value),
                    Err(e) => {
                        warn!("IPC handler error for '{}': {}", request.method, e);
                        Err(JsonRpcError::method_not_found(format!("{}: {}", request.method, e)))
                    }
                }
            } else {
                warn!("Unknown JSON-RPC method: {} (no IPC handler attached)", request.method);
                Err(JsonRpcError::method_not_found(&request.method))
            }
        }
    };

    if is_notification {
        return Err(StatusCode::NO_CONTENT);
    }

    let id = request.id.unwrap_or(Value::Null);
    let response = match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION.into(),
            result: Some(value),
            error: None,
            id,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION.into(),
            result: None,
            error: Some(error),
            id,
        },
    };

    Ok(Json(response))
}
