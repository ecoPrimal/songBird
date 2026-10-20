// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024-2026 ecoPrimals

use axum::http::StatusCode;
use serde_json::Value;

use super::super::types::{JsonRpcError, JsonRpcErrorExt};

pub(super) fn extract_str_param(params: Option<&Value>, key: &str) -> Result<String, JsonRpcError> {
    params
        .and_then(|p| p.as_object())
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| JsonRpcError::invalid_params(format!("Missing '{key}'")))
}

pub(super) fn jsonrpc_from_compute_error(e: crate::server::compute_api::ApiError) -> JsonRpcError {
    use crate::server::compute_api::ApiError;
    match e {
        ApiError::Routing(msg) | ApiError::Execution(msg) => JsonRpcError::internal_error(msg),
        ApiError::InvalidRequest(msg) => JsonRpcError::invalid_params(msg),
        ApiError::NotFound(msg) => JsonRpcError {
            code: -32001,
            message: msg,
            data: None,
        },
    }
}

pub(super) const fn jsonrpc_code_from_http_status(status: StatusCode) -> i32 {
    match status.as_u16() {
        404 => -32001,
        400..=499 => JsonRpcError::INVALID_PARAMS,
        _ => JsonRpcError::INTERNAL_ERROR,
    }
}

#[cfg(test)]
mod tests {
    // SPDX-License-Identifier: AGPL-3.0-or-later

    #![allow(clippy::unwrap_used, reason = "test assertions")]

    use axum::http::StatusCode;

    use super::{jsonrpc_code_from_http_status, jsonrpc_from_compute_error};
    use crate::server::jsonrpc_api::types::JsonRpcError;

    #[test]
    fn jsonrpc_from_compute_error_maps_variants() {
        use crate::server::compute_api::ApiError;
        let e = jsonrpc_from_compute_error(ApiError::InvalidRequest("bad".into()));
        assert_eq!(e.code, JsonRpcError::INVALID_PARAMS);
        assert!(e.message.contains("bad"));

        let e = jsonrpc_from_compute_error(ApiError::Routing("r".into()));
        assert_eq!(e.code, JsonRpcError::INTERNAL_ERROR);

        let e = jsonrpc_from_compute_error(ApiError::NotFound("n".into()));
        assert_eq!(e.code, -32001);
    }

    #[test]
    fn jsonrpc_code_from_http_status_maps_buckets() {
        assert_eq!(jsonrpc_code_from_http_status(StatusCode::NOT_FOUND), -32001);
        assert_eq!(
            jsonrpc_code_from_http_status(StatusCode::BAD_REQUEST),
            JsonRpcError::INVALID_PARAMS
        );
        assert_eq!(
            jsonrpc_code_from_http_status(StatusCode::INTERNAL_SERVER_ERROR),
            JsonRpcError::INTERNAL_ERROR
        );
    }
}
