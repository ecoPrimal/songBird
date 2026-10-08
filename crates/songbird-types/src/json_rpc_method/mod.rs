// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![expect(
    missing_docs,
    reason = "variant names mirror `domain.verb` wire strings — self-documenting"
)]
#![expect(
    clippy::too_many_lines,
    reason = "`as_wire_str` / `from_wire_str` are exhaustive mechanical dispatch tables"
)]

//! Typed JSON-RPC 2.0 method names (`domain.verb`) for Songbird dispatch.
//!
//! Wire format stays plain strings; this module maps those strings to enums for
//! exhaustive routing while [`std::fmt::Display`] / serde serialize back to canonical names.

use core::fmt;
use core::str::FromStr;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

mod domain_methods;

pub use domain_methods::*;

/// Normalize known aliases before parsing (same rules as `songbird-universal-ipc` introspection).
///
/// Also maps NEST capability tokens to their primary callable method so that
/// calling e.g. `network.discovery` dispatches to `discovery.peers` instead of
/// returning "unknown JSON-RPC method".
#[must_use]
pub fn normalize_json_rpc_method_name(method: &str) -> &str {
    match method {
        "capabilities.list" => "capability.list",
        "capabilities.methods" => "capability.methods",

        // Legacy flat-namespace methods → canonical domain.verb (Wave migration)
        "discover_capabilities" => "capabilities.discover",
        "discover_by_family" => "discovery.by_family",
        "discover_by_capability" => "discovery.by_capability",
        "announce_capabilities" => "capabilities.announce",
        "create_genetic_tunnel" => "tunnel.create_genetic",
        "get_service_health" => "health.service",
        "encrypt_discovery" => "discovery.encrypt",
        "decrypt_discovery" => "discovery.decrypt",

        // healthSpring §3: canonical name is `ipc.discover` — absorbs all aliases
        "capability.discover"
        | "discovery.find_by_capability"
        | "net.discovery.find_by_capability" => "ipc.discover",

        "ping" => "health.liveness",
        "register_service" => "ipc.register",
        "ipc.resolve_by_name" => "ipc.resolve",
        "health_check" | "status" | "check" | "health" => "health.check",

        // Tower Atomic aliases — biomeOS signal graph compatibility
        "tower.enroll" => "mesh.enroll",

        // swarmVine gossip namespace — mesh.* wire names alias to gossip.* handlers
        "mesh.relay" => "gossip.relay",
        "mesh.inject" => "gossip.inject",
        "mesh.spread" => "gossip.spread",
        "mesh.subscribe" => "gossip.subscribe",

        // Canonical inference namespace (inference.* is canonical; model.*/ai.* are aliases)
        "model.infer" | "ai.infer" | "ai.inference" => "inference.infer",
        "model.status" | "ai.status" => "inference.status",
        "model.list" | "ai.list" => "inference.list",
        "model.load" | "ai.load" => "inference.load",

        // NEST capability tokens → primary callable method
        "network.discovery" => "discovery.peers",
        "network.federation" => "songbird.federation.peers",
        "network.relay" => "relay.status",
        "network.stun" => "stun.status",
        "network.igd" => "igd.status",
        "network.tls" => "http.request",
        "network.tor" => "tor.status",
        "network.onion" => "onion.status",
        "ipc.jsonrpc" | "ipc.tarpc" => "rpc.methods",
        "network.btsp" => "btsp.capabilities",
        "network.quic" | "crypto.delegate" | "nfc.genesis" | "bluetooth.pair" => "health.readiness",

        other => other,
    }
}

/// Full method table for Songbird JSON-RPC dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonRpcMethod {
    Primal(PrimalMethod),
    Rpc(RpcMethod),
    Identity,
    IdentityGet(IdentityMethod),
    /// Raw `health` on the orchestrator Unix socket (biomeOS); not `health.check`.
    BiomeOsHealth,
    Health(HealthMethod),
    Tower(TowerMethod),
    Acme(AcmeMethod),
    Capabilities(CapabilitiesMethod),
    Ipc(IpcMethod),
    Http(HttpMethod),
    Stun(StunMethod),
    Igd(IgdMethod),
    Relay(RelayMethod),
    Discovery(DiscoveryMethod),
    Rendezvous(RendezvousMethod),
    Peer(PeerMethod),
    Birdsong(BirdsongMethod),
    Mesh(MeshMethod),
    Gossip(GossipMethod),
    Punch(PunchMethod),
    Onion(OnionMethod),
    Federation(FederationMethod),
    Tor(TorMethod),
    Compute(ComputeMethod),
    SongbirdCompute(SongbirdComputeMethod),
    Deployment(DeploymentMethod),
    Task(TaskMethod),
    Consent(ConsentMethod),
    Registry(RegistryMethod),
    Route(RouteMethod),
    Btsp(BtspMethod),
    Protocol(ProtocolMethod),
    SongbirdServices(SongbirdServicesMethod),
    Songbird(SongbirdMethod),
    Network(NetworkMethod),
    Storage(StorageMethod),
    Lifecycle(LifecycleMethod),
    Inference(InferenceMethod),
    Graph(GraphMethod),
    Coordination(CoordinationMethod),
    Tunnel(TunnelMethod),
    Content(ContentMethod),
}

/// Failed to map a wire string to [`JsonRpcMethod`].
///
/// The inner string is the full human-readable message (including the
/// `"unknown JSON-RPC method: "` prefix) so callers can move it into a
/// JSON-RPC error without an extra allocation via [`JsonRpcMethodParseError::into_message`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct JsonRpcMethodParseError(pub String);

impl JsonRpcMethodParseError {
    /// Consume the error and return the owned message string (no extra allocation).
    #[must_use]
    pub fn into_message(self) -> String {
        self.0
    }
}

impl JsonRpcMethod {
    /// Primary wire name for this method (used by [`std::fmt::Display`] and JSON serialization).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Primal(PrimalMethod::Info) => "primal.info",
            Self::Primal(PrimalMethod::Capabilities) => "primal.capabilities",
            Self::Primal(PrimalMethod::Register) => "primal.register",
            Self::Primal(PrimalMethod::Unregister) => "primal.unregister",
            Self::Primal(PrimalMethod::GetProvider) => "primal.get_provider",
            Self::Primal(PrimalMethod::ListProviders) => "primal.list_providers",
            Self::Primal(PrimalMethod::ListAll) => "primal.list_all",
            Self::Primal(PrimalMethod::Health) => "primal.health",
            Self::Primal(PrimalMethod::Ping) => "primal.ping",
            Self::Primal(PrimalMethod::Announce) => "primal.announce",
            Self::Rpc(RpcMethod::Methods) => "rpc.methods",
            Self::Rpc(RpcMethod::Discover) => "rpc.discover",
            Self::Identity => "identity",
            Self::IdentityGet(IdentityMethod::Get) => "identity.get",
            Self::BiomeOsHealth => "health",
            Self::Health(HealthMethod::Liveness) => "health.liveness",
            Self::Health(HealthMethod::Readiness) => "health.readiness",
            Self::Health(HealthMethod::Check) => "health.check",
            Self::Health(HealthMethod::Ping) => "health.ping",
            Self::Health(HealthMethod::Service) => "health.service",
            Self::Tower(TowerMethod::Health) => "tower.health",
            Self::Tower(TowerMethod::MeshStatus) => "tower.mesh_status",
            Self::Acme(AcmeMethod::ChallengeReady) => "acme.challenge_ready",
            Self::Acme(AcmeMethod::ChallengeCleanup) => "acme.challenge_cleanup",
            Self::Capabilities(CapabilitiesMethod::List) => "capability.list",
            Self::Capabilities(CapabilitiesMethod::Methods) => "capability.methods",
            Self::Capabilities(CapabilitiesMethod::Discover) => "capabilities.discover",
            Self::Capabilities(CapabilitiesMethod::Announce) => "capabilities.announce",
            Self::Capabilities(CapabilitiesMethod::Resolve) => "capability.resolve",
            Self::Capabilities(CapabilitiesMethod::Call) => "capability.call",
            Self::Capabilities(CapabilitiesMethod::Health) => "capability.health",
            Self::Ipc(IpcMethod::Register) => "ipc.register",
            Self::Ipc(IpcMethod::Resolve) => "ipc.resolve",
            Self::Ipc(IpcMethod::Discover) => "ipc.discover",
            Self::Ipc(IpcMethod::List) => "ipc.list",
            Self::Ipc(IpcMethod::FindCapability) => "ipc.find_capability",
            Self::Ipc(IpcMethod::Heartbeat) => "ipc.heartbeat",
            Self::Ipc(IpcMethod::Watch) => "ipc.watch",
            Self::Ipc(IpcMethod::RelayStats) => "ipc.relay_stats",
            Self::Http(HttpMethod::Request) => "http.request",
            Self::Http(HttpMethod::Get) => "http.get",
            Self::Http(HttpMethod::Post) => "http.post",
            Self::Http(HttpMethod::Put) => "http.put",
            Self::Http(HttpMethod::Delete) => "http.delete",
            Self::Http(HttpMethod::Proxy) => "http.proxy",
            Self::Stun(StunMethod::Serve) => "stun.serve",
            Self::Stun(StunMethod::Stop) => "stun.stop",
            Self::Stun(StunMethod::Status) => "stun.status",
            Self::Stun(StunMethod::GetPublicAddress) => "stun.get_public_address",
            Self::Stun(StunMethod::Bind) => "stun.bind",
            Self::Stun(StunMethod::ProbePortPattern) => "stun.probe_port_pattern",
            Self::Stun(StunMethod::DetectNatType) => "stun.detect_nat_type",
            Self::Igd(IgdMethod::Discover) => "igd.discover",
            Self::Igd(IgdMethod::MapPort) => "igd.map_port",
            Self::Igd(IgdMethod::UnmapPort) => "igd.unmap_port",
            Self::Igd(IgdMethod::Status) => "igd.status",
            Self::Igd(IgdMethod::ExternalIp) => "igd.external_ip",
            Self::Igd(IgdMethod::AutoConfigure) => "igd.auto_configure",
            Self::Relay(RelayMethod::Serve) => "relay.serve",
            Self::Relay(RelayMethod::Stop) => "relay.stop",
            Self::Relay(RelayMethod::Status) => "relay.status",
            Self::Relay(RelayMethod::Allocate) => "relay.allocate",
            Self::Relay(RelayMethod::Forward) => "relay.forward",
            Self::Discovery(DiscoveryMethod::Peers) => "discovery.peers",
            Self::Discovery(DiscoveryMethod::Announce) => "discovery.announce",
            Self::Discovery(DiscoveryMethod::ContentPeers) => "discovery.content_peers",
            Self::Discovery(DiscoveryMethod::ListPeers) => "discovery.list_peers",
            Self::Discovery(DiscoveryMethod::PeerCount) => "discovery.peer_count",
            Self::Discovery(DiscoveryMethod::RejectedPeers) => "discovery.rejected_peers",
            Self::Discovery(DiscoveryMethod::Status) => "discovery.status",
            Self::Discovery(DiscoveryMethod::Topology) => "discovery.topology",
            Self::Discovery(DiscoveryMethod::Health) => "discovery.health",
            Self::Discovery(DiscoveryMethod::Query) => "discovery.query",
            Self::Discovery(DiscoveryMethod::Bonds) => "discovery.bonds",
            Self::Discovery(DiscoveryMethod::ByFamily) => "discovery.by_family",
            Self::Discovery(DiscoveryMethod::ByCapability) => "discovery.by_capability",
            Self::Discovery(DiscoveryMethod::Encrypt) => "discovery.encrypt",
            Self::Discovery(DiscoveryMethod::Decrypt) => "discovery.decrypt",
            Self::Rendezvous(RendezvousMethod::Register) => "rendezvous.register",
            Self::Rendezvous(RendezvousMethod::Lookup) => "rendezvous.lookup",
            Self::Peer(PeerMethod::Connect) => "peer.connect",
            Self::Peer(PeerMethod::Ping) => "peer.ping",
            Self::Birdsong(BirdsongMethod::GenerateEncryptedBeacon) => {
                "birdsong.generate_encrypted_beacon"
            }
            Self::Birdsong(BirdsongMethod::DecryptBeacon) => "birdsong.decrypt_beacon",
            Self::Birdsong(BirdsongMethod::VerifyLineage) => "birdsong.verify_lineage",
            Self::Birdsong(BirdsongMethod::GetLineage) => "birdsong.get_lineage",
            Self::Birdsong(BirdsongMethod::Advertise) => "birdsong.advertise",
            Self::Birdsong(BirdsongMethod::Schema) => "birdsong.schema",
            Self::Mesh(MeshMethod::Init) => "mesh.init",
            Self::Mesh(MeshMethod::Status) => "mesh.status",
            Self::Mesh(MeshMethod::FindPath) => "mesh.find_path",
            Self::Mesh(MeshMethod::Announce) => "mesh.announce",
            Self::Mesh(MeshMethod::Peers) => "mesh.peers",
            Self::Mesh(MeshMethod::Topology) => "mesh.topology",
            Self::Mesh(MeshMethod::HealthCheck) => "mesh.health_check",
            Self::Mesh(MeshMethod::AutoDiscover) => "mesh.auto_discover",
            Self::Mesh(MeshMethod::DiscoverRemotes) => "mesh.discover_remotes",
            Self::Mesh(MeshMethod::Mirror) => "mesh.mirror",
            Self::Mesh(MeshMethod::Publish) => "mesh.publish",
            Self::Mesh(MeshMethod::Subscribe) => "mesh.subscribe",
            Self::Mesh(MeshMethod::ProbeLatency) => "mesh.probe_latency",
            Self::Mesh(MeshMethod::CapabilitiesAnnounce) => "mesh.capabilities_announce",
            Self::Mesh(MeshMethod::CapabilitiesRevoke) => "mesh.capabilities_revoke",
            Self::Mesh(MeshMethod::Enroll) => "mesh.enroll",
            Self::Mesh(MeshMethod::GateEnroll) => "mesh.gate_enroll",
            Self::Mesh(MeshMethod::PruneStale) => "mesh.prune_stale",
            Self::Mesh(MeshMethod::ConnectivityCheck) => "mesh.connectivity_check",
            Self::Mesh(MeshMethod::Throughput) => "mesh.throughput",
            Self::Gossip(GossipMethod::Relay) => "gossip.relay",
            Self::Gossip(GossipMethod::Inject) => "gossip.inject",
            Self::Gossip(GossipMethod::Spread) => "gossip.spread",
            Self::Gossip(GossipMethod::Subscribe) => "gossip.subscribe",
            Self::Gossip(GossipMethod::Query) => "gossip.query",
            Self::Gossip(GossipMethod::Pool) => "gossip.pool",
            Self::Punch(PunchMethod::Request) => "punch.request",
            Self::Punch(PunchMethod::Coordinate) => "punch.coordinate",
            Self::Punch(PunchMethod::Status) => "punch.status",
            Self::Onion(OnionMethod::Start) => "onion.start",
            Self::Onion(OnionMethod::Stop) => "onion.stop",
            Self::Onion(OnionMethod::Status) => "onion.status",
            Self::Onion(OnionMethod::Connect) => "onion.connect",
            Self::Onion(OnionMethod::Address) => "onion.address",
            Self::Federation(FederationMethod::Peers) => "songbird.federation.peers",
            Self::Federation(FederationMethod::Status) => "songbird.federation.status",
            Self::Federation(FederationMethod::Join) => "songbird.federation.join",
            Self::Federation(FederationMethod::Broadcast) => "federation.broadcast",
            Self::Tor(TorMethod::Status) => "tor.status",
            Self::Tor(TorMethod::Connect) => "tor.connect",
            Self::Tor(TorMethod::ServiceStart) => "tor.service.start",
            Self::Tor(TorMethod::ServiceStop) => "tor.service.stop",
            Self::Tor(TorMethod::ConsensusFetch) => "tor.consensus.fetch",
            Self::Tor(TorMethod::CircuitBuild) => "tor.circuit.build",
            Self::Tor(TorMethod::CircuitClose) => "tor.circuit.close",
            Self::Compute(ComputeMethod::Route) => "compute.route",
            Self::SongbirdCompute(SongbirdComputeMethod::Schedule) => "songbird.compute.schedule",
            Self::SongbirdCompute(SongbirdComputeMethod::Status) => "songbird.compute.status",
            Self::Deployment(DeploymentMethod::Create) => "deployment.create",
            Self::Deployment(DeploymentMethod::Status) => "deployment.status",
            Self::Deployment(DeploymentMethod::HotSwap) => "deployment.hot_swap",
            Self::Deployment(DeploymentMethod::Restart) => "deployment.restart",
            Self::Deployment(DeploymentMethod::List) => "deployment.list",
            Self::Task(TaskMethod::Create) => "task.create",
            Self::Task(TaskMethod::List) => "task.list",
            Self::Consent(ConsentMethod::Check) => "consent.check",
            Self::Consent(ConsentMethod::Grant) => "consent.grant",
            Self::Registry(RegistryMethod::Register) => "registry.register",
            Self::Registry(RegistryMethod::Discover) => "registry.discover",
            Self::Route(RouteMethod::Add) => "route.add",
            Self::Route(RouteMethod::Remove) => "route.remove",
            Self::Route(RouteMethod::List) => "route.list",
            Self::Btsp(BtspMethod::Negotiate) => "btsp.negotiate",
            Self::Btsp(BtspMethod::Capabilities) => "btsp.capabilities",
            Self::Protocol(ProtocolMethod::Negotiate) => "protocol.negotiate",
            Self::Protocol(ProtocolMethod::Capabilities) => "songbird.protocol.capabilities",
            Self::SongbirdServices(SongbirdServicesMethod::List) => "songbird.services.list",
            Self::SongbirdServices(SongbirdServicesMethod::Get) => "songbird.services.get",
            Self::SongbirdServices(SongbirdServicesMethod::Register) => {
                "songbird.services.register"
            }
            Self::Songbird(SongbirdMethod::Health) => "songbird.health",
            Self::Songbird(SongbirdMethod::Version) => "songbird.version",
            Self::Network(NetworkMethod::BeaconExchange) => "network.beacon_exchange",
            Self::Network(NetworkMethod::Broadcast) => "network.broadcast",
            Self::Network(NetworkMethod::Listen) => "network.listen",
            Self::Storage(StorageMethod::Get) => "storage.get",
            Self::Storage(StorageMethod::Put) => "storage.put",
            Self::Storage(StorageMethod::Delete) => "storage.delete",
            Self::Storage(StorageMethod::List) => "storage.list",
            Self::Storage(StorageMethod::Flush) => "storage.flush",
            Self::Lifecycle(LifecycleMethod::Composition) => "lifecycle.composition",
            Self::Lifecycle(LifecycleMethod::ValidateConsumed) => "lifecycle.validate_consumed",
            Self::Inference(InferenceMethod::Infer) => "inference.infer",
            Self::Inference(InferenceMethod::Status) => "inference.status",
            Self::Inference(InferenceMethod::List) => "inference.list",
            Self::Inference(InferenceMethod::Load) => "inference.load",
            Self::Graph(GraphMethod::Validate) => "graph.validate",
            Self::Graph(GraphMethod::CheckAvailability) => "graph.check_availability",
            Self::Graph(GraphMethod::SuggestAlternatives) => "graph.suggest_alternatives",
            Self::Coordination(CoordinationMethod::ValidatePattern) => {
                "coordination.validate_pattern"
            }
            Self::Tunnel(TunnelMethod::CreateGenetic) => "tunnel.create_genetic",
            Self::Content(ContentMethod::Locate) => "content.locate",
            Self::Content(ContentMethod::Verify) => "content.verify",
            Self::Content(ContentMethod::Availability) => "content.availability",
            Self::Content(ContentMethod::Put) => "content.put",
        }
    }

    /// Parse a raw JSON-RPC `method` string (as sent on the wire before alias normalization).
    ///
    /// Use this for the orchestrator Unix socket, which routes bare `"health"` separately from
    /// `"health.check"`.
    ///
    /// # Errors
    ///
    /// Returns [`JsonRpcMethodParseError`] when the name is not recognized.
    pub fn from_wire_str(s: &str) -> Result<Self, JsonRpcMethodParseError> {
        Ok(match s {
            "primal.info" => Self::Primal(PrimalMethod::Info),
            "primal.capabilities" => Self::Primal(PrimalMethod::Capabilities),
            "primal.register" => Self::Primal(PrimalMethod::Register),
            "primal.unregister" => Self::Primal(PrimalMethod::Unregister),
            "primal.get_provider" => Self::Primal(PrimalMethod::GetProvider),
            "primal.list_providers" => Self::Primal(PrimalMethod::ListProviders),
            "primal.list_all" => Self::Primal(PrimalMethod::ListAll),
            "primal.health" => Self::Primal(PrimalMethod::Health),
            "primal.ping" => Self::Primal(PrimalMethod::Ping),
            "primal.announce" => Self::Primal(PrimalMethod::Announce),
            "rpc.methods" => Self::Rpc(RpcMethod::Methods),
            "rpc.discover" => Self::Rpc(RpcMethod::Discover),
            "capabilities.discover" | "discover_capabilities" => {
                Self::Capabilities(CapabilitiesMethod::Discover)
            }
            "identity" => Self::Identity,
            "identity.get" => Self::IdentityGet(IdentityMethod::Get),
            "health" => Self::BiomeOsHealth,
            "health.liveness" => Self::Health(HealthMethod::Liveness),
            "health.readiness" => Self::Health(HealthMethod::Readiness),
            "health.check" => Self::Health(HealthMethod::Check),
            "health.ping" => Self::Health(HealthMethod::Ping),
            "health.service" | "get_service_health" => Self::Health(HealthMethod::Service),
            "tower.health" => Self::Tower(TowerMethod::Health),
            "tower.mesh_status" => Self::Tower(TowerMethod::MeshStatus),
            "acme.challenge_ready" => Self::Acme(AcmeMethod::ChallengeReady),
            "acme.challenge_cleanup" => Self::Acme(AcmeMethod::ChallengeCleanup),
            "capability.list" | "capabilities.list" => Self::Capabilities(CapabilitiesMethod::List),
            "capability.methods" | "capabilities.methods" => {
                Self::Capabilities(CapabilitiesMethod::Methods)
            }
            "capabilities.announce" | "announce_capabilities" => {
                Self::Capabilities(CapabilitiesMethod::Announce)
            }
            "capability.resolve" => Self::Capabilities(CapabilitiesMethod::Resolve),
            "capability.call" => Self::Capabilities(CapabilitiesMethod::Call),
            "capability.health" => Self::Capabilities(CapabilitiesMethod::Health),
            "ipc.register" => Self::Ipc(IpcMethod::Register),
            "ipc.resolve" => Self::Ipc(IpcMethod::Resolve),
            "ipc.discover" => Self::Ipc(IpcMethod::Discover),
            "ipc.list" => Self::Ipc(IpcMethod::List),
            "ipc.find_capability" => Self::Ipc(IpcMethod::FindCapability),
            "ipc.heartbeat" => Self::Ipc(IpcMethod::Heartbeat),
            "ipc.watch" => Self::Ipc(IpcMethod::Watch),
            "ipc.relay_stats" => Self::Ipc(IpcMethod::RelayStats),
            "http.request" => Self::Http(HttpMethod::Request),
            "http.get" => Self::Http(HttpMethod::Get),
            "http.post" => Self::Http(HttpMethod::Post),
            "http.put" => Self::Http(HttpMethod::Put),
            "http.delete" => Self::Http(HttpMethod::Delete),
            "http.proxy" => Self::Http(HttpMethod::Proxy),
            "stun.serve" => Self::Stun(StunMethod::Serve),
            "stun.stop" => Self::Stun(StunMethod::Stop),
            "stun.status" => Self::Stun(StunMethod::Status),
            "stun.get_public_address" => Self::Stun(StunMethod::GetPublicAddress),
            "stun.bind" => Self::Stun(StunMethod::Bind),
            "stun.probe_port_pattern" => Self::Stun(StunMethod::ProbePortPattern),
            "stun.detect_nat_type" => Self::Stun(StunMethod::DetectNatType),
            "igd.discover" => Self::Igd(IgdMethod::Discover),
            "igd.map_port" => Self::Igd(IgdMethod::MapPort),
            "igd.unmap_port" => Self::Igd(IgdMethod::UnmapPort),
            "igd.status" => Self::Igd(IgdMethod::Status),
            "igd.external_ip" => Self::Igd(IgdMethod::ExternalIp),
            "igd.auto_configure" => Self::Igd(IgdMethod::AutoConfigure),
            "relay.serve" => Self::Relay(RelayMethod::Serve),
            "relay.stop" => Self::Relay(RelayMethod::Stop),
            "relay.status" => Self::Relay(RelayMethod::Status),
            "relay.allocate" => Self::Relay(RelayMethod::Allocate),
            "relay.forward" => Self::Relay(RelayMethod::Forward),
            "discovery.peers" | "discovery.find_primals" | "find_primals" => {
                Self::Discovery(DiscoveryMethod::Peers)
            }
            "discovery.announce" | "announce_presence" => {
                Self::Discovery(DiscoveryMethod::Announce)
            }
            "discovery.content_peers" => Self::Discovery(DiscoveryMethod::ContentPeers),
            "discovery.list_peers" => Self::Discovery(DiscoveryMethod::ListPeers),
            "discovery.peer_count" => Self::Discovery(DiscoveryMethod::PeerCount),
            "discovery.rejected_peers" => Self::Discovery(DiscoveryMethod::RejectedPeers),
            "discovery.status" => Self::Discovery(DiscoveryMethod::Status),
            "discovery.topology" => Self::Discovery(DiscoveryMethod::Topology),
            "discovery.health" => Self::Discovery(DiscoveryMethod::Health),
            "discovery.query" => Self::Discovery(DiscoveryMethod::Query),
            "discovery.bonds" => Self::Discovery(DiscoveryMethod::Bonds),
            "discovery.by_family" | "discover_by_family" => {
                Self::Discovery(DiscoveryMethod::ByFamily)
            }
            "discovery.by_capability" | "discover_by_capability" => {
                Self::Discovery(DiscoveryMethod::ByCapability)
            }
            "discovery.encrypt" | "encrypt_discovery" => Self::Discovery(DiscoveryMethod::Encrypt),
            "discovery.decrypt" | "decrypt_discovery" => Self::Discovery(DiscoveryMethod::Decrypt),
            "rendezvous.register" => Self::Rendezvous(RendezvousMethod::Register),
            "rendezvous.lookup" => Self::Rendezvous(RendezvousMethod::Lookup),
            "peer.connect" => Self::Peer(PeerMethod::Connect),
            "peer.ping" => Self::Peer(PeerMethod::Ping),
            "birdsong.generate_encrypted_beacon" => {
                Self::Birdsong(BirdsongMethod::GenerateEncryptedBeacon)
            }
            "birdsong.decrypt_beacon" => Self::Birdsong(BirdsongMethod::DecryptBeacon),
            "birdsong.verify_lineage" => Self::Birdsong(BirdsongMethod::VerifyLineage),
            "birdsong.get_lineage" => Self::Birdsong(BirdsongMethod::GetLineage),
            "birdsong.advertise" => Self::Birdsong(BirdsongMethod::Advertise),
            "birdsong.schema" => Self::Birdsong(BirdsongMethod::Schema),
            "mesh.init" => Self::Mesh(MeshMethod::Init),
            "mesh.status" => Self::Mesh(MeshMethod::Status),
            "mesh.find_path" => Self::Mesh(MeshMethod::FindPath),
            "mesh.announce" => Self::Mesh(MeshMethod::Announce),
            "mesh.peers" => Self::Mesh(MeshMethod::Peers),
            "mesh.topology" => Self::Mesh(MeshMethod::Topology),
            "mesh.health_check" => Self::Mesh(MeshMethod::HealthCheck),
            "mesh.auto_discover" => Self::Mesh(MeshMethod::AutoDiscover),
            "mesh.discover_remotes" => Self::Mesh(MeshMethod::DiscoverRemotes),
            "mesh.mirror" => Self::Mesh(MeshMethod::Mirror),
            "mesh.publish" => Self::Mesh(MeshMethod::Publish),
            "mesh.deliver" => Self::Mesh(MeshMethod::Subscribe),
            "mesh.probe_latency" => Self::Mesh(MeshMethod::ProbeLatency),
            "mesh.relay" | "gossip.relay" => Self::Gossip(GossipMethod::Relay),
            "mesh.inject" | "gossip.inject" => Self::Gossip(GossipMethod::Inject),
            "mesh.spread" | "gossip.spread" => Self::Gossip(GossipMethod::Spread),
            "mesh.subscribe" | "gossip.subscribe" => Self::Gossip(GossipMethod::Subscribe),
            "gossip.query" => Self::Gossip(GossipMethod::Query),
            "gossip.pool" | "defense.pool" => Self::Gossip(GossipMethod::Pool),
            "mesh.capabilities_announce" => Self::Mesh(MeshMethod::CapabilitiesAnnounce),
            "mesh.capabilities_revoke" => Self::Mesh(MeshMethod::CapabilitiesRevoke),
            "mesh.enroll" => Self::Mesh(MeshMethod::Enroll),
            "mesh.gate_enroll" => Self::Mesh(MeshMethod::GateEnroll),
            "mesh.prune_stale" => Self::Mesh(MeshMethod::PruneStale),
            "mesh.connectivity_check" => Self::Mesh(MeshMethod::ConnectivityCheck),
            "mesh.throughput" => Self::Mesh(MeshMethod::Throughput),
            "punch.request" => Self::Punch(PunchMethod::Request),
            "punch.coordinate" => Self::Punch(PunchMethod::Coordinate),
            "punch.status" => Self::Punch(PunchMethod::Status),
            "onion.start" => Self::Onion(OnionMethod::Start),
            "onion.stop" => Self::Onion(OnionMethod::Stop),
            "onion.status" => Self::Onion(OnionMethod::Status),
            "onion.connect" => Self::Onion(OnionMethod::Connect),
            "onion.address" => Self::Onion(OnionMethod::Address),
            "songbird.federation.peers" | "federation.peers" => {
                Self::Federation(FederationMethod::Peers)
            }
            "songbird.federation.status" | "federation.status" => {
                Self::Federation(FederationMethod::Status)
            }
            "songbird.federation.join" => Self::Federation(FederationMethod::Join),
            "federation.broadcast" | "songbird.federation.broadcast" => {
                Self::Federation(FederationMethod::Broadcast)
            }
            "tor.status" => Self::Tor(TorMethod::Status),
            "tor.connect" => Self::Tor(TorMethod::Connect),
            "tor.service.start" => Self::Tor(TorMethod::ServiceStart),
            "tor.service.stop" => Self::Tor(TorMethod::ServiceStop),
            "tor.consensus.fetch" => Self::Tor(TorMethod::ConsensusFetch),
            "tor.circuit.build" => Self::Tor(TorMethod::CircuitBuild),
            "tor.circuit.close" => Self::Tor(TorMethod::CircuitClose),
            "compute.route" => Self::Compute(ComputeMethod::Route),
            "songbird.compute.schedule" => Self::SongbirdCompute(SongbirdComputeMethod::Schedule),
            "songbird.compute.status" => Self::SongbirdCompute(SongbirdComputeMethod::Status),
            "deployment.create" => Self::Deployment(DeploymentMethod::Create),
            "deployment.status" => Self::Deployment(DeploymentMethod::Status),
            "deployment.hot_swap" => Self::Deployment(DeploymentMethod::HotSwap),
            "deployment.restart" => Self::Deployment(DeploymentMethod::Restart),
            "deployment.list" => Self::Deployment(DeploymentMethod::List),
            "task.create" => Self::Task(TaskMethod::Create),
            "task.list" => Self::Task(TaskMethod::List),
            "consent.check" => Self::Consent(ConsentMethod::Check),
            "consent.grant" => Self::Consent(ConsentMethod::Grant),
            "registry.register" => Self::Registry(RegistryMethod::Register),
            "registry.discover" => Self::Registry(RegistryMethod::Discover),
            "route.add" => Self::Route(RouteMethod::Add),
            "route.remove" => Self::Route(RouteMethod::Remove),
            "route.list" => Self::Route(RouteMethod::List),
            "btsp.negotiate" => Self::Btsp(BtspMethod::Negotiate),
            "btsp.capabilities" => Self::Btsp(BtspMethod::Capabilities),
            "protocol.negotiate" => Self::Protocol(ProtocolMethod::Negotiate),
            "songbird.protocol.capabilities" => Self::Protocol(ProtocolMethod::Capabilities),
            "songbird.services.list" => Self::SongbirdServices(SongbirdServicesMethod::List),
            "songbird.services.get" => Self::SongbirdServices(SongbirdServicesMethod::Get),
            "songbird.services.register" => {
                Self::SongbirdServices(SongbirdServicesMethod::Register)
            }
            "songbird.health" => Self::Songbird(SongbirdMethod::Health),
            "songbird.version" => Self::Songbird(SongbirdMethod::Version),
            "network.beacon_exchange" => Self::Network(NetworkMethod::BeaconExchange),
            "network.broadcast" => Self::Network(NetworkMethod::Broadcast),
            "network.listen" => Self::Network(NetworkMethod::Listen),
            "storage.get" => Self::Storage(StorageMethod::Get),
            "storage.put" => Self::Storage(StorageMethod::Put),
            "storage.delete" => Self::Storage(StorageMethod::Delete),
            "storage.list" => Self::Storage(StorageMethod::List),
            "storage.flush" => Self::Storage(StorageMethod::Flush),
            "lifecycle.composition" => Self::Lifecycle(LifecycleMethod::Composition),
            "lifecycle.validate_consumed" => Self::Lifecycle(LifecycleMethod::ValidateConsumed),
            "inference.infer" => Self::Inference(InferenceMethod::Infer),
            "inference.status" => Self::Inference(InferenceMethod::Status),
            "inference.list" => Self::Inference(InferenceMethod::List),
            "inference.load" => Self::Inference(InferenceMethod::Load),
            "graph.validate" => Self::Graph(GraphMethod::Validate),
            "graph.check_availability" => Self::Graph(GraphMethod::CheckAvailability),
            "graph.suggest_alternatives" => Self::Graph(GraphMethod::SuggestAlternatives),
            "coordination.validate_pattern" => {
                Self::Coordination(CoordinationMethod::ValidatePattern)
            }
            "tunnel.create_genetic" | "create_genetic_tunnel" => {
                Self::Tunnel(TunnelMethod::CreateGenetic)
            }
            "content.locate" => Self::Content(ContentMethod::Locate),
            "content.verify" => Self::Content(ContentMethod::Verify),
            "content.availability" => Self::Content(ContentMethod::Availability),
            "content.put" => Self::Content(ContentMethod::Put),
            _ => {
                return Err(JsonRpcMethodParseError(format!("unknown JSON-RPC method: {s}")));
            }
        })
    }

    /// Parse after applying [`normalize_json_rpc_method_name`] (IPC broker and HTTP gateway).
    ///
    /// # Errors
    ///
    /// Returns [`JsonRpcMethodParseError`] when the name is not recognized.
    pub fn parse_ipc(method: &str) -> Result<Self, JsonRpcMethodParseError> {
        let normalized = normalize_json_rpc_method_name(method);
        if normalized != method {
            tracing::warn!(
                legacy_method = method,
                canonical_method = normalized,
                "deprecated JSON-RPC method name; migrate to canonical domain.verb form"
            );
        }
        Self::from_wire_str(normalized)
    }
}

impl fmt::Display for JsonRpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

impl FromStr for JsonRpcMethod {
    type Err = JsonRpcMethodParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_wire_str(s)
    }
}

impl Serialize for JsonRpcMethod {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire_str())
    }
}

/// Deserialize via `deserialize_str` so serde can borrow wire names without allocating a `String`
/// when the underlying format supports it.
struct JsonRpcMethodDeserializeVisitor;

impl<'de> Visitor<'de> for JsonRpcMethodDeserializeVisitor {
    type Value = JsonRpcMethod;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON-RPC method name string")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        JsonRpcMethod::from_str(v).map_err(de::Error::custom)
    }

    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        JsonRpcMethod::from_str(v).map_err(de::Error::custom)
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        JsonRpcMethod::from_str(&v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for JsonRpcMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(JsonRpcMethodDeserializeVisitor)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod json_rpc_method_tests;
