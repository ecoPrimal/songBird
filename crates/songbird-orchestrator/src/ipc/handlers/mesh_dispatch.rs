// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Mesh networking JSON-RPC dispatch (GAP-16: Tower Atomic Validation).

use super::IpcHandlers;
use crate::ipc::pure_rust_server::JsonRpcErrorExt;
use songbird_types::json_rpc_method::MeshMethod;

impl IpcHandlers {
    /// Dispatch a `mesh.*` JSON-RPC method to the `MeshHandler`.
    ///
    /// Converts the `Result<Value, String>` return type from `MeshHandler` to
    /// the `Result<Value, JsonRpcError>` expected by the UDS routing layer.
    pub async fn mesh_dispatch(
        &self,
        method: MeshMethod,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, crate::ipc::pure_rust_server::JsonRpcError> {
        let params = params.unwrap_or(serde_json::Value::Null);
        let result: Result<serde_json::Value, String> = match method {
            MeshMethod::Init => self.mesh_handler.handle_init(params).await,
            MeshMethod::Status => self.mesh_handler.handle_status(params).await,
            MeshMethod::FindPath => self.mesh_handler.handle_find_path(params).await,
            MeshMethod::Announce => self.mesh_handler.handle_announce(params).await,
            MeshMethod::Peers => self.mesh_handler.handle_peers(params).await,
            MeshMethod::Topology => self.mesh_handler.handle_topology(params).await,
            MeshMethod::HealthCheck => self.mesh_handler.handle_health_check(params).await,
            MeshMethod::AutoDiscover => self.mesh_handler.handle_auto_discover(params).await,
            MeshMethod::DiscoverRemotes => self.mesh_handler.handle_discover_remotes(params).await,
            MeshMethod::Mirror => self.mesh_handler.handle_mirror(params).await,
            MeshMethod::Publish => self.mesh_handler.handle_publish(params).await,
            MeshMethod::ProbeLatency => self.mesh_handler.handle_probe_latency(params).await,
            MeshMethod::CapabilitiesAnnounce => {
                self.mesh_handler.handle_capabilities_announce(params).await
            }
            MeshMethod::CapabilitiesRevoke => {
                self.mesh_handler.handle_capabilities_revoke(params).await
            }
            MeshMethod::Subscribe => self.mesh_handler.handle_subscribe(params).await,
            MeshMethod::Enroll => self.mesh_handler.handle_enroll(params).await,
            MeshMethod::GateEnroll => self.mesh_handler.handle_gate_enroll(params).await,
            MeshMethod::PruneStale => self.mesh_handler.handle_prune_stale(params).await,
            MeshMethod::ConnectivityCheck => {
                self.mesh_handler.handle_connectivity_check(params).await
            }
            MeshMethod::Throughput => self.mesh_handler.handle_throughput(params).await,
        };
        result.map_err(|e| crate::ipc::pure_rust_server::JsonRpcError::internal_error(e))
    }
}
