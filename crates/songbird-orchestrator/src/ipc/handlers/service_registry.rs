// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Service Registry Handlers
//!
//! Handles registration, discovery, and health checking of primal services.
//! v3.20.0: Service registry for capability-based discovery
//! v3.22.1: Extracted to focused module (Jan 12, 2026)
//! v3.34.0: Migrated to Pure Rust types (Phase 4B)

use std::time::SystemTime;
use tracing::{debug, info};

use crate::ipc::handlers::IpcHandlers;
use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
use crate::ipc::types::{
    CapabilityResolveRequest, CapabilityResolveResponse, DiscoverByCapabilityRequest,
    DiscoverByCapabilityResponse, GetServiceHealthRequest, GetServiceHealthResponse,
    HealthCheckResponse, HealthStatus, RegisterServiceRequest, RegisterServiceResponse,
    system_time_to_iso8601,
};

// ============================================================================
// jsonrpsee Handlers (Original API)
// ============================================================================

/// Handle `register_service` RPC call
///
/// Registers a primal service with Songbird for capability-based discovery.
///
/// ## Example Request
///
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "register_service",
///   "params": {
///     "primal_name": "security provider",
///     "capabilities": ["encryption", "identity"],
///     "endpoint": "/run/user/1000/security-provider-nat0.sock",
///     "protocol": "json-rpc",
///     "health_check_interval": 30
///   },
///   "id": 4
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn register_service(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<RegisterServiceResponse, JsonRpcError> {
    debug!("📝 IPC: register_service called");

    // Parse request parameters
    let request: RegisterServiceRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid params: {e}")))?;

    info!(
        "📋 Registering service: {} with capabilities: {:?}",
        request.primal_name, request.capabilities
    );

    // Register in service registry
    let service_id = handlers
        .service_registry
        .register_service(
            request.primal_name,
            request.capabilities,
            request.endpoint,
            request.protocol,
            request.health_check_interval,
        )
        .await
        .map_err(|e| {
            JsonRpcError::custom(-32603, "Failed to register service", Some(format!("{e}")))
        })?;

    info!("✅ Service registered: {}", service_id);

    Ok(RegisterServiceResponse {
        service_id,
        status: String::from("registered"),
        registered_at: system_time_to_iso8601(SystemTime::now()),
    })
}

/// Handle `discover_by_capability` RPC call
///
/// Discovers primals by capability (e.g., "encryption", "storage", "*" for all).
///
/// ## Example Request
///
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "discover_by_capability",
///   "params": {
///     "capability": "encryption",
///     "protocol": "json-rpc"
///   },
///   "id": 5
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn discover_by_capability(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<DiscoverByCapabilityResponse, JsonRpcError> {
    debug!("🔍 IPC: discover_by_capability called");

    // Parse request parameters
    let request: DiscoverByCapabilityRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid params: {e}")))?;

    info!("🔎 Discovering primals with capability: {}", request.capability);

    // Query service registry
    let primals = handlers
        .service_registry
        .discover_by_capability(&request.capability, request.protocol.as_deref())
        .await
        .map_err(|e| {
            JsonRpcError::custom(-32603, "Failed to discover primals", Some(format!("{e}")))
        })?;

    info!("   Found {} primals", primals.len());

    Ok(DiscoverByCapabilityResponse {
        primals,
    })
}

/// Handle `get_service_health` RPC call
///
/// Gets the health status of a specific registered service.
///
/// ## Example Request
///
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "get_service_health",
///   "params": {
///     "service_id": "security-provider-12345"
///   },
///   "id": 6
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn get_service_health(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<GetServiceHealthResponse, JsonRpcError> {
    debug!("🩺 IPC: get_service_health called");

    // Parse request parameters
    let request: GetServiceHealthRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid params: {e}")))?;

    info!("🏥 Checking health for service: {}", request.service_id);

    // Get health from registry
    let (status, message) =
        handlers.service_registry.get_service_health(&request.service_id).await.map_err(|e| {
            JsonRpcError::custom(-32603, "Failed to get health", Some(format!("{e}")))
        })?;

    let health = HealthStatus {
        service_id: request.service_id,
        status,
        message,
        timestamp: system_time_to_iso8601(SystemTime::now()),
    };

    Ok(GetServiceHealthResponse {
        health,
    })
}

/// Handle `health_check` RPC call
///
/// Returns Songbird's own health status.
///
/// ## Example Request
///
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "health_check",
///   "params": {},
///   "id": 7
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn health_check(
    _handlers: &IpcHandlers,
    _params: serde_json::Value,
) -> Result<HealthCheckResponse, JsonRpcError> {
    debug!("💓 IPC: health_check called");

    // Songbird's health is always "healthy" if responding to RPC
    let health = HealthStatus {
        service_id: String::from(songbird_types::primal_names::SELF_NAME),
        status: String::from("healthy"),
        message: None,
        timestamp: system_time_to_iso8601(SystemTime::now()),
    };

    Ok(HealthCheckResponse {
        health,
    })
}

// ============================================================================
// Pure JSON Adapters (v3.22.0)
// ============================================================================

/// Service Registry: `register_service` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn register_service_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
    let request: RegisterServiceRequest = match params {
        Some(p) => serde_json::from_value(p).map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::invalid_params(format!(
                "Invalid params: {e}"
            ))
        })?,
        None => {
            return Err(crate::ipc::pure_rust_server::JsonRpcError::invalid_params(
                "params required",
            ));
        }
    };

    let response = handlers
        .service_registry
        .register_service(
            request.primal_name,
            request.capabilities,
            request.endpoint,
            request.protocol,
            request.health_check_interval,
        )
        .await
        .map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
                "Failed to register service: {e}"
            ))
        })?;

    let resp = RegisterServiceResponse {
        service_id: response,
        status: String::from("registered"),
        registered_at: system_time_to_iso8601(SystemTime::now()),
    };

    serde_json::to_value(resp).map_err(|e| {
        crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
            "Failed to serialize response: {e}"
        ))
    })
}

/// Service Registry: `discover_by_capability` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn discover_by_capability_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
    let request: DiscoverByCapabilityRequest = match params {
        Some(p) => serde_json::from_value(p).map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::invalid_params(format!(
                "Invalid params: {e}"
            ))
        })?,
        None => {
            return Err(crate::ipc::pure_rust_server::JsonRpcError::invalid_params(
                "params required",
            ));
        }
    };

    let primals = handlers
        .service_registry
        .discover_by_capability(&request.capability, request.protocol.as_deref())
        .await
        .map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
                "Failed to discover primals: {e}"
            ))
        })?;

    let resp = DiscoverByCapabilityResponse {
        primals,
    };

    serde_json::to_value(resp).map_err(|e| {
        crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
            "Failed to serialize response: {e}"
        ))
    })
}

/// `capability.resolve` — single-step DNS-like routing by capability (pure JSON adapter).
///
/// Returns the best provider endpoint for the requested capability. Uses the
/// same underlying `discover_by_capability` registry query but returns only the
/// first (most recently registered) match, matching the universal-ipc
/// `capability.resolve` wire contract.
///
/// # Errors
///
/// Returns a method-not-found-style error if no provider is registered for the
/// requested capability.
pub async fn capability_resolve_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
    let request: CapabilityResolveRequest = match params {
        Some(p) => serde_json::from_value(p).map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::invalid_params(format!(
                "Invalid params: {e}"
            ))
        })?,
        None => {
            return Err(crate::ipc::pure_rust_server::JsonRpcError::invalid_params(
                "params required",
            ));
        }
    };

    let primals =
        handlers.service_registry.discover_by_capability(&request.capability, None).await.map_err(
            |e| {
                crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
                    "Failed to resolve capability: {e}"
                ))
            },
        )?;

    let provider = primals.into_iter().next().ok_or_else(|| {
        let msg = format!("No provider found for capability: {}", request.capability);
        crate::ipc::pure_rust_server::JsonRpcError::custom(-32601, &msg, None)
    })?;

    let socket = if provider.endpoint.starts_with('/') {
        Some(provider.endpoint.clone())
    } else {
        None
    };
    let native_endpoint = if provider.endpoint.starts_with('/') {
        format!("unix://{}", provider.endpoint)
    } else {
        provider.endpoint.clone()
    };
    let virtual_endpoint = format!("capability://{}@{}", request.capability, provider.primal_name);

    let transport = if provider.endpoint.starts_with('/') {
        songbird_universal_ipc::TransportEndpoint::Uds {
            path: provider.endpoint.clone(),
        }
    } else if let Some(addr) = provider.endpoint.strip_prefix("tcp://") {
        let (host, port) = addr.rsplit_once(':').unwrap_or((addr, "0"));
        songbird_universal_ipc::TransportEndpoint::Tcp {
            host: host.to_string(),
            port: port.parse().unwrap_or(0),
        }
    } else {
        songbird_universal_ipc::TransportEndpoint::Uds {
            path: provider.endpoint.clone(),
        }
    };

    let resp = CapabilityResolveResponse {
        service_id: provider.service_id,
        primal_name: provider.primal_name,
        endpoint: provider.endpoint,
        protocol: provider.protocol,
        socket,
        native_endpoint,
        transport,
        virtual_endpoint,
        capabilities: provider.capabilities,
    };

    serde_json::to_value(resp).map_err(|e| {
        crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
            "Failed to serialize response: {e}"
        ))
    })
}

/// Service Registry: `get_service_health` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn get_service_health_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
    let request: GetServiceHealthRequest = match params {
        Some(p) => serde_json::from_value(p).map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::invalid_params(format!(
                "Invalid params: {e}"
            ))
        })?,
        None => {
            return Err(crate::ipc::pure_rust_server::JsonRpcError::invalid_params(
                "params required",
            ));
        }
    };

    let (status, message) =
        handlers.service_registry.get_service_health(&request.service_id).await.map_err(|e| {
            crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
                "Failed to get health: {e}"
            ))
        })?;

    let health = HealthStatus {
        service_id: request.service_id,
        status,
        message,
        timestamp: system_time_to_iso8601(SystemTime::now()),
    };

    let resp = GetServiceHealthResponse {
        health,
    };

    serde_json::to_value(resp).map_err(|e| {
        crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
            "Failed to serialize response: {e}"
        ))
    })
}

/// Service Registry: `health_check` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn health_check_json(
    _handlers: &IpcHandlers,
) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
    let health = HealthStatus {
        service_id: String::from(songbird_types::primal_names::SELF_NAME),
        status: String::from("healthy"),
        message: Some(String::from("Songbird orchestrator is running")),
        timestamp: system_time_to_iso8601(SystemTime::now()),
    };

    let resp = HealthCheckResponse {
        health,
    };

    serde_json::to_value(resp).map_err(|e| {
        crate::ipc::pure_rust_server::JsonRpcError::internal_error(format!(
            "Failed to serialize response: {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test assertions")]

    use std::sync::Arc;

    use super::*;
    use crate::app::connection_manager::ConnectionManager;
    use crate::ipc::handlers::IpcHandlers;
    use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
    use crate::ipc::registry::ServiceRegistry;
    use songbird_http_client::SecurityRpcClient;

    fn test_handlers() -> IpcHandlers {
        let registry = Arc::new(ServiceRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new());
        let security_client = Arc::new(SecurityRpcClient::new_direct(
            "/tmp/songbird-orchestrator-service-registry-tests.sock",
        ));
        IpcHandlers::new(registry, None, connection_manager, security_client)
    }

    fn register_params() -> serde_json::Value {
        serde_json::json!({
            "primal_name": "test primal",
            "capabilities": ["encryption"],
            "endpoint": "/tmp/test-primal.sock",
            "protocol": "json-rpc",
            "health_check_interval": 30
        })
    }

    #[tokio::test]
    async fn register_service_rejects_invalid_params() {
        let handlers = test_handlers();
        let err = register_service(&handlers, serde_json::json!(1)).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn discover_by_capability_rejects_invalid_params() {
        let handlers = test_handlers();
        let err =
            discover_by_capability(&handlers, serde_json::json!([1, 2, 3])).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn get_service_health_rejects_invalid_params() {
        let handlers = test_handlers();
        let err = get_service_health(&handlers, serde_json::json!("bad")).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn health_check_response_shape() {
        let handlers = test_handlers();
        let resp = health_check(&handlers, serde_json::json!({})).await.unwrap();
        assert_eq!(resp.health.service_id, "songbird");
        assert_eq!(resp.health.status, "healthy");
        assert!(resp.health.timestamp.contains('T') || !resp.health.timestamp.is_empty());
    }

    #[tokio::test]
    async fn register_service_json_requires_params() {
        let handlers = test_handlers();
        let err = register_service_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("params required"));
    }

    #[tokio::test]
    async fn register_service_json_success() {
        let handlers = test_handlers();
        let out = register_service_json(&handlers, Some(register_params())).await.unwrap();
        assert_eq!(out["status"], serde_json::json!("registered"));
        assert!(out.get("service_id").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn discover_by_capability_json_wildcard_after_register() {
        let handlers = test_handlers();
        register_service_json(&handlers, Some(register_params())).await.unwrap();
        let out = discover_by_capability_json(
            &handlers,
            Some(serde_json::json!({ "capability": "*", "protocol": "json-rpc" })),
        )
        .await
        .unwrap();
        let primals = out["primals"].as_array().unwrap();
        assert_eq!(primals.len(), 1);
    }

    #[tokio::test]
    async fn capability_resolve_json_no_provider() {
        let handlers = test_handlers();
        let err = capability_resolve_json(
            &handlers,
            Some(serde_json::json!({ "capability": "nonexistent-cap" })),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("No provider found"));
    }

    #[tokio::test]
    async fn capability_resolve_json_returns_first_match() {
        let handlers = test_handlers();
        register_service_json(&handlers, Some(register_params())).await.unwrap();
        let out = capability_resolve_json(
            &handlers,
            Some(serde_json::json!({ "capability": "encryption" })),
        )
        .await
        .unwrap();
        assert_eq!(out["endpoint"], serde_json::json!("/tmp/test-primal.sock"));
        assert_eq!(out["protocol"], serde_json::json!("json-rpc"));
        assert!(out.get("primal_name").is_some(), "primal_name must be in response");
        assert_eq!(out["socket"], serde_json::json!("/tmp/test-primal.sock"));
        assert_eq!(out["native_endpoint"], serde_json::json!("unix:///tmp/test-primal.sock"));
        assert_eq!(
            out["virtual_endpoint"],
            serde_json::json!("capability://encryption@test primal")
        );
    }

    #[tokio::test]
    async fn get_service_health_json_unknown_service() {
        let handlers = test_handlers();
        let out = get_service_health_json(
            &handlers,
            Some(serde_json::json!({ "service_id": "missing" })),
        )
        .await
        .unwrap();
        assert_eq!(out["health"]["status"], serde_json::json!("unknown"));
    }

    #[tokio::test]
    async fn health_check_json_shape() {
        let handlers = test_handlers();
        let out = health_check_json(&handlers).await.unwrap();
        assert_eq!(out["health"]["service_id"], serde_json::json!("songbird"));
        assert_eq!(out["health"]["status"], serde_json::json!("healthy"));
        assert_eq!(out["health"]["message"], serde_json::json!("Songbird orchestrator is running"));
    }
}
