// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![forbid(unsafe_code)]

//! `gossip.*` method dispatch — `MeshRelay` transport for swarmVine gossip.
//!
//! songBird relays gossip payloads across gates via its `:7700` federation mesh
//! when swarmVine's direct TCP 7800 path is unreachable. This is the transport
//! variant that closes the "ant colony" gap: swarmVine owns epidemic propagation,
//! songBird owns cross-gate transport.

use super::super::IpcServiceHandler;
use serde_json::Value;
use songbird_types::json_rpc_method::{GossipMethod, JsonRpcMethod};

pub(super) async fn dispatch_gossip(
    handler: &IpcServiceHandler,
    method: JsonRpcMethod,
    params: Value,
) -> Result<Value, String> {
    match method {
        JsonRpcMethod::Gossip(GossipMethod::Relay) => handler.handle_gossip_relay(params).await,
        JsonRpcMethod::Gossip(GossipMethod::Inject) => handler.handle_gossip_inject(params).await,
        JsonRpcMethod::Gossip(GossipMethod::Spread) => handler.handle_gossip_spread(params).await,
        JsonRpcMethod::Gossip(GossipMethod::Subscribe) => {
            handler.handle_gossip_subscribe(params).await
        }
        JsonRpcMethod::Gossip(GossipMethod::Query) => handler.handle_gossip_query(params).await,
        JsonRpcMethod::Gossip(GossipMethod::Pool) => handler.handle_gossip_pool(params).await,
        other => Err(format!("Unknown gossip method: {other}")),
    }
}
