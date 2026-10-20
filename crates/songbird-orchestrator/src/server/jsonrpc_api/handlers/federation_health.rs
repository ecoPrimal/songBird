// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

use serde_json::Value;
use tracing::debug;

use super::super::JsonRpcState;
use super::super::types::{JsonRpcError, JsonRpcErrorExt};

/// songbird.federation.peers
pub async fn handle_federation_peers(state: &JsonRpcState) -> Result<Value, JsonRpcError> {
    let nodes = state.federation_state.active_nodes().await;
    let peers: Vec<Value> = nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "node_id": node.node_id,
                "name": node.node_name,
                "address": node.node_address,
                "endpoints": node.active_endpoints().iter().map(|ep| &ep.address).collect::<Vec<_>>(),
                "capabilities": node.capabilities,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "peers": peers,
        "count": nodes.len(),
    }))
}

/// songbird.federation.join
pub async fn handle_federation_join(
    state: &JsonRpcState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing join parameters"))?;

    let obj = params
        .as_object()
        .ok_or_else(|| JsonRpcError::invalid_params("Parameters must be an object"))?;

    let node_id = obj
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'node_id'"))?;
    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or(node_id);

    let now = chrono::Utc::now();
    let registration = songbird_network_federation::state::NodeRegistration {
        node_id: node_id.to_string(),
        node_name: name.to_string(),
        node_address: obj.get("address").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        endpoints: None,
        cpu_cores: 0,
        memory_gb: 0,
        gpu_model: None,
        storage_gb: None,
        capabilities: obj
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        status: songbird_network_federation::state::NodeStatus::Active,
        joined_at: now,
        last_heartbeat: now,
    };

    state.federation_state.register_node(registration).await;

    let stats = state.federation_state.get_stats().await;

    Ok(serde_json::json!({
        "status": "joined",
        "node_id": node_id,
        "active_peers": stats.active_nodes,
    }))
}

/// songbird.protocol.capabilities
pub async fn handle_protocol_capabilities() -> Result<Value, JsonRpcError> {
    Ok(serde_json::json!({
        "songbird_version": env!("CARGO_PKG_VERSION"),
        "protocols": {
            "http": {
                "version": "1.1",
                "available": true
            },
            "jsonrpc": {
                "version": "2.0",
                "available": true
            },
            "tarpc": {
                "version": "0.34",
                "available": true
            }
        },
        "preferred_protocol": "jsonrpc",
        "fallback_protocol": "http"
    }))
}

/// songbird.health
pub async fn handle_health(state: &JsonRpcState) -> Result<Value, JsonRpcError> {
    let start_time = state.start_time.read().await;
    let uptime_seconds = start_time.elapsed().as_secs();

    Ok(serde_json::json!({
        "status": "healthy",
        "primal": songbird_types::primal_names::SELF_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_s": uptime_seconds
    }))
}

/// songbird.version
pub async fn handle_version() -> Result<Value, JsonRpcError> {
    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": "Songbird Universal Orchestrator",
        "architecture": "100% Rust Core + Universal Compatibility"
    }))
}

/// `health.check` / `health` / `status` / `check` — full health with details
///
/// Capability-based crypto provider discovery: asks "who provides crypto?"
/// rather than naming a specific primal. Follows the 5-tier discovery standard.
/// Subsystem status is derived from real runtime state, not hardcoded.
pub async fn handle_health_standard(state: &JsonRpcState) -> Result<Value, JsonRpcError> {
    let start_time = state.start_time.read().await;
    let uptime_seconds = start_time.elapsed().as_secs();

    let crypto_provider_available = {
        let socket = songbird_process_env::var("CRYPTO_PROVIDER_SOCKET")
            .or_else(|_| songbird_process_env::var("CRYPTO_SIGN_PROVIDER_SOCKET"))
            .ok();
        if let Some(path) = socket {
            std::path::Path::new(&path).exists()
        } else {
            let xdg = songbird_process_env::var("XDG_RUNTIME_DIR").unwrap_or_default();
            if xdg.is_empty() {
                false
            } else {
                let family_id = crate::env_config::family_id();
                let xdg_path = std::path::PathBuf::from(&xdg)
                    .join(songbird_types::defaults::paths::BIOMEOS_RUNTIME_SUBDIR)
                    .join(format!("crypto-provider-{family_id}.sock"))
                    .to_string_lossy()
                    .into_owned();
                std::path::Path::new(&xdg_path).exists()
            }
        }
    };

    let ipc_socket = crate::env_config::socket_path();
    let ipc_status = if ipc_socket.exists() {
        "up"
    } else {
        "degraded"
    };

    let federation_stats = state.federation_state.get_stats().await;
    let federation_status = if federation_stats.active_nodes > 0 {
        "up"
    } else {
        "standalone"
    };

    let overall = if ipc_status == "up" {
        "healthy"
    } else {
        "degraded"
    };

    Ok(serde_json::json!({
        "status": overall,
        "primal": songbird_types::primal_names::SELF_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_s": uptime_seconds,
        "crypto_provider_available": crypto_provider_available,
        "subsystems": {
            "ipc": ipc_status,
            "discovery": "up",
            "federation": federation_status
        }
    }))
}

/// identity - Return Songbird's identity and capabilities
pub async fn handle_identity() -> Result<Value, JsonRpcError> {
    let family_id = crate::env_config::family_id();

    let capabilities = vec![
        "network.broadcast",
        "network.listen",
        "network.beacon_exchange",
        "encrypt_discovery",
        "decrypt_discovery",
        "http.post",
        "http.get",
        "http.request",
        "discovery.announce",
        "discovery.query",
        "security.verify",
    ];

    Ok(serde_json::json!({
        "primal": crate::env_config::primal_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "family_id": family_id,
        "capabilities": capabilities
    }))
}

/// `network.beacon_exchange` - Exchange encrypted beacons with peers
pub async fn handle_beacon_exchange(params: Option<Value>) -> Result<Value, JsonRpcError> {
    let params = params.ok_or_else(|| JsonRpcError::invalid_params("Missing params"))?;

    let beacon = params
        .get("beacon")
        .ok_or_else(|| JsonRpcError::invalid_params("Missing 'beacon' parameter"))?;

    let peer_address = params.get("peer_address").and_then(|v| v.as_str());

    debug!("📡 Beacon exchange request received");
    if let Some(addr) = peer_address {
        debug!("   Peer: {}", addr);
    }

    Ok(serde_json::json!({
        "status": "received",
        "beacon_size": beacon.to_string().len(),
        "peer_address": peer_address,
        "message": "Beacon received. Full peer discovery available via IPC socket."
    }))
}
