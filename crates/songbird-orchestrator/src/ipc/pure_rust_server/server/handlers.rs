// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! JSON-RPC request routing and response construction for the pure Rust IPC server.

use songbird_types::json_rpc_method::{
    CapabilitiesMethod, CoordinationMethod, DiscoveryMethod, GraphMethod, HealthMethod, HttpMethod,
    IpcMethod, PrimalMethod, TunnelMethod,
};
use songbird_types::{JsonRpcMethod, normalize_json_rpc_method_name};

use super::super::coordination_handlers;
use super::super::method_gate::{CallerContext, dispatch_auth_method, extract_bearer_token};
use super::super::protocol::{
    JsonRpcError, JsonRpcErrorExt, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseExt,
};
use super::UnixSocketServer;

impl UnixSocketServer {
    /// Handle a JSON-RPC 2.0 request and route to appropriate API handler
    #[expect(clippy::too_many_lines, reason = "IPC JSON-RPC handler dispatch")]
    pub(crate) async fn handle_jsonrpc_request(
        &self,
        mut request: JsonRpcRequest,
        caller: &CallerContext,
    ) -> JsonRpcResponse {
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);

        if request.jsonrpc != "2.0" {
            return JsonRpcResponse::error(
                id,
                JsonRpcError::INVALID_REQUEST,
                r#"Invalid Request: jsonrpc must be "2.0""#,
            );
        }

        // Extract _bearer_token from params and enrich CallerContext
        let caller = if let Some(ref mut params) = request.params {
            if let Some(token) = extract_bearer_token(params) {
                caller.clone().with_bearer_token(token)
            } else {
                caller.clone()
            }
        } else {
            caller.clone()
        };

        // JH-0: Pre-dispatch method gate authorization
        if let Some(auth_result) = dispatch_auth_method(&request.method, &self.gate, &caller) {
            return JsonRpcResponse::success(id, auth_result);
        }

        if let Err(gate_err) = self.gate.check(&request.method, &caller) {
            return JsonRpcResponse::error_from(gate_err, id);
        }

        let normalized = normalize_json_rpc_method_name(&request.method);

        let result = match JsonRpcMethod::parse_ipc(&request.method) {
            Ok(JsonRpcMethod::Health(HealthMethod::Liveness)) => {
                Ok(songbird_universal_ipc::introspection::health_liveness())
            }
            Ok(JsonRpcMethod::Health(HealthMethod::Readiness)) => {
                let status = songbird_universal_ipc::introspection::SubsystemStatus {
                    ipc: true,
                    ..Default::default()
                };
                Ok(songbird_universal_ipc::introspection::health_readiness(&status))
            }
            Ok(JsonRpcMethod::Health(HealthMethod::Check)) => {
                let status = songbird_universal_ipc::introspection::SubsystemStatus {
                    ipc: true,
                    ..Default::default()
                };
                Ok(songbird_universal_ipc::introspection::health_check(&status, None))
            }
            Ok(JsonRpcMethod::Ipc(IpcMethod::Register)) => {
                self.handlers.register_service_json(request.params).await
            }
            Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::List)) => {
                Ok(songbird_universal_ipc::introspection::capabilities_list())
            }
            Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::Methods)) => {
                Ok(songbird_universal_ipc::introspection::capabilities_methods())
            }
            Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::Resolve)) => {
                self.handlers.capability_resolve_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::Peers)) => {
                self.handlers.discovery_peers_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::Topology)) => {
                self.handlers.discovery_topology_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::Health)) => {
                self.handlers.discovery_health_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::Query)) => {
                self.handlers.discovery_query_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::Bonds)) => {
                self.handlers.discovery_bonds_json(request.params).await
            }
            Ok(JsonRpcMethod::Identity) => {
                Ok(songbird_universal_ipc::introspection::identity(&crate::env_config::family_id()))
            }
            Ok(JsonRpcMethod::IdentityGet(_)) => {
                Ok(songbird_universal_ipc::introspection::identity_get())
            }
            Ok(JsonRpcMethod::Primal(PrimalMethod::Info)) => {
                Ok(songbird_universal_ipc::introspection::primal_info())
            }
            Ok(JsonRpcMethod::Primal(PrimalMethod::Capabilities)) => {
                Ok(songbird_universal_ipc::introspection::primal_capabilities())
            }
            Ok(JsonRpcMethod::Primal(PrimalMethod::Announce)) => {
                let socket = self.socket_path.to_string_lossy();
                Ok(songbird_universal_ipc::introspection::primal_announce_with_socket(&socket))
            }
            Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::Discover)) => {
                coordination_handlers::handle_discover_capabilities().await
            }
            Ok(JsonRpcMethod::Http(HttpMethod::Request)) => {
                self.handlers
                    .http_request(request.params.unwrap_or_else(|| serde_json::json!({})))
                    .await
            }
            Ok(JsonRpcMethod::Http(HttpMethod::Get)) => {
                self.handlers
                    .http_get(request.params.unwrap_or_else(|| serde_json::json!({})))
                    .await
            }
            Ok(JsonRpcMethod::Http(HttpMethod::Post)) => {
                self.handlers
                    .http_post(request.params.unwrap_or_else(|| serde_json::json!({})))
                    .await
            }
            Ok(JsonRpcMethod::Mesh(m)) => self.handlers.mesh_dispatch(m, request.params).await,
            Ok(JsonRpcMethod::Http(HttpMethod::Put)) => {
                self.handlers
                    .http_put(request.params.unwrap_or_else(|| serde_json::json!({})))
                    .await
            }
            Ok(JsonRpcMethod::Http(HttpMethod::Delete)) => {
                self.handlers
                    .http_delete(request.params.unwrap_or_else(|| serde_json::json!({})))
                    .await
            }
            Ok(JsonRpcMethod::Http(HttpMethod::Proxy)) => Ok(serde_json::json!({
                "error": "http.proxy not available on this transport path — use the Universal IPC socket"
            })),
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::ByFamily)) => {
                self.handlers.discover_by_family_json(request.params).await
            }
            Ok(JsonRpcMethod::Tunnel(TunnelMethod::CreateGenetic)) => {
                self.handlers.create_genetic_tunnel_json(request.params).await
            }
            Ok(JsonRpcMethod::Capabilities(CapabilitiesMethod::Announce)) => {
                self.handlers.announce_capabilities_json(request.params).await
            }
            Ok(JsonRpcMethod::Discovery(DiscoveryMethod::ByCapability)) => {
                self.handlers.discover_by_capability_json(request.params).await
            }
            Ok(JsonRpcMethod::Health(HealthMethod::Service)) => {
                self.handlers.get_service_health_json(request.params).await
            }
            Ok(JsonRpcMethod::Graph(GraphMethod::Validate)) => {
                self.handlers.validate_graph_json(request.params).await
            }
            Ok(JsonRpcMethod::Graph(GraphMethod::CheckAvailability)) => {
                self.handlers.check_availability_json(request.params).await
            }
            Ok(JsonRpcMethod::Graph(GraphMethod::SuggestAlternatives)) => {
                self.handlers.suggest_alternatives_json(request.params).await
            }
            Ok(JsonRpcMethod::Coordination(CoordinationMethod::ValidatePattern)) => {
                self.handlers.validate_coordination_pattern_json(request.params).await
            }
            Ok(_) => Err(JsonRpcError::method_not_found(normalized)),
            Err(e) => Err(JsonRpcError::method_not_found(e.into_message())),
        };

        match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(error) => JsonRpcResponse::error_from(error, id),
        }
    }
}
