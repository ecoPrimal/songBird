# songBird — Evolution Record

The changelog is sorted by capability surface, not by date.
Each section tells the arc: what emerged, what it replaced, what it became.
The date-sorted session history lives in `CHANGELOG_ARCHIVE.md`.

`H(data|epitope) < H(data|date) < H(data)`

---

## Gossip & Capability Propagation — cross-gate state exchange

Gossip started as implicit mesh state; it became an explicit protocol surface for propagating capabilities, reachability, and version metadata across gates.

- **Capability announce** (Wave 75→113): Began as outbound-only `mesh.capabilities_announce` broadcasts; gained retry queue with age-based eviction (cap 50), unknown-peer rejection, and read-lock scope fixes so gossip never blocks the mesh.
- **Cross-gate reachability gossip** (Wave 113): `reachable_peers` in announce payloads evolved into `PartitionStatus` computation — the mesh now detects asymmetric partitions from gossip, not just local probes.
- **Version gossip** (Wave 113): Version field in announces and probes replaced ad-hoc version skew detection; `mesh.status` now surfaces `version_skew[]` and `partition_warnings[]`.
- **Neural API announce** (Wave 43→45): Outbound `primal.announce` on startup replaced separate lifecycle/capability/method registration calls; capabilities aligned to routing domains with actual bound socket paths.
- **Discovery bridge gossip** (Wave 49→51): `bootstrap_peers` and `discovery.peers` bridged into mesh seeding; `SONGBIRD_PEERS` env auto-seeds federation on startup with zero-config mesh bootstrap.

---

## Mesh Federation & Topology — multi-gate coordination

The mesh began as a federation concept; it matured into a full topology API with pathfinding, health loops, and auto-reconnect.

- **Mesh RPC surface** (Wave 204→113): `mesh.init`, `mesh.status`, `mesh.peers`, `mesh.topology`, `mesh.find_path`, `mesh.health_check`, `mesh.auto_discover` moved from TCP-only to canonical UDS (`songbird.sock`); became the tower atomic validation gate.
- **Three-gate mesh** (Wave 96→100): LAN HTTP fallback (`HttpRemoteConnection`), bare-name UDS resolution, and security-socket routing fixed the blockers that prevented sporeGate/blueGate/flockGate from forming a live mesh.
- **Auto-reconnect** (Wave 111): Background health loop with 30s→300s exponential backoff replaced fire-and-forget bootstrap; peers re-record latency via `record_direct_connection()` on recovery.
- **Federation status truth** (Wave 110→111): `enabled` reporting evolved from `total_nodes > 0` (connectivity conflated with configuration) to env-aware detection via `SONGBIRD_FEDERATION_ENABLED` / `SONGBIRD_PEERS`.
- **Multi-instance fractal scaling** (Wave 3.7.2→3.7.3): Global PID/single-socket collision replaced with `songbird-{family}-{node}` scoping; unlimited spores per machine via dynamic XDG socket paths.
- **Content mirroring** (Wave 60): `mesh.discover_remotes` and `mesh.mirror` emerged for cross-gate repo/content sync used by `ecosystem.push`.

---

## Peer Discovery — finding peers and capabilities

Discovery evolved from hardcoded endpoints to a multi-method, capability-first, latency-aware registry.

- **Multi-method discovery** (v0.2.1): Five-method fallback chain (env, DNS-SD, mDNS, registry, config) replaced hardcoded primal URLs; became the sovereignty foundation (98.7% hardcoding elimination).
- **Capability-first socket discovery** (Wave 97→102): `discover_beardog*` became `discover_security_provider_socket()` with priority chain `$SECURITY_PROVIDER_SOCKET` > XDG `security.sock` > deprecated `$BEARDOG_SOCKET`; 53 files migrated across 13 crates.
- **Self-healing auto-discovery** (Wave 138→139): Socket auto-discovery seeds `ipc.resolve` registry at startup; stale socket cleanup (primalSpring R10) prevents ghost endpoints.
- **TransportEndpoint JSON** (Wave 98): `ipc.resolve` and `capability.resolve` evolved from string URLs to structured `TransportEndpoint` (UDS/TCP/MeshRelay) — Phase 2 M1 gate for sourDough transport injection.
- **Latency-aware peers** (Wave 69→70): Active RTT probing wired `latency_ms` into `discovery.peers`; connection pooling and circuit breakers added for probe efficiency.
- **Peer discovery API** (v3.8.0→3.9.0): Programmatic mesh state API (`discovery.status`, topology learning) replaced black-box federation; enabled tower CLI commands for peer ping/list/status.
- **Observability** (Wave 3.9.0): Discovery metrics and registry wiring fixed so agents can monitor topology without human intervention.

---

## Content Distribution — federated seeding and leeching

Content distribution started as topic announcements; it became a TTL-governed federation registry with manifest-addressed lookup.

- **Topic announce** (Wave 140→143): `discovery.announce` gained topic mode (`topic`, `manifest_hash`, `seeder_count`, `bond_types_accepted`); presence announcements no longer pollute the content store.
- **Content peer query** (Wave 143): `discovery.content_peers` emerged with `topic`, `family_only`, and BLAKE3 `manifest_hash` filters tied to NestGate `ContentManifest`.
- **Content store** (Wave 143): In-memory `ContentAnnouncementStore` with 10min TTL, de-duplication on re-announce, and GC replaced ad-hoc peer lists.
- **Capability filtering** (Wave 140): `discovery.peers` gained `capability_filter` (string + array) and `family_only` per `content_distribution_federation.toml`.
- **Cross-gate mirror** (Wave 60): Remote content source discovery via `mesh.discover_remotes` bridges gossip topology to content placement decisions.

---

## Transport & Session Protocol — BTSP, JSON-RPC, riboCipher

The wire stack evolved from plain JSON-RPC to multi-protocol accept loops with encrypted BTSP sessions and tiered riboCipher signals.

- **Protocol-agnostic adapters** (v3.11.0): Unix-socket JSON-RPC became PRIMARY; HTTP/HTTPS fallback for cross-machine; all four domain adapters (Security, Storage, Compute, AI) auto-detect from endpoint prefix.
- **BTSP handshake phases** (Wave 132→186): Phase 2 server handshake on UDS accept → Phase 3 `btsp.negotiate` encrypted framing → live connection verification; NDJSON auto-detect in accept loop replaced protocol guessing.
- **Session protocol extraction** (Wave 113): `session_protocol.rs` split from monolithic `connection.rs` (815→472L); BTSP framing, NDJSON loop, encrypted Phase 3 isolated.
- **riboCipher transport signals** (Wave 113): All three accept loops detect `0xEC`/`0xED`/`0xEE` before protocol fork — clear (commensal), mito-obfuscated (family), nuclear-sealed (sovereign) tiers mapped to Anderson membrane model.
- **TLS/HTTP cohabitation** (Wave 3.21.0→79): Same-port peek (`0x16` = TLS, ASCII = HTTP) replaced separate TLS/HTTP listeners; LAN mesh evolved from forced `https://` to context-aware `http://` on `:7700`.
- **Persistent NDJSON sessions** (Wave 148): UDS connections hold session state across requests instead of one-shot request/response per connection.
- **Direct-mode crypto routing** (Wave 79): BTSP tunnel crypto calls route through security provider instead of inline TLS; fixes SB-TLS-01 blocker for mesh peers without capability tokens.

---

## NAT Traversal & Hole Punching — getting through NAT

NAT traversal began as STUN stubs; it became a coordinated punch pipeline with port-pattern probing and relay assistance.

- **STUN client** (Wave 76→213): Pure Rust STUN binding transactions with mito-beacon credentials (`StunCredentials`) replaced placeholder env-var checks; port pattern probing (`stun.probe_port_pattern`) characterizes Sequential/Random/PortPreserving/Symmetric NAT.
- **Coordinated punch** (Wave v0.2.1→213): `punch.coordinate` via relay server replaced simulated sleep loops; `HolePunchCoordinator` wired to real `punch_to_peer()` at service init.
- **UPnP/NAT-PMP** (Wave v3.41.0): `songbird-igd` crate (SSDP + SOAP + RFC 6886) auto-configures router port forwarding on `SONGBIRD_IGD_ENABLED=true`.
- **Shadow comparator** (Wave 213): NAT traversal data plane with shadow-mode path comparison validates relay vs direct before promoting to production routing.
- **Cross-gate TURN dispatch** (Wave 38): TURN relay dispatch across gates with NAT field test replaced single-gate punch assumptions.

---

## Relay — TURN, virtual endpoints, cross-subnet, onion

Relay capabilities layered from simple forwarding to signed, virtual, and cross-subnet routing.

- **Virtual endpoint relay** (Wave 70→74): Phase 1 shadow mode → Phase 2 active routing; virtual endpoints decouple logical peer identity from physical reachability.
- **Cross-subnet relay** (Wave 74): Relay routing across subnet boundaries replaced LAN-only mesh assumptions; env var alignment unified relay configuration.
- **Ed25519 relay signatures** (Wave 78): Phase 3.5 relay message signature verification replaced trust-on-first-use relay forwarding.
- **BTSP relay verification** (Wave 158→167): Step 3→4 verification relay fixed silent-fail hangs (`read_to_end`, premature `stream.shutdown()`) that broke BearDog BTSP roundtrips.
- **Onion relay coordinator** (Wave 142): Register/query/heartbeat/punch_request message handling in `songbird-onion-relay` replaced stub coordinator.
- **VPS relay server** (Wave 199→201): Pass 12 sentinel VPS relay + `capability.resolve` parity enabled WAN gate federation through golgiBody relay.
- **TURN client crate** (Wave 205): `songbird-turn-client` extracted as standalone crate with `primal.announce` adoption for atomic registration.

---

## Security, Trust & Lineage — who may connect

Security evolved from BearDog-coupled hardcoding to capability-based verification with mesh trust exchange and signed IPC.

- **Mesh trust exchange** (Wave 80): P0 BD-TRUST-01 — cross-gate trust evaluation via security provider replaced implicit LAN trust; gates verify lineage before accepting mesh connections.
- **Security provider naming** (Wave 97→112): `BearDogVerifier` → `SecurityProviderVerifier`; all `beardog`/`toadstool`/`squirrel` identifiers scrubbed to capability-domain names across 50+ deprecated identifiers in 12 crates.
- **JWT hardening** (Wave 102): Dev secret fallback removed; `SONGBIRD_JWT_SECRET` required — unset secret rejects all tokens instead of silently using `"songbird-dev-secret"`.
- **Signed IPC registration** (Wave 177): `ipc.register` identity verification replaced unsigned registration; whitespace-tolerant protocol detection prevents framing confusion.
- **Lineage relay** (Wave 145→146): `Box<dyn LineageProvider>` → static enum dispatch; `BirdSongCrypto` and `RelayAuthority` evolved from async-trait dyn to native AFIT enums.
- **CryptoCapability abstraction** (Wave 5.21.0→5.22.0): Provider-swappable crypto layer replaced direct BearDog coupling; full TLS migration to CryptoCapability completed HTTPS stack.
- **Frame size guard** (Wave 192): `MAX_ONION_FRAME` (16 MiB) validation before buffer allocation prevents memory-bomb via untrusted frame headers.

---

## BirdSong & Communal Immunity — encrypted mesh beacons

BirdSong started as encrypted peer beacons; it became the communal immunity layer with mito-beacon tiers and federation-wide key exchange.

- **BirdSong encryption** (Wave 86→130): Ring-dependent crypto replaced with security-provider delegation; `birdsong_handler` modularized (855L → 7 modules) with beacon encrypt/decrypt, lineage, schema introspection.
- **Mito-beacon tier** (Wave 140): `beacon.encrypt`/`beacon.decrypt`/`beacon.get_id` RPC methods with graceful fallback to legacy `birdsong.*` — Anderson Tier 2 membrane signal.
- **Family-scoped encryption** (Wave 3.21.0): `family_id` parameter in `encrypt_for_lineage()`/`decrypt_birdsong()` replaced hardcoded `"nat0"` defaults; dynamic `env_config::family_id()` with `"default"` fallback.
- **BirdSong federation security** (Wave 78→143): Relay access levels, session lifecycle, broadcast keys in `songbird-network-federation` replaced stub passthrough crypto.
- **Schema introspection** (Wave 82): `birdsong.schema` method for agent-driven protocol discovery without hardcoded method lists.

---

## Health & Observability — knowing mesh state

Health evolved from always-healthy stubs to real probes conforming to the ecosystem 13/13 contract.

- **HEALTH-01 contract** (Wave 110): Bare `{"method":"health"}` returns `{status, primal, version, uptime_s}`; `BiomeOsHealth` wired in IPC dispatch; `uptime_seconds` → `uptime_s` standardized.
- **Real health probes** (Wave 112): `HealthProbe::NoOp` replaced with TcpConnect, UnixSocket, FilesystemAccess, Custom, JsonRpcHealth variants with timing measurement.
- **Mesh health loop** (Wave 96→113): `spawn_peer_health_loop` uses `probe_peer_full()` returning RTT + version; partition flags surface in `mesh.health_check` when cross-gate gossip reveals asymmetry.
- **Federation health** (Wave 102): `TarpcServer::health_check` and `IntegrationManager::check_core_services` evolved from always-healthy stubs to real socket/TCP probes.
- **Resilience modules** (Wave 8.25.0): Centralized TimeoutConfig, ConnectionPool, CircuitBreaker, HealthCheck modules replaced scattered timeout literals and ad-hoc retry logic.

---

## Resilience & Connection Management — surviving failures

Resilience grew from retry comments to structured backoff, pooling, and event-driven coordination.

- **Retry hardening** (Wave 76→77): Exponential backoff with jitter, flaky test elimination via env-mutation mutex, and event-driven wakeup replaced polling loops.
- **Connection pooling** (Wave 70→8.25.0): Active latency probing + bounded pool with health checking; 30-50% projected latency reduction on hot paths.
- **Circuit breaker** (Wave 8.25.0): Closed→Open→Half-Open state machine prevents cascading failures; domain-based breaker sharing via CircuitBreakerManager.
- **Event-driven architecture** (Wave v3.42.0): ConsentManager, UnixSocketServer, BirdSongBroadcaster migrated from 10-100ms polling to `tokio::sync::Notify` instant wakeup.
- **Graceful degradation** (Wave 96→99): Security provider failure no longer crashes orchestrator; degrades to unsigned ops, plain HTTP federation, secure-random JWT when unconfigured.

---

## Architecture & Engineering Discipline — how the codebase evolved

The codebase transformed from audit-discovered debt to reference implementation through systematic capability-first refactoring.

- **Sovereignty foundation** (v0.1.0→0.2.0): Universal Capability Adapter with 15 crates, multi-method discovery, zero hardcoded primal dependencies; Grade A- (91/100) audit baseline.
- **Reference implementation** (v0.3.0 Dec 2025): 98.7% hardcoding elimination, 95% Result-based errors, TOP 0.1% unsafe (0.06%); federation and discovery extracted to dedicated modules.
- **async-trait elimination** (Wave 144→145): dyn dispatch → static enum dispatch across federation, discovery, lineage-relay; 150 async-trait instances audited, dyn-required cases tracked as SB-06.
- **Mock isolation** (Wave 147→154): Production mock types gated behind `#[cfg(any(test, feature = "test-mocks"))]`; zero mock types in release builds.
- **Ring/quinn elimination** (Wave 89→93): Pure Rust QUIC engine, rcgen replaced with ed25519-dalek test cert generation; `cargo tree | grep ring` returns empty in default build.
- **NestGate storage migration** (Wave 120): sled → NestGate via JSON-RPC `storage.*`; `sled-storage` feature deprecated; consent/task lifecycle uses NestGate capability discovery.
- **Edition 2024 + UniBin** (Wave v0.3.1): Full workspace Edition 2024 migration; unified binary packaging; pedantic clippy zero-warning across 30 crates.
- **Test modernization** (Wave 118→171): Sleep-based tests → `start_paused` + `advance()`; port 0 ephemeral binds; injectable env readers eliminated 120+ `std::env::set_var` calls in tests; coverage 71%→73%+.

---

## Alternative Transports — QUIC, Tor, NFC

Optional transport layers emerged for specialized routing without compromising the default pure-Rust stack.

- **Pure Rust QUIC** (Wave 89): quinn/ring eliminated; custom QUIC engine with pure Rust cert generation; zero C dependencies in default build.
- **Pure Rust Tor Phase 2A** (Wave v3.34.0→v3.41.0): Directory protocol (consensus fetch/parse, relay selection, 3-hop paths) without Tor daemon; Phase 2B circuit building blocked on security provider AES/SHA3 extensions.
- **NFC genesis integration** (Wave v3.41.0): BearDogNfcCrypto client with 3-tier socket discovery replaced 9 crypto stubs in genesis NFC path.
- **Protocol detection on UDS** (Wave 140): First-byte peek via `BufReader::fill_buf()` + `PeekedStream` auto-detects BTSP vs plain JSON-RPC without native `peek()`.

---

*Full date-sorted history: `CHANGELOG_ARCHIVE.md`*
*Last compressed: Wave 172*
