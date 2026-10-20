// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

//! Graph Intelligence API Handlers (v3.21.0, refactored v3.22.1)
//!
//! Handlers for Collaborative Intelligence graph validation and optimization.
//!
//! v3.21.0: Initial implementation for graph intelligence
//! v3.22.1: Extracted from monolithic handlers.rs (Jan 12, 2026)

use super::IpcHandlers;
use crate::graph::{
    AlternativeSuggestions, AvailabilityReport, CoordinationValidationResult, ValidationResult,
};
use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
use tracing::{debug, info};

// ============================================================================
// jsonrpsee Handlers (for jsonrpsee server)
// ============================================================================

/// Handle `graph.validate` RPC call (Pure Rust, v3.34.0)
///
/// v3.21.0: Validates graph structure and relationships
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn validate_graph(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<ValidationResult, JsonRpcError> {
    debug!("🔍 Graph Intelligence API: graph.validate");

    let graph: crate::graph::Graph = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse graph: {e}"), None))?;

    let result = handlers.graph_validator.validate(&graph);

    info!("✅ Graph validation complete: valid={}, {} issues", result.valid, result.issues.len());

    Ok(result)
}

/// Handle `graph.check_availability` RPC call (Pure Rust, v3.34.0)
///
/// v3.21.0: Checks if required primals are available
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn check_availability(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<AvailabilityReport, JsonRpcError> {
    debug!("🔍 Graph Intelligence API: graph.check_availability");

    let graph: crate::graph::Graph = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse graph: {e}"), None))?;

    let report = handlers.availability_checker.check_availability(&graph).await.map_err(|e| {
        JsonRpcError::custom(-32603, format!("Availability check failed: {e}"), None)
    })?;

    info!(
        "✅ Availability check complete: {} available, {} unavailable",
        report.available.len(),
        report.unavailable.len()
    );

    Ok(report)
}

/// Handle `graph.suggest_alternatives` RPC call (Pure Rust, v3.34.0)
///
/// v3.21.0: Suggests alternative primals for unavailable nodes
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn suggest_alternatives(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<AlternativeSuggestions, JsonRpcError> {
    debug!("🔍 Graph Intelligence API: graph.suggest_alternatives");

    let node: crate::graph::GraphNode = serde_json::from_value(params).map_err(|e| {
        JsonRpcError::custom(-32602, format!("Failed to parse graph node: {e}"), None)
    })?;

    let suggestions =
        handlers.availability_checker.suggest_alternatives(&node).await.map_err(|e| {
            JsonRpcError::custom(-32603, format!("Failed to suggest alternatives: {e}"), None)
        })?;

    info!(
        "✅ Found {} alternative suggestions for node '{}'",
        suggestions.alternatives.len(),
        node.id
    );

    Ok(suggestions)
}

/// Handle `coordination.validate_pattern` RPC call (Pure Rust, v3.34.0)
///
/// v3.21.0 Week 3: Validates coordination patterns
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn validate_coordination_pattern(
    handlers: &IpcHandlers,
    params: serde_json::Value,
) -> Result<CoordinationValidationResult, JsonRpcError> {
    debug!("🔍 Graph Intelligence API: coordination.validate_pattern");

    let graph: crate::graph::Graph = serde_json::from_value(params)
        .map_err(|e| JsonRpcError::custom(-32602, format!("Failed to parse graph: {e}"), None))?;

    let result = handlers.coordination_validator.validate_pattern(&graph).await.map_err(|e| {
        JsonRpcError::custom(-32603, format!("Coordination validation failed: {e}"), None)
    })?;

    info!(
        "✅ Coordination validation complete: valid={}, {} issues",
        result.valid,
        result.issues.len()
    );

    Ok(result)
}

// ============================================================================
// Pure JSON Adapters (for pure Rust Unix socket server v3.22.0)
// ============================================================================

/// Graph Intelligence: `validate_graph` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
#[expect(
    clippy::unused_async,
    reason = "async signature required by Axum, trait objects, or future I/O"
)]
pub async fn validate_graph_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let graph: crate::graph::Graph = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for graph.validate"));
    };

    let result = handlers.graph_validator.validate(&graph);
    serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// Graph Intelligence: `check_availability` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn check_availability_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let graph: crate::graph::Graph = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for graph.check_availability"));
    };

    let report = handlers
        .availability_checker
        .check_availability(&graph)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    serde_json::to_value(report).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// Graph Intelligence: `suggest_alternatives` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn suggest_alternatives_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let node: crate::graph::GraphNode = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params("Missing params for graph.suggest_alternatives"));
    };

    let suggestions = handlers
        .availability_checker
        .suggest_alternatives(&node)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;

    serde_json::to_value(suggestions).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

/// Graph Intelligence: `validate_coordination_pattern` (pure JSON adapter)
/// # Errors
///
/// Returns an error if the operation fails.
pub async fn validate_coordination_pattern_json(
    handlers: &IpcHandlers,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let graph: crate::graph::Graph = if let Some(p) = params {
        serde_json::from_value(p).map_err(|e| JsonRpcError::invalid_params(e.to_string()))?
    } else {
        return Err(JsonRpcError::invalid_params(
            "Missing params for coordination.validate_pattern",
        ));
    };

    let result = handlers
        .coordination_validator
        .validate_pattern(&graph)
        .await
        .map_err(|e| JsonRpcError::internal_error(e.to_string()))?;
    serde_json::to_value(result).map_err(|e| JsonRpcError::internal_error(e.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test assertions")]

    use std::sync::Arc;

    use super::*;
    use crate::app::connection_manager::ConnectionManager;
    use crate::graph::{Graph, GraphMetadata, GraphNode};
    use crate::ipc::handlers::IpcHandlers;
    use crate::ipc::pure_rust_server::{JsonRpcError, JsonRpcErrorExt};
    use crate::ipc::registry::ServiceRegistry;
    use songbird_http_client::SecurityRpcClient;

    fn test_handlers() -> IpcHandlers {
        let registry = Arc::new(ServiceRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new());
        let security_client = Arc::new(SecurityRpcClient::new_direct(
            "/tmp/songbird-orchestrator-graph-intelligence-tests.sock",
        ));
        IpcHandlers::new(registry, None, connection_manager, security_client)
    }

    fn minimal_graph_value() -> serde_json::Value {
        let graph = Graph::new(
            String::from("g-test"),
            String::from("Test Graph"),
            vec![],
            vec![],
            GraphMetadata::default(),
        );
        serde_json::to_value(graph).unwrap()
    }

    fn sample_node_json() -> serde_json::Value {
        let node = GraphNode {
            id: String::from("n1"),
            primal_name: None,
            capability: String::from("encryption"),
            inputs: vec![],
            outputs: vec![],
            config: serde_json::json!({}),
            preferred_protocol: None,
            timeout_secs: None,
        };
        serde_json::to_value(node).unwrap()
    }

    #[tokio::test]
    async fn validate_graph_rejects_non_object_params() {
        let handlers = test_handlers();
        let err = validate_graph(&handlers, serde_json::json!("not-a-graph")).await.unwrap_err();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("Failed to parse graph"));
    }

    #[tokio::test]
    async fn validate_graph_json_missing_params() {
        let handlers = test_handlers();
        let err = validate_graph_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
        assert!(err.message.contains("Missing params"));
    }

    #[tokio::test]
    async fn validate_graph_json_invalid_graph_shape() {
        let handlers = test_handlers();
        let err = validate_graph_json(&handlers, Some(serde_json::json!({}))).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn validate_graph_json_success_roundtrip() {
        let handlers = test_handlers();
        let out = validate_graph_json(&handlers, Some(minimal_graph_value())).await.unwrap();
        assert_eq!(out["valid"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn check_availability_json_missing_params() {
        let handlers = test_handlers();
        let err = check_availability_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn check_availability_json_empty_graph() {
        let handlers = test_handlers();
        let out = check_availability_json(&handlers, Some(minimal_graph_value())).await.unwrap();
        assert!(out.get("summary").is_some());
    }

    #[tokio::test]
    async fn suggest_alternatives_json_missing_params() {
        let handlers = test_handlers();
        let err = suggest_alternatives_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn suggest_alternatives_json_rejects_invalid_node() {
        let handlers = test_handlers();
        let err =
            suggest_alternatives_json(&handlers, Some(serde_json::json!({}))).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn suggest_alternatives_json_success() {
        let handlers = test_handlers();
        let out = suggest_alternatives_json(&handlers, Some(sample_node_json())).await.unwrap();
        assert!(out.get("alternatives").is_some());
    }

    #[tokio::test]
    async fn validate_coordination_pattern_json_missing_params() {
        let handlers = test_handlers();
        let err = validate_coordination_pattern_json(&handlers, None).await.unwrap_err();
        assert_eq!(err.code, JsonRpcError::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn validate_coordination_pattern_json_empty_graph() {
        let handlers = test_handlers();
        let out = validate_coordination_pattern_json(&handlers, Some(minimal_graph_value()))
            .await
            .unwrap();
        assert!(out.get("valid").is_some());
        assert!(out.get("pattern").is_some());
    }
}
