// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

#![allow(missing_docs, reason = "variant names mirror `domain.verb` wire strings")]

/// `primal.*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimalMethod {
    /// `primal.info`
    Info,
    /// `primal.capabilities`
    Capabilities,
    /// `primal.register`
    Register,
    /// `primal.unregister`
    Unregister,
    /// `primal.get_provider`
    GetProvider,
    /// `primal.list_providers`
    ListProviders,
    /// `primal.list_all`
    ListAll,
    /// `primal.health`
    Health,
    /// `primal.ping`
    Ping,
    /// `primal.announce` — atomic registration replacing lifecycle/capability/method register (biomeOS v3.57)
    Announce,
}

/// `rpc.*` introspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RpcMethod {
    /// `rpc.methods`
    Methods,
    /// `rpc.discover`
    Discover,
}

/// `health.*` (normalized ecosystem standard).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthMethod {
    /// `health.liveness`
    Liveness,
    /// `health.readiness`
    Readiness,
    /// `health.check`
    Check,
    /// `health.ping` — RTT probe for latency measurement.
    Ping,
    /// `health.service` — check registered service/primal health by ID
    Service,
}

/// `tower.*` — Tower Atomic stack health facade for biomeOS signal graphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TowerMethod {
    /// `tower.health` — aggregate Tower Atomic stack health
    /// (process + crypto + mesh + connectivity).
    Health,
    /// `tower.mesh_status` — enriched mesh status for Tower validation.
    MeshStatus,
}

/// `acme.*` — ACME certificate management (Tower Atomic collaboration with bearDog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcmeMethod {
    /// `acme.challenge_ready` — register an HTTP-01 challenge token for serving.
    ChallengeReady,
    /// `acme.challenge_cleanup` — remove a completed challenge token.
    ChallengeCleanup,
}

/// `capabilities.*` / `capability.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilitiesMethod {
    /// `capability.list` — list callable methods and capability envelope (alias: `capabilities.list`)
    List,
    /// `capability.methods` — map of capability token → callable JSON-RPC methods
    Methods,
    /// `capabilities.discover` — cross-primal capability scan (biomeOS)
    Discover,
    /// `capabilities.announce` — announce capabilities to federation/mesh peers
    Announce,
    /// `capability.resolve` — single-step routing: returns the best provider endpoint
    /// for a given capability (the IPC equivalent of DNS resolution).
    Resolve,
    /// `capability.call` — cross-gate dispatch: invoke a capability operation, resolving
    /// locally or forwarding to a remote gate via mesh/relay transport.
    Call,
    /// `capability.health` — dispatch-path health probe: checks reachability of
    /// registered capability providers for cellMembrane monitoring.
    Health,
}

/// `lifecycle.*` — composition and runtime state introspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleMethod {
    /// `lifecycle.composition` — returns current composition state (which primals
    /// are up, which capabilities are live, health status) for real-time monitoring.
    Composition,
    /// `lifecycle.validate_consumed` — checks that all `consumed_capabilities`
    /// declared by a primal are satisfiable by currently registered providers.
    ValidateConsumed,
}

/// `identity.*` — Wire Standard Level 2 self-identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdentityMethod {
    /// `identity.get` — returns `{primal, version, domain, license, methods}`
    Get,
}

/// `ipc.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpcMethod {
    Register,
    Resolve,
    Discover,
    List,
    FindCapability,
    Heartbeat,
    Watch,
    RelayStats,
}

/// `http.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Request,
    Get,
    Post,
    Put,
    Delete,
    Proxy,
}

/// `route.*` — dynamic route configuration for drawbridge proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteMethod {
    /// Add or update a route mapping (capability → backend URL).
    Add,
    /// Remove a route by capability name.
    Remove,
    /// List all configured routes with details.
    List,
}

/// `stun.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StunMethod {
    Serve,
    Stop,
    Status,
    GetPublicAddress,
    Bind,
    ProbePortPattern,
    DetectNatType,
}

/// `igd.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IgdMethod {
    Discover,
    MapPort,
    UnmapPort,
    Status,
    ExternalIp,
    AutoConfigure,
}

/// `relay.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayMethod {
    Serve,
    Stop,
    Status,
    Allocate,
    /// Forward a JSON-RPC payload to a remote peer via mesh relay infrastructure.
    /// Accepts `{peer_id, capability, payload}` and routes through the mesh.
    Forward,
}

/// `discovery.*` and legacy aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscoveryMethod {
    Peers,
    Announce,
    ContentPeers,
    ListPeers,
    PeerCount,
    RejectedPeers,
    Status,
    Topology,
    Health,
    Query,
    Bonds,
    /// `discovery.by_family` — filter discovered peers by genetic family tags
    ByFamily,
    /// `discovery.by_capability` — find peers providing a given capability
    ByCapability,
    /// `discovery.encrypt` — encrypt discovery/birdsong payload (legacy: `encrypt_discovery`)
    Encrypt,
    /// `discovery.decrypt` — decrypt discovery/birdsong payload (legacy: `decrypt_discovery`)
    Decrypt,
}

/// `rendezvous.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendezvousMethod {
    Register,
    Lookup,
}

/// `peer.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerMethod {
    Connect,
    Ping,
}

/// `birdsong.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BirdsongMethod {
    GenerateEncryptedBeacon,
    DecryptBeacon,
    VerifyLineage,
    GetLineage,
    Advertise,
    Schema,
}

/// `mesh.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshMethod {
    Init,
    Status,
    FindPath,
    Announce,
    Peers,
    Topology,
    HealthCheck,
    AutoDiscover,
    DiscoverRemotes,
    Mirror,
    Publish,
    Subscribe,
    ProbeLatency,
    CapabilitiesAnnounce,
    /// `mesh.capabilities_revoke` — explicit capability withdrawal propagated mesh-wide.
    CapabilitiesRevoke,
    Enroll,
    /// `mesh.gate_enroll` — full autonomous gate enrollment: verifies physical proof,
    /// allocates mesh IP, registers WG peer, provisions Forgejo SSH key, delivers
    /// family seed. The zero-operator enrollment endpoint.
    GateEnroll,
    PruneStale,
    /// `mesh.connectivity_check` — active E2E validation across gate boundaries.
    /// Sends bidirectional JSON-RPC probes and reports per-peer reachability with
    /// riboCipher acceptance, cross-gate path status, and round-trip latency.
    ConnectivityCheck,
    /// `mesh.throughput` — sustained TCP streaming test measuring actual transfer
    /// capacity between this node and a target peer. Returns MB/s achieved.
    Throughput,
}

/// `gossip.*` — swarmVine-owned gossip methods relayed by songBird across gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GossipMethod {
    /// `gossip.relay` — relay a gossip payload to a target gate's swarmVine via
    /// songBird's `:7700` federation mesh. Accepts `{target_gate, topic, payload}`.
    /// When `target_gate` is `"local"` or absent, injects into the local swarmVine.
    Relay,
    /// `gossip.inject` — inject a gossip payload directly into local swarmVine.
    Inject,
    /// `gossip.spread` — broadcast gossip to ALL reachable mesh peers (epidemic fan-out).
    /// Unlike `gossip.relay` (targeted), this enables full cross-gate propagation when
    /// swarmVine's direct TCP 7800 path is unreachable between gates.
    Spread,
    /// `gossip.subscribe` — register interest in a gossip topic. When gossip arrives
    /// on the subscribed topic, songBird delivers it to the subscriber's endpoint.
    Subscribe,
    /// `gossip.query` — query gossip entries by topic and key prefix.
    /// Returns matching entries from the local swarmVine gossip table.
    /// Used by content.verify (cas.have), defense inversion (epitope.feed), etc.
    Query,
    /// `gossip.pool` — communal immunity pooling. Queries the mesh for all
    /// `defense` topic entries (epitope feeds from peer forges) and merges them
    /// into a unified immune surface. Each membrane contact point can call this
    /// to get the combined behavioral genetics across the entire mesh.
    Pool,
}

/// `punch.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PunchMethod {
    Request,
    Coordinate,
    Status,
}

/// `onion.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnionMethod {
    Start,
    Stop,
    Status,
    Connect,
    Address,
}

/// `federation.*` / `songbird.federation.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FederationMethod {
    Peers,
    Status,
    Join,
    Broadcast,
}

/// `tor.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TorMethod {
    Status,
    Connect,
    ServiceStart,
    ServiceStop,
    ConsensusFetch,
    CircuitBuild,
    CircuitClose,
}

/// `compute.route`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComputeMethod {
    Route,
}

/// `songbird.compute.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SongbirdComputeMethod {
    Schedule,
    Status,
}

/// `deployment.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeploymentMethod {
    Create,
    Status,
    /// Hot-swap a running service binary — stop old, deploy new, start, verify.
    HotSwap,
    /// Restart an existing deployment without changing the binary.
    Restart,
    /// List all active deployments.
    List,
}

/// `task.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskMethod {
    Create,
    List,
}

/// `consent.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsentMethod {
    Check,
    Grant,
}

/// `registry.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistryMethod {
    Register,
    Discover,
}

/// `btsp.negotiate` and `btsp.capabilities`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BtspMethod {
    Negotiate,
    Capabilities,
}

/// `protocol.negotiate` and `songbird.protocol.capabilities`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolMethod {
    Negotiate,
    Capabilities,
}

/// `songbird.services.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SongbirdServicesMethod {
    List,
    Get,
    Register,
}

/// `songbird.health` and `songbird.version`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SongbirdMethod {
    Health,
    Version,
}

/// `network.*`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkMethod {
    BeaconExchange,
    Broadcast,
    Listen,
}

/// `storage.*` — storage capability domain (delegated persistence).
///
/// Songbird delegates all persistence via `storage.*` JSON-RPC IPC to the storage
/// capability provider. `IpcStorageBackend` is the production path; `InMemoryStorage`
/// is the fallback when no provider is available (SB-03 resolved, sled eliminated Wave 135).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageMethod {
    Get,
    Put,
    Delete,
    List,
    Flush,
}

/// `inference.*` — canonical inference namespace (absorbs `model.*` and `ai.*` aliases).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferenceMethod {
    /// `inference.infer`
    Infer,
    /// `inference.status`
    Status,
    /// `inference.list`
    List,
    /// `inference.load`
    Load,
}

/// `tunnel.*` — genetic and transport tunnel operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TunnelMethod {
    /// `tunnel.create_genetic` — establish BTSP tunnel with genetic lineage proof
    CreateGenetic,
}

/// `graph.*` — dependency graph operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphMethod {
    Validate,
    CheckAvailability,
    SuggestAlternatives,
}

/// `coordination.*` — pattern validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoordinationMethod {
    ValidatePattern,
}

/// `content.*` — content-addressable storage (CAS) location, verification, and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentMethod {
    /// `content.locate` — find endpoint(s) where content by hash can be fetched.
    Locate,
    /// `content.verify` — verify content hash at a known endpoint.
    Verify,
    /// `content.availability` — check whether content is available locally or on mesh peers.
    Availability,
    /// `content.put` — store content via a local CAS provider (routes to `content_storage` capability).
    Put,
}
