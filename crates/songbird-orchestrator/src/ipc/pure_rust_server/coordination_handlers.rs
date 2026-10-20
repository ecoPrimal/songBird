// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Coordination client IPC handlers
//!
//! NEW (Jan 20, 2026): Upstream integration from biomeOS.
//!
//! JSON-RPC handlers for **coordination / AI** clients that introspect Songbird over IPC:
//! - `discover_capabilities`: Advertise Songbird's capabilities
//! - `http.request`: Delegate HTTP/HTTPS requests (Pure Rust via Tower Atomic) — see also [`handle_http_request`]
//! - `health`: Simple health check endpoint
//!
//! ## Tower Atomic Architecture
//!
//! ```text
//! Coordination client → Songbird (TLS/HTTP) → security provider (Crypto) → External HTTPS
//! ```
//!
//! Songbird handles protocol logic (TLS/HTTP), `security provider` handles cryptographic operations,
//! resulting in a 100% Pure Rust networking stack with no C dependencies.

use anyhow::Result;
use tracing::info;

use super::protocol::{JsonRpcError, JsonRpcErrorExt};
use songbird_http_client::SongbirdHttpClient;

/// Handle `discover_capabilities` — return Songbird's advertised capabilities
///
/// Lets coordination clients discover that Songbird provides HTTP delegation and related APIs.
///
/// ## Response Format
/// ```json
/// {
///   "capabilities": ["http.post", "http.get", "http.request", ...],
///   "metadata": {
///     "primal_name": "songbird",
///     "version": "4.9.0",
///     "family_id": "default"
///   }
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn handle_discover_capabilities() -> Result<serde_json::Value, JsonRpcError> {
    info!("🔍 Capability discovery request received");

    // Get family ID from canonical env_config (proper env chain, default: "default")
    let family_id = crate::env_config::family_id();

    // Songbird's capabilities for inter-primal communication
    let capabilities = vec![
        "http.post",          // POST requests
        "http.get",           // GET requests
        "http.request",       // Generic HTTP requests
        "discovery.announce", // Service announcement
        "discovery.query",    // Service discovery
        "security.verify",    // JWT verification (via security provider delegation)
    ];

    Ok(serde_json::json!({
        "capabilities": capabilities,
        "metadata": {
            "primal_name": crate::env_config::primal_name(),
            "version": env!("CARGO_PKG_VERSION"),
            "family_id": family_id
        }
    }))
}

/// Handle http.request - Delegate HTTP requests to external services
///
/// EVOLVED (Jan 21, 2026): Pure Rust HTTP via Tower Atomic (`security provider` crypto delegation)
///
/// ## Request Format
/// ```json
/// {
///   "method": "POST",
///   "url": "https://api.anthropic.com/v1/messages",
///   "headers": { "content-type": "application/json", ... },
///   "body": { ... }
/// }
/// ```
///
/// ## Response Format
/// ```json
/// {
///   "status": 200,
///   "headers": { "content-type": "application/json" },
///   "body": { ... }
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn handle_http_request(
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct HttpRequestParams {
        method: String,
        url: String,
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
        #[serde(default)]
        body: Option<serde_json::Value>,
    }

    let params: HttpRequestParams = match params {
        Some(p) => {
            serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
        }
        None => return Err(JsonRpcError::invalid_params("Missing params")),
    };

    info!("🌐 HTTP delegation (Pure Rust): {} {}", params.method, params.url);

    // ✅ NEW v2.0.0: Use Neural API for capability translation
    // SongbirdHttpClient routes crypto calls through Neural API, which translates
    // semantic capabilities to actual provider methods. This enables TRUE PRIMAL pattern.
    let client = SongbirdHttpClient::from_env(); // Uses NEURAL_API_SOCKET env var

    // Make request via Pure Rust client (NO reqwest, NO ring, NO C!)
    let response =
        client
            .request(&params.method, &params.url, params.headers, params.body)
            .await
            .map_err(|e| JsonRpcError::internal_error(format!("HTTP request failed: {e}")))?;

    info!("✅ HTTP delegation complete (Pure Rust): {} (status: {})", params.url, response.status);

    Ok(serde_json::json!({
        "status": response.status,
        "headers": response.headers,
        "body": response.body
    }))
}

/// Handle health - Simple health check
///
/// Used by coordination clients for availability checks (e.g. `is_available()`).
///
/// ## Response Format
/// ```json
/// {
///   "status": "healthy",
///   "primal": "songbird",
///   "version": "4.9.0"
/// }
/// ```
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn handle_health() -> Result<serde_json::Value, JsonRpcError> {
    Ok(serde_json::json!({
        "status": "healthy",
        "primal": crate::env_config::primal_name(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, reason = "unit tests")]

    #[tokio::test]
    async fn handle_discover_capabilities_returns_expected_shape() {
        let v = super::handle_discover_capabilities().await.expect("ok");
        let caps: Vec<String> = serde_json::from_value(v["capabilities"].clone()).expect("caps");
        assert!(caps.contains(&String::from("http.get")));
        assert!(caps.contains(&String::from("http.post")));
        assert!(caps.contains(&String::from("http.request")));
        assert_eq!(v["metadata"]["primal_name"], "songbird");
        assert_eq!(v["metadata"]["version"].as_str().expect("version"), env!("CARGO_PKG_VERSION"));
        let fid = v["metadata"]["family_id"].as_str().expect("family_id");
        assert!(!fid.is_empty());
    }

    #[tokio::test]
    async fn handle_discover_capabilities_family_id_respects_env_override() {
        let _g = songbird_process_env::test_env_lock();
        for key in [
            "SONGBIRD_ORCHESTRATOR_FAMILY_ID",
            "SONGBIRD_ORCHESTRATOR_FAMILY",
            "BIOMEOS_FAMILY_ID",
            "SONGBIRD_FAMILY_ID",
            "FAMILY_ID",
        ] {
            songbird_process_env::remove_var(key);
        }
        songbird_process_env::set_var("SONGBIRD_ORCHESTRATOR_FAMILY_ID", "coord-test-family");
        let v = super::handle_discover_capabilities().await.expect("ok");
        assert_eq!(v["metadata"]["family_id"], "coord-test-family");
        songbird_process_env::remove_var("SONGBIRD_ORCHESTRATOR_FAMILY_ID");
    }
}
