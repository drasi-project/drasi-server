// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use axum::http::StatusCode;
use drasi_server_core::error::DrasiError;
use serde::Serialize;
use utoipa::ToSchema;

/// Error codes for API responses
pub mod error_codes {
    pub const SOURCE_CREATE_FAILED: &str = "SOURCE_CREATE_FAILED";
    pub const SOURCE_NOT_FOUND: &str = "SOURCE_NOT_FOUND";
    pub const SOURCE_START_FAILED: &str = "SOURCE_START_FAILED";
    pub const SOURCE_STOP_FAILED: &str = "SOURCE_STOP_FAILED";
    pub const SOURCE_DELETE_FAILED: &str = "SOURCE_DELETE_FAILED";

    pub const QUERY_CREATE_FAILED: &str = "QUERY_CREATE_FAILED";
    pub const QUERY_NOT_FOUND: &str = "QUERY_NOT_FOUND";
    pub const QUERY_START_FAILED: &str = "QUERY_START_FAILED";
    pub const QUERY_STOP_FAILED: &str = "QUERY_STOP_FAILED";
    pub const QUERY_DELETE_FAILED: &str = "QUERY_DELETE_FAILED";
    pub const QUERY_RESULTS_FAILED: &str = "QUERY_RESULTS_FAILED";

    pub const REACTION_CREATE_FAILED: &str = "REACTION_CREATE_FAILED";
    pub const REACTION_NOT_FOUND: &str = "REACTION_NOT_FOUND";
    pub const REACTION_START_FAILED: &str = "REACTION_START_FAILED";
    pub const REACTION_STOP_FAILED: &str = "REACTION_STOP_FAILED";
    pub const REACTION_DELETE_FAILED: &str = "REACTION_DELETE_FAILED";

    pub const COMPONENT_ALREADY_EXISTS: &str = "COMPONENT_ALREADY_EXISTS";
    pub const COMPONENT_NOT_FOUND: &str = "COMPONENT_NOT_FOUND";
    pub const INVALID_STATE: &str = "INVALID_STATE";
    pub const CONFIGURATION_ERROR: &str = "CONFIGURATION_ERROR";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub const READ_ONLY_MODE: &str = "READ_ONLY_MODE";
}

/// Detailed error information
#[derive(Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Error code for programmatic error handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Component ID if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
}

/// API error response structure
#[derive(Serialize, ToSchema)]
pub struct ApiErrorResponse {
    /// Always false for error responses
    pub success: bool,
    /// Error details
    pub error: ErrorDetail,
}

impl ApiErrorResponse {
    /// Create a new error response
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: None,
                component_id: None,
            },
        }
    }

    /// Create an error response with component ID
    pub fn with_component(
        code: impl Into<String>,
        message: impl Into<String>,
        component_id: impl Into<String>,
    ) -> Self {
        Self {
            success: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: None,
                component_id: Some(component_id.into()),
            },
        }
    }

    /// Create an error response with details
    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            success: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: Some(details),
                component_id: None,
            },
        }
    }
}

impl From<DrasiError> for ErrorDetail {
    fn from(error: DrasiError) -> Self {
        match error {
            DrasiError::ComponentNotFound(name) => ErrorDetail {
                code: error_codes::COMPONENT_NOT_FOUND.to_string(),
                message: format!("Component not found: {}", name),
                details: None,
                component_id: Some(name),
            },
            DrasiError::ComponentAlreadyExists(name) => ErrorDetail {
                code: error_codes::COMPONENT_ALREADY_EXISTS.to_string(),
                message: format!("Component already exists: {}", name),
                details: None,
                component_id: Some(name),
            },
            DrasiError::InvalidState(msg) => ErrorDetail {
                code: error_codes::INVALID_STATE.to_string(),
                message: format!("Invalid state: {}", msg),
                details: None,
                component_id: None,
            },
            DrasiError::Configuration(msg) => ErrorDetail {
                code: error_codes::CONFIGURATION_ERROR.to_string(),
                message: format!("Configuration error: {}", msg),
                details: None,
                component_id: None,
            },
            DrasiError::IoError(err) => ErrorDetail {
                code: error_codes::INTERNAL_ERROR.to_string(),
                message: format!("IO error: {}", err),
                details: None,
                component_id: None,
            },
            DrasiError::SerializationError(err) => ErrorDetail {
                code: error_codes::INTERNAL_ERROR.to_string(),
                message: format!("Serialization error: {}", err),
                details: None,
                component_id: None,
            },
            DrasiError::Other(err) => ErrorDetail {
                code: error_codes::INTERNAL_ERROR.to_string(),
                message: format!("Error: {}", err),
                details: None,
                component_id: None,
            },
        }
    }
}

/// Helper to determine HTTP status code from DrasiError
pub fn error_to_status_code(error: &DrasiError) -> StatusCode {
    match error {
        DrasiError::ComponentNotFound(_) => StatusCode::NOT_FOUND,
        DrasiError::ComponentAlreadyExists(_) => StatusCode::CONFLICT,
        DrasiError::Configuration(_) => StatusCode::BAD_REQUEST,
        DrasiError::InvalidState(_) => StatusCode::BAD_REQUEST,
        DrasiError::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        DrasiError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        DrasiError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
