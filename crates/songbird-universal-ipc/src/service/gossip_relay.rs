// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![forbid(unsafe_code)]

//! `gossip.*` — `MeshRelay` transport for swarmVine cross-gate gossip.
//!
//! Methods: `gossip.relay`, `gossip.inject`, `gossip.spread`, `gossip.subscribe`
//!
//! Enables cross-gate gossip propagation through songBird's `:7700` federation
//! mesh when swarmVine's direct TCP 7800 path is unreachable between gates.
//!
//! Flow:
//! 1. Local primal → songBird `gossip.subscribe {topic, endpoint}` (registers interest)
//! 2. Local swarmVine → songBird `gossip.relay {target_gate, topic, payload}`
//! 3. songBird resolves best path to target gate via `MeshHandler::mesh()`
//! 4. songBird POSTs `gossip.inject` to remote songBird on `:7700`
//! 5. Remote songBird injects into local swarmVine + delivers to subscribers

use super::IpcServiceHandler;
use serde_json::{Value, json};
use songbird_onion_relay::mesh::EndpointType;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};
#[cfg(unix)]
use tracing::warn;

impl IpcServiceHandler {
    /// Handle `gossip.relay` — relay a gossip payload to a target gate's swarmVine.
    ///
    /// Params:
    /// - `target_gate` (string, optional): node ID of the destination gate.
    ///   If absent, empty, or `"local"`, injects into local swarmVine only.
    /// - `topic` (string): gossip topic (e.g. `"tower"`, `"capability"`)
    /// - `key` (string, optional): dedup key for the gossip entry
    /// - `payload` (object): the gossip payload to propagate
    ///
    /// Returns `{relayed_to, status}` on success.
    pub(super) async fn handle_gossip_relay(&self, params: Value) -> Result<Value, String> {
        let target_gate = params.get("target_gate").and_then(Value::as_str).unwrap_or("local");

        let explicit_topic = params.get("topic").and_then(Value::as_str);
        let topic = explicit_topic
            .or_else(|| {
                params
                    .get("payload")
                    .and_then(|p| p.get("topic"))
                    .and_then(Value::as_str)
            })
            .ok_or(
                "Missing required field: topic (include top-level 'topic' or 'payload.topic')",
            )?;

        if explicit_topic.is_none() {
            info!(
                target: "songbird::gossip_relay",
                inferred_topic = topic,
                "mesh.relay compat: topic inferred from payload; callers should include top-level 'topic' field"
            );
        }

        let payload = params.get("payload").cloned().unwrap_or(Value::Null);

        let key = params.get("key").and_then(Value::as_str).unwrap_or("");

        if target_gate.is_empty() || target_gate == "local" {
            self.inject_gossip_locally(topic, key, &payload).await?;
            return Ok(json!({
                "relayed_to": "local",
                "status": "injected"
            }));
        }

        // Resolve path to target gate via mesh
        let mesh_guard = self.mesh_handler.mesh().await;
        let mesh = mesh_guard.as_ref().ok_or("Mesh not initialized — cannot relay gossip")?;

        let path = mesh
            .get_best_path(target_gate)
            .await
            .ok_or_else(|| format!("No path to gate '{target_gate}'"))?;

        let addr = match path.endpoint_type {
            EndpointType::Direct {
                addr,
            }
            | EndpointType::Local {
                addr,
            }
            | EndpointType::Overlay {
                addr,
                ..
            } => addr,
            EndpointType::FamilyRelay {
                ..
            }
            | EndpointType::TorOnion {
                ..
            } => {
                return Err(format!(
                    "No direct/LAN/overlay path to '{target_gate}' — relay/onion not supported for gossip"
                ));
            }
        };

        let url = songbird_types::constants::jsonrpc_endpoint_url(&addr);

        let our_node_id = self.mesh_handler.node_id_async().await;
        let request = json!({
            "jsonrpc": "2.0",
            "method": "gossip.inject",
            "params": {
                "topic": topic,
                "key": key,
                "payload": payload,
                "origin_gate": our_node_id,
            },
            "id": 1
        });

        debug!(
            target: "songbird::gossip_relay",
            target_gate,
            topic,
            url = %url,
            "relaying gossip via mesh"
        );

        post_gossip_relay(&url, &request).await?;

        Ok(json!({
            "relayed_to": target_gate,
            "via": url,
            "status": "relayed"
        }))
    }

    /// Handle `gossip.inject` — inject a gossip payload into local swarmVine
    /// AND deliver to all local topic subscribers.
    ///
    /// Called by remote songBird peers (via `:7700` federation) or directly by
    /// local primals that want to inject gossip without specifying a target gate.
    pub(super) async fn handle_gossip_inject(&self, params: Value) -> Result<Value, String> {
        let topic =
            params.get("topic").and_then(Value::as_str).ok_or("Missing required field: topic")?;

        let key = params.get("key").and_then(Value::as_str).unwrap_or("");

        let payload = params.get("payload").cloned().unwrap_or(Value::Null);

        let origin = params.get("origin_gate").and_then(Value::as_str).unwrap_or("unknown");

        debug!(
            target: "songbird::gossip_relay",
            topic,
            origin,
            "injecting gossip into local swarmVine + delivering to subscribers"
        );

        self.inject_gossip_locally(topic, key, &payload).await?;

        let delivered = self.deliver_to_subscribers(topic, key, &payload, origin).await;

        Ok(json!({
            "status": "injected",
            "topic": topic,
            "origin_gate": origin,
            "subscribers_notified": delivered,
        }))
    }

    /// Handle `gossip.subscribe` — register a local primal's interest in a gossip topic.
    ///
    /// Params:
    /// - `topic` (string): gossip topic to subscribe to
    /// - `primal_id` (string): subscriber's primal identifier
    /// - `endpoint` (string): UDS path where gossip payloads will be delivered
    ///
    /// Returns `{subscription_id, topic, status}`.
    pub(super) async fn handle_gossip_subscribe(&self, params: Value) -> Result<Value, String> {
        let topic =
            params.get("topic").and_then(Value::as_str).ok_or("Missing required field: topic")?;

        let primal_id = params
            .get("primal_id")
            .and_then(Value::as_str)
            .ok_or("Missing required field: primal_id")?;

        let endpoint = params
            .get("endpoint")
            .and_then(Value::as_str)
            .ok_or("Missing required field: endpoint")?;

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let subscription_id = format!("gsub-{primal_id}-{topic}-{nonce:08x}");

        let sub = super::GossipSubscription {
            id: subscription_id.clone(),
            primal_id: Arc::from(primal_id),
            endpoint: std::path::PathBuf::from(endpoint),
            created: std::time::Instant::now(),
        };

        {
            let mut registry = self
                .gossip_subscriptions
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.subscribe(Arc::from(topic), sub);
        }

        debug!(
            target: "songbird::gossip_relay",
            topic,
            primal_id,
            subscription_id = %subscription_id,
            "gossip subscription registered"
        );

        Ok(json!({
            "subscription_id": subscription_id,
            "topic": topic,
            "primal_id": primal_id,
            "status": "subscribed"
        }))
    }

    /// Deliver a gossip payload to all local subscribers of a topic.
    async fn deliver_to_subscribers(
        &self,
        topic: &str,
        key: &str,
        payload: &Value,
        origin: &str,
    ) -> u32 {
        let subscribers = {
            let registry =
                self.gossip_subscriptions.read().unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.subscribers_for(topic)
        };

        if subscribers.is_empty() {
            return 0;
        }

        let mut delivered: u32 = 0;

        for sub in &subscribers {
            if self.deliver_single(sub, topic, key, payload, origin).await {
                delivered += 1;
            }
        }

        delivered
    }

    /// Deliver gossip to a single subscriber via their UDS endpoint.
    #[cfg(unix)]
    async fn deliver_single(
        &self,
        sub: &super::GossipSubscription,
        topic: &str,
        key: &str,
        payload: &Value,
        origin: &str,
    ) -> bool {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixStream;

        let rpc = json!({
            "jsonrpc": "2.0",
            "method": "gossip.deliver",
            "params": {
                "topic": topic,
                "key": key,
                "payload": payload,
                "origin_gate": origin,
                "subscription_id": sub.id,
            },
            "id": null
        });

        let timeout = Duration::from_secs(2);
        match tokio::time::timeout(timeout, UnixStream::connect(&sub.endpoint)).await {
            Ok(Ok(stream)) => {
                let (reader, mut writer) = stream.into_split();
                let Ok(mut msg) = serde_json::to_vec(&rpc) else {
                    return false;
                };
                msg.push(b'\n');
                if writer.write_all(&msg).await.is_ok() {
                    let mut response = String::new();
                    let mut buf_reader = BufReader::new(reader);
                    let _ = tokio::time::timeout(
                        Duration::from_secs(1),
                        buf_reader.read_line(&mut response),
                    )
                    .await;
                    true
                } else {
                    false
                }
            }
            Ok(Err(e)) => {
                debug!(
                    target: "songbird::gossip_relay",
                    primal = %sub.primal_id,
                    error = %e,
                    "failed to deliver gossip to subscriber"
                );
                false
            }
            Err(_) => {
                debug!(
                    target: "songbird::gossip_relay",
                    primal = %sub.primal_id,
                    "timeout delivering gossip to subscriber"
                );
                false
            }
        }
    }

    #[cfg(not(unix))]
    async fn deliver_single(
        &self,
        _sub: &super::GossipSubscription,
        _topic: &str,
        _key: &str,
        _payload: &Value,
        _origin: &str,
    ) -> bool {
        false
    }

    /// Handle `gossip.spread` — broadcast gossip to ALL reachable mesh peers.
    ///
    /// Epidemic fan-out: injects locally AND relays to every reachable federation
    /// peer. Prevents relay loops via `origin_gate` — peers that originated the
    /// gossip or already received it (tracked by `seen_gates`) are skipped.
    ///
    /// Params:
    /// - `topic` (string): gossip topic
    /// - `key` (string, optional): dedup key
    /// - `payload` (object): gossip payload
    /// - `origin_gate` (string, optional): originator gate (loop prevention)
    /// - `seen_gates` (array of strings, optional): gates that already have this entry
    pub(super) async fn handle_gossip_spread(&self, params: Value) -> Result<Value, String> {
        let topic =
            params.get("topic").and_then(Value::as_str).ok_or("Missing required field: topic")?;

        let key = params.get("key").and_then(Value::as_str).unwrap_or("");

        let payload = params.get("payload").cloned().unwrap_or(Value::Null);

        let origin = params.get("origin_gate").and_then(Value::as_str).unwrap_or("");

        let seen_gates: Vec<String> = params
            .get("seen_gates")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();

        self.inject_gossip_locally(topic, key, &payload).await?;

        let mesh_guard = self.mesh_handler.mesh().await;
        let Some(mesh) = mesh_guard.as_ref() else {
            return Ok(json!({
                "status": "local_only",
                "reason": "mesh not initialized",
                "spread_to": 0
            }));
        };

        let our_node_id = self.mesh_handler.node_id_async().await;
        let reachable = mesh.get_reachable_nodes().await;
        drop(mesh_guard);

        let inject_request = json!({
            "topic": topic,
            "key": key,
            "payload": payload,
            "origin_gate": our_node_id,
        });

        let (spread_count, failures) = self
            .fan_out_gossip(&reachable, origin, &seen_gates, &our_node_id, &inject_request)
            .await;

        let mut result = json!({
            "status": "spread",
            "topic": topic,
            "spread_to": spread_count,
            "local_injected": true,
        });

        if !failures.is_empty() {
            result["unreachable_peers"] = json!(failures);
        }

        Ok(result)
    }

    /// Fan-out a gossip payload to reachable peers, skipping origin and seen gates.
    async fn fan_out_gossip(
        &self,
        reachable: &[String],
        origin: &str,
        seen_gates: &[String],
        our_node_id: &str,
        inject_params: &Value,
    ) -> (u32, Vec<String>) {
        use songbird_onion_relay::mesh::EndpointType;

        let mut spread_count: u32 = 0;
        let mut failures: Vec<String> = Vec::new();

        let mut seen: std::collections::HashSet<&str> =
            seen_gates.iter().map(String::as_str).collect();
        if !origin.is_empty() {
            seen.insert(origin);
        }
        seen.insert(our_node_id);

        for peer_id in reachable {
            if seen.contains(peer_id.as_str()) {
                continue;
            }

            let mesh_guard = self.mesh_handler.mesh().await;
            let Some(mesh) = mesh_guard.as_ref() else {
                break;
            };
            let path = mesh.get_best_path(peer_id).await;
            drop(mesh_guard);

            let Some(path) = path else {
                continue;
            };

            let (EndpointType::Direct {
                addr,
            }
            | EndpointType::Local {
                addr,
            }
            | EndpointType::Overlay {
                addr,
                ..
            }) = path.endpoint_type
            else {
                continue;
            };

            let url = songbird_types::constants::jsonrpc_endpoint_url(&addr);

            let request = json!({
                "jsonrpc": "2.0",
                "method": "gossip.inject",
                "params": inject_params,
                "id": 1
            });

            match post_gossip_relay(&url, &request).await {
                Ok(()) => spread_count += 1,
                Err(e) => {
                    debug!(
                        target: "songbird::gossip_relay",
                        peer = %peer_id,
                        error = %e,
                        "spread failed to peer"
                    );
                    failures.push(peer_id.clone());
                }
            }
        }

        (spread_count, failures)
    }

    /// Handle `gossip.query` — query local swarmVine for gossip entries by topic/key prefix.
    ///
    /// This formalizes the ad-hoc UDS queries that content.rs and other modules
    /// were doing manually. Provides a proper JSON-RPC interface.
    ///
    /// Params:
    /// - `topic` (string): gossip topic to query (e.g. "defense", "data")
    /// - `key_prefix` (string, optional): filter entries by key prefix
    ///
    /// Returns `{entries: [{key, payload, origin_gate, timestamp}]}`.
    pub(super) async fn handle_gossip_query(&self, params: Value) -> Result<Value, String> {
        let topic = params
            .get("topic")
            .and_then(Value::as_str)
            .ok_or("Missing required field: topic")?;

        let key_prefix = params
            .get("key_prefix")
            .and_then(Value::as_str)
            .unwrap_or("");

        #[cfg(unix)]
        {
            use super::swarmvine_gossip::discover_swarmvine_socket;
            let Some(socket_path) = discover_swarmvine_socket() else {
                return Ok(json!({
                    "entries": [],
                    "status": "swarmvine_unavailable"
                }));
            };

            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::UnixStream;

            let rpc = json!({
                "jsonrpc": "2.0",
                "method": "gossip.query",
                "params": { "topic": topic, "key_prefix": key_prefix },
                "id": 1
            });

            let timeout_dur = Duration::from_secs(3);
            match tokio::time::timeout(timeout_dur, UnixStream::connect(&socket_path)).await {
                Ok(Ok(stream)) => {
                    let (reader, mut writer) = stream.into_split();
                    let _ = writer.write_all(&[0xEC, 0x01]).await;
                    let msg = format!("{rpc}\n");
                    let _ = writer.write_all(msg.as_bytes()).await;

                    let mut response = String::new();
                    let mut buf_reader = BufReader::new(reader);
                    match tokio::time::timeout(timeout_dur, buf_reader.read_line(&mut response)).await {
                        Ok(Ok(n)) if n > 0 => {
                            let resp: Value = serde_json::from_str(&response).unwrap_or_default();
                            let entries = resp
                                .get("result")
                                .and_then(|r| r.get("entries"))
                                .cloned()
                                .unwrap_or(json!([]));
                            Ok(json!({
                                "entries": entries,
                                "topic": topic,
                                "status": "ok"
                            }))
                        }
                        _ => Ok(json!({
                            "entries": [],
                            "status": "timeout"
                        })),
                    }
                }
                Ok(Err(e)) => Ok(json!({
                    "entries": [],
                    "status": format!("connect_error: {e}")
                })),
                Err(_) => Ok(json!({
                    "entries": [],
                    "status": "timeout"
                })),
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (topic, key_prefix);
            Ok(json!({
                "entries": [],
                "status": "not_available"
            }))
        }
    }

    /// Handle `gossip.pool` — communal immunity pooling.
    ///
    /// Queries the local swarmVine gossip table for ALL entries with topic
    /// "defense" and key prefix "epitope.feed:", then merges them into a
    /// unified immune surface.
    ///
    /// This is the inversion: every membrane contact point calls `gossip.pool`
    /// to get the combined behavioral genetics from across the entire mesh.
    /// Each node's epitope feed (their local observations) gets mixed with
    /// every other node's observations, creating communal immunity.
    ///
    /// Returns:
    /// ```json
    /// {
    ///   "schema": "ecoPrimals/communal-immunity/v1",
    ///   "pool_size": 3,
    ///   "nodes": ["sporeGate", "golgiBody", "northGate"],
    ///   "merged_clusters": [...],
    ///   "cluster_overlap": {...},
    ///   "coverage": {...}
    /// }
    /// ```
    pub(super) async fn handle_gossip_pool(&self, _params: Value) -> Result<Value, String> {
        // Query local gossip for all epitope feeds from the mesh
        let query_result = self.handle_gossip_query(json!({
            "topic": "defense",
            "key_prefix": "epitope.feed:"
        })).await?;

        let entries = query_result
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Merge epitope clusters from all contributing nodes
        let mut all_clusters: Vec<Value> = Vec::new();
        let mut contributing_nodes: Vec<String> = Vec::new();
        let mut total_observations: u64 = 0;
        let mut total_subgroups: u64 = 0;

        for entry in &entries {
            let payload = entry.get("payload").cloned().unwrap_or_default();

            let node_id = payload
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            contributing_nodes.push(node_id.clone());

            if let Some(clusters) = payload.get("epitope_clusters").and_then(Value::as_array) {
                for cluster in clusters {
                    let mut enriched = cluster.clone();
                    if let Some(obj) = enriched.as_object_mut() {
                        obj.insert("origin_node".to_string(), json!(node_id));
                    }
                    all_clusters.push(enriched);
                }
            }

            if let Some(pop) = payload.get("population") {
                total_observations += pop
                    .get("total_observations")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                total_subgroups += pop
                    .get("total_subgroups")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
            }
        }

        // Compute cluster overlap — which epitope hashes appear from multiple nodes
        let mut hash_sources: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for cluster in &all_clusters {
            if let Some(hash) = cluster.get("hash").and_then(Value::as_str) {
                let origin = cluster
                    .get("origin_node")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                hash_sources
                    .entry(hash.to_string())
                    .or_default()
                    .push(origin.to_string());
            }
        }

        let multi_node_clusters: Vec<Value> = hash_sources
            .iter()
            .filter(|(_, sources)| sources.len() > 1)
            .map(|(hash, sources)| {
                json!({
                    "epitope_hash": hash,
                    "seen_by": sources,
                    "confirmation_count": sources.len(),
                })
            })
            .collect();

        let unique_hashes: std::collections::HashSet<&str> = all_clusters
            .iter()
            .filter_map(|c| c.get("hash").and_then(Value::as_str))
            .collect();

        Ok(json!({
            "schema": "ecoPrimals/communal-immunity/v1",
            "generated_epoch": now,
            "pool_size": contributing_nodes.len(),
            "nodes": contributing_nodes,
            "merged_clusters": all_clusters,
            "cluster_overlap": {
                "multi_node_confirmations": multi_node_clusters,
                "confirmed_count": multi_node_clusters.len(),
            },
            "coverage": {
                "total_unique_epitopes": unique_hashes.len(),
                "total_cluster_entries": all_clusters.len(),
                "total_observations": total_observations,
                "total_subgroups": total_subgroups,
            },
            "usage": "Epitopes confirmed by multiple nodes have highest confidence. \
                      Feed these into your local classifier with weight proportional \
                      to confirmation_count. Single-node epitopes are local signals; \
                      multi-node epitopes are mesh-confirmed fleet signatures.",
        }))
    }

    /// Inject a gossip payload into the local swarmVine via UDS.
    ///
    /// Uses swarmVine's expected `0xEC 0x01` preamble followed by JSON-RPC.
    async fn inject_gossip_locally(
        &self,
        topic: &str,
        key: &str,
        payload: &Value,
    ) -> Result<(), String> {
        #[cfg(unix)]
        {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::UnixStream;

            let socket_path = super::swarmvine_gossip::discover_swarmvine_socket();
            let Some(socket_path) = socket_path else {
                warn!(
                    target: "songbird::gossip_relay",
                    topic,
                    "swarmVine not reachable — gossip injection skipped"
                );
                return Ok(());
            };

            let rpc = json!({
                "jsonrpc": "2.0",
                "method": "gossip.inject",
                "params": {
                    "topic": topic,
                    "key": key,
                    "payload": payload,
                },
                "id": 1
            });

            let inject_timeout = Duration::from_secs(3);
            match tokio::time::timeout(inject_timeout, UnixStream::connect(&socket_path)).await {
                Ok(Ok(stream)) => {
                    let (reader, mut writer) = stream.into_split();
                    // swarmVine expects local injection preamble: 0xEC 0x01
                    if writer.write_all(&[0xEC, 0x01]).await.is_err() {
                        return Ok(());
                    }
                    let Ok(mut msg) = serde_json::to_vec(&rpc) else {
                        return Ok(());
                    };
                    msg.push(b'\n');
                    if writer.write_all(&msg).await.is_ok() {
                        let mut response = String::new();
                        let mut buf_reader = BufReader::new(reader);
                        let _ = tokio::time::timeout(
                            Duration::from_secs(2),
                            buf_reader.read_line(&mut response),
                        )
                        .await;
                        debug!(
                            target: "songbird::gossip_relay",
                            topic,
                            "gossip injected into swarmVine"
                        );
                    }
                }
                Ok(Err(e)) => {
                    warn!(
                        target: "songbird::gossip_relay",
                        topic,
                        error = %e,
                        "failed to connect to swarmVine socket"
                    );
                }
                Err(_) => {
                    warn!(
                        target: "songbird::gossip_relay",
                        topic,
                        "timeout connecting to swarmVine socket"
                    );
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (topic, key, payload);
            debug!(
                target: "songbird::gossip_relay",
                "gossip injection not available on this platform (UDS-only)"
            );
        }

        Ok(())
    }
}

/// POST a JSON-RPC request to a remote songBird peer for gossip relay.
async fn post_gossip_relay(url: &str, body: &Value) -> Result<(), String> {
    use http_body_util::Full;
    use hyper::Request;
    use hyper::body::Bytes;
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;

    let body_bytes = serde_json::to_vec(body).map_err(|e| format!("Serialize: {e}"))?;
    let uri: hyper::Uri = url.parse().map_err(|e| format!("URI: {e}"))?;

    let request = Request::builder()
        .method(hyper::Method::POST)
        .uri(&uri)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| format!("Build request: {e}"))?;

    let client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

    let timeout = Duration::from_secs(5);
    let response = tokio::time::timeout(timeout, client.request(request))
        .await
        .map_err(|_| format!("Timeout relaying gossip to {url}"))?
        .map_err(|e| format!("HTTP error relaying to {url}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Remote peer {url} returned HTTP {status}"));
    }

    Ok(())
}
