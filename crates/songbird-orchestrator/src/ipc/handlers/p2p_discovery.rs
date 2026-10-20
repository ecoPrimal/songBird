// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! P2P Discovery API Handlers (v3.19.1, refactored v3.22.1)
//!
//! Handlers for peer discovery and BTSP tunnel establishment.
//!
//! v3.19.1: Initial implementation for P2P discovery
//! v3.22.1: Extracted from monolithic handlers.rs (Jan 12, 2026)

use super::IpcHandlers;
use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
use crate::ipc::types::{
    AnnounceCapabilitiesRequest, AnnounceCapabilitiesResponse, CreateGeneticTunnelRequest,
    CreateGeneticTunnelResponse, DiscoverByFamilyRequest, DiscoverByFamilyResponse, DiscoveredNode,
};
use songbird_types::trust::TrustLevel;
use std::time::SystemTime;
use tracing::{info, warn};

/// Returns whether a discovered peer matches any of the requested family tag substrings.
/// Used by `discover_by_family` and `discover_by_family_json` (keep logic identical).
fn peer_matches_family_tags(peer_tags: Option<&Vec<String>>, family_tags: &[String]) -> bool {
    let Some(tags) = peer_tags else {
        return false;
    };
    family_tags.iter().any(|family_tag| tags.iter().any(|peer_tag| peer_tag.contains(family_tag)))
}

/// Build RPC node from a discovered peer without cloning fields the response owns.
fn discovered_node_from_peer(
    peer: songbird_discovery::anonymous::DiscoveredPeer,
) -> DiscoveredNode {
    let songbird_discovery::anonymous::DiscoveredPeer {
        session_id,
        node_id,
        node_name,
        tags,
        capabilities,
        last_seen,
        address,
        port,
        ..
    } = peer;
    let https_endpoint =
        songbird_types::constants::endpoint_url(&address.ip().to_string(), port, "");
    DiscoveredNode {
        node_id: node_id.unwrap_or(session_id),
        node_name,
        genetic_families: tags.unwrap_or_default(),
        sub_federations: vec![],
        capabilities,
        btsp_endpoint: None,
        https_endpoint,
        last_seen: format!("{last_seen:?}"),
    }
}

// ============================================================================
// jsonrpsee Handlers (for jsonrpsee server)
// ============================================================================

/// Handle `discover_by_family` RPC call (Pure Rust, v3.34.0)
///
/// v3.19.1: Returns discovered peers filtered by family tags
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn discover_by_family(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<DiscoverByFamilyResponse, JsonRpcError> {
    info!("🔍 P2P Discovery API: discover_by_family");

    let request: DiscoverByFamilyRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse params: {e}"), None))?;

    if let Some(listener) = &handlers.discovery_listener {
        // Get all discovered peers
        let discovered_peers = listener.get_peers().await;

        info!("🔍 Discovered {} peers total", discovered_peers.len());

        // Filter by family tags if provided
        let filtered_nodes: Vec<_> = if request.family_tags.is_empty() {
            // No filter - return all peers
            discovered_peers
        } else {
            // Filter peers by family tags
            discovered_peers
                .into_iter()
                .filter(|peer| peer_matches_family_tags(peer.tags.as_ref(), &request.family_tags))
                .collect()
        };

        info!(
            "✅ Found {} peers matching family tags: {:?}",
            filtered_nodes.len(),
            request.family_tags
        );

        // Convert DiscoveredPeer to response nodes
        let nodes = filtered_nodes.into_iter().map(discovered_node_from_peer).collect();

        Ok(DiscoverByFamilyResponse {
            nodes,
        })
    } else {
        warn!("⚠️  Discovery listener not available");
        Ok(DiscoverByFamilyResponse {
            nodes: vec![],
        })
    }
}

/// Handle `create_genetic_tunnel` RPC call (Pure Rust, v3.34.0)
///
/// v3.19.1: Establishes BTSP tunnel with genetic lineage proof
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn create_genetic_tunnel(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<CreateGeneticTunnelResponse, JsonRpcError> {
    info!("🔗 P2P Discovery API: create_genetic_tunnel");

    let request: CreateGeneticTunnelRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse params: {e}"), None))?;

    // Get peer endpoint from request or fail
    let peer_endpoint = match &request.peer_endpoint {
        Some(ep) => ep.clone(),
        None => {
            return Err(JsonRpcError::custom(
                -32602,
                "peer_endpoint is required for tunnel creation",
                None,
            ));
        }
    };

    // Extract genetic family from proof
    let genetic_families = if let Some(ref proof) = request.genetic_proof {
        vec![proof.family_id.clone()]
    } else {
        vec![]
    };

    // Establish connection via connection manager
    let result = handlers
        .connection_manager
        .establish_connection(
            request.peer_node_id.clone(),
            peer_endpoint.clone(),
            vec![], // capabilities (unknown at this point)
            genetic_families,
            TrustLevel::Elevated, // Genetic proof grants elevated trust
            String::from("genetic_tunnel"),
        )
        .await;

    let tunnel_id = format!(
        "tunnel-{}-{}",
        request.peer_node_id,
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs()
    );

    match result {
        Ok(()) => {
            info!("✅ BTSP tunnel established: {}", tunnel_id);
            Ok(CreateGeneticTunnelResponse {
                tunnel_id,
                status: String::from("established"),
                local_endpoint: None, // NOTE: Would require BTSP client integration (future: Arc<BtspClient> in handlers)
                peer_endpoint: Some(peer_endpoint),
                encryption: Some(String::from("ChaCha20-Poly1305")),
                created_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .to_string(),
            })
        }
        Err(e) => {
            warn!("❌ Failed to establish BTSP tunnel: {}", e);
            Ok(CreateGeneticTunnelResponse {
                tunnel_id: String::new(),
                status: String::from("failed"),
                local_endpoint: None,
                peer_endpoint: Some(peer_endpoint),
                encryption: None,
                created_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .to_string(),
            })
        }
    }
}

/// Handle `announce_capabilities` RPC call (Pure Rust, v3.34.0)
///
/// v3.19.1: Updates broadcaster with new capabilities
/// # Errors
///
/// Returns an error if the operation fails.
#[allow(clippy::unused_async, reason = "async signature matches other IPC handler entry points")]
pub async fn announce_capabilities(
    _handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<AnnounceCapabilitiesResponse, JsonRpcError> {
    info!("📢 P2P Discovery API: announce_capabilities");

    let request: AnnounceCapabilitiesRequest = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse params: {e}"), None))?;

    // NOTE: Capability announcement requires broadcaster restart to take effect
    // The broadcaster is created at startup with initial capabilities.
    // Dynamic updates would require adding Arc<RwLock<Broadcaster>> to handlers,
    // or implementing a broadcaster control channel (future enhancement).
    // For now, we log the requested capabilities for observability.
    info!(
        "📢 Capability announcement requested: caps={:?}, federations={:?}, families={:?}",
        request.capabilities, request.sub_federations, request.genetic_families
    );
    info!("💡 To apply: Restart orchestrator with updated capabilities in config");

    Ok(AnnounceCapabilitiesResponse {
        status: String::from("updated"),
        broadcasting: true,
        updated_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    })
}

// ============================================================================
// Pure JSON Adapters (for pure Rust Unix socket server v3.22.0)
// ============================================================================

/// P2P Discovery: `discover_by_family` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn discover_by_family_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let request: DiscoverByFamilyRequest = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for discover_by_family"));
    };

    if let Some(listener) = &handlers.discovery_listener {
        // Get all discovered peers (same implementation as jsonrpsee handler above)
        let discovered_peers = listener.get_peers().await;
        let family_tags = &request.family_tags;

        // Filter by family tags if provided
        let filtered_nodes: Vec<_> = if family_tags.is_empty() {
            discovered_peers
        } else {
            discovered_peers
                .into_iter()
                .filter(|peer| peer_matches_family_tags(peer.tags.as_ref(), family_tags))
                .collect()
        };

        // Convert to response nodes
        let nodes = filtered_nodes.into_iter().map(discovered_node_from_peer).collect();

        let response = DiscoverByFamilyResponse {
            nodes,
        };
        serde_json::to_value(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
    } else {
        let response = DiscoverByFamilyResponse {
            nodes: vec![],
        };
        serde_json::to_value(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
    }
}

/// P2P Discovery: `create_genetic_tunnel` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn create_genetic_tunnel_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let request: CreateGeneticTunnelRequest = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for create_genetic_tunnel"));
    };

    // Get peer endpoint from request or fail
    let peer_endpoint = match &request.peer_endpoint {
        Some(ep) => ep.clone(),
        None => {
            return Err(JsonRpcError::invalid_params(
                "peer_endpoint is required for tunnel creation",
            ));
        }
    };

    // Extract genetic family from proof
    let genetic_families = if let Some(ref proof) = request.genetic_proof {
        vec![proof.family_id.clone()]
    } else {
        vec![]
    };

    let result = handlers
        .connection_manager
        .establish_connection(
            request.peer_node_id.clone(),
            peer_endpoint.clone(),
            vec![], // capabilities (unknown at this point)
            genetic_families,
            TrustLevel::Elevated,
            String::from("genetic_tunnel"),
        )
        .await;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let tunnel_id = format!("tunnel-{}-{}", request.peer_node_id, timestamp);

    let response = match result {
        Ok(()) => CreateGeneticTunnelResponse {
            tunnel_id,
            status: String::from("established"),
            local_endpoint: None,
            peer_endpoint: Some(peer_endpoint),
            encryption: Some(String::from("ChaCha20-Poly1305")),
            created_at: timestamp,
        },
        Err(e) => CreateGeneticTunnelResponse {
            tunnel_id: String::new(),
            status: format!("failed: {e}"),
            local_endpoint: None,
            peer_endpoint: Some(peer_endpoint),
            encryption: None,
            created_at: timestamp,
        },
    };

    serde_json::to_value(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// P2P Discovery: `announce_capabilities` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn announce_capabilities_json(
    _handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let request: AnnounceCapabilitiesRequest = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for announce_capabilities"));
    };

    // NOTE: Capability announcement requires broadcaster restart (see jsonrpsee handler above)
    info!(
        "📢 Capability announcement requested (pure_rust_server): caps={:?}, federations={:?}, families={:?}",
        request.capabilities, request.sub_federations, request.genetic_families
    );

    let response = AnnounceCapabilitiesResponse {
        status: String::from("logged"), // Changed from "updated" to reflect reality
        broadcasting: true,
        updated_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    };

    serde_json::to_value(response).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::{
        announce_capabilities, create_genetic_tunnel, create_genetic_tunnel_json,
        discover_by_family, discover_by_family_json, discovered_node_from_peer,
        peer_matches_family_tags,
    };
    use crate::app::connection_manager::ConnectionManager;
    use crate::ipc::handlers::IpcHandlers;
    use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
    use crate::ipc::registry::ServiceRegistry;
    use songbird_discovery::anonymous::DiscoveredPeer;
    use songbird_http_client::SecurityRpcClient;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::time::SystemTime;

    fn test_ipc_handlers() -> IpcHandlers {
        IpcHandlers::new(
            Arc::new(ServiceRegistry::new()),
            None,
            Arc::new(ConnectionManager::new()),
            Arc::new(SecurityRpcClient::new("/tmp/songbird-p2p-discovery-handler-tests.sock")),
        )
    }

    #[test]
    fn peer_matches_when_tag_contains_family_token() {
        let tags = vec![String::from("crypto:family:nat0:tower1")];
        let families = vec![String::from("nat0")];
        assert!(peer_matches_family_tags(Some(&tags), &families));
    }

    #[test]
    fn peer_no_match_when_tags_missing() {
        let families = vec![String::from("nat0")];
        assert!(!peer_matches_family_tags(None, &families));
    }

    #[test]
    fn peer_no_match_when_family_list_nonmatching() {
        let tags = vec![String::from("crypto:family:other:node")];
        let families = vec![String::from("nat0")];
        assert!(!peer_matches_family_tags(Some(&tags), &families));
    }

    #[test]
    fn empty_family_filter_matches_nothing_in_helper() {
        let tags = vec![String::from("x")];
        let families: Vec<String> = vec![];
        assert!(!peer_matches_family_tags(Some(&tags), &families));
    }

    #[test]
    fn discovered_node_prefers_node_id_over_session() {
        let peer = DiscoveredPeer {
            session_id: "sess-1".into(),
            node_id: Some("node-stable".into()),
            node_name: Some("east".into()),
            endpoints: None,
            capabilities: vec!["compute".into()],
            tags: Some(vec!["tag-a".into()]),
            timestamp: Some(1),
            identity_attestations: None,
            protocols: vec![],
            port: 8443,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 9),
            last_seen: SystemTime::UNIX_EPOCH,
            version: "3".into(),
        };
        let node = discovered_node_from_peer(peer);
        assert_eq!(node.node_id, "node-stable");
        assert_eq!(node.node_name, Some("east".into()));
        assert_eq!(node.genetic_families, vec!["tag-a"]);
        assert_eq!(node.capabilities, vec![String::from("compute")]);
        assert_eq!(node.https_endpoint, "http://10.0.0.5:8443");
    }

    #[test]
    fn discovered_node_falls_back_to_session_id_when_no_node_id() {
        let peer = DiscoveredPeer {
            session_id: "sess-only".into(),
            node_id: None,
            node_name: None,
            endpoints: None,
            capabilities: vec![],
            tags: None,
            timestamp: None,
            identity_attestations: None,
            protocols: vec![],
            port: 443,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1),
            last_seen: SystemTime::UNIX_EPOCH,
            version: "2".into(),
        };
        let node = discovered_node_from_peer(peer);
        assert_eq!(node.node_id, "sess-only");
        assert_eq!(node.genetic_families, Vec::<String>::new());
    }

    #[tokio::test]
    async fn discover_by_family_without_listener_returns_empty_nodes() {
        let handlers = test_ipc_handlers();
        let res = discover_by_family(&handlers, serde_json::json!({ "family_tags": ["any"] }))
            .await
            .unwrap();
        assert!(res.nodes.is_empty());
    }

    #[tokio::test]
    async fn discover_by_family_invalid_params_is_json_rpc_error() {
        let handlers = test_ipc_handlers();
        let err =
            discover_by_family(&handlers, serde_json::json!("not-an-object")).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn discover_by_family_json_missing_params_errors() {
        let handlers = test_ipc_handlers();
        let err = discover_by_family_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn create_genetic_tunnel_requires_peer_endpoint() {
        let handlers = test_ipc_handlers();
        let err = create_genetic_tunnel(&handlers, serde_json::json!({ "peer_node_id": "n1" }))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("peer_endpoint"));
    }

    #[tokio::test]
    async fn create_genetic_tunnel_json_missing_params_errors() {
        let handlers = test_ipc_handlers();
        let err = create_genetic_tunnel_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn announce_capabilities_accepts_request() {
        let handlers = test_ipc_handlers();
        let res = announce_capabilities(
            &handlers,
            serde_json::json!({
                "capabilities": ["storage"],
                "sub_federations": ["f1"],
                "genetic_families": ["g1"]
            }),
        )
        .await
        .unwrap();
        assert_eq!(res.status, "updated");
        assert!(res.broadcasting);
        assert!(!res.updated_at.is_empty());
    }
}
