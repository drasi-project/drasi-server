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
use drasi_lib::DrasiError;
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
    pub const QUERY_RESULTS_UNAVAILABLE: &str = "QUERY_RESULTS_UNAVAILABLE";

    pub const REACTION_CREATE_FAILED: &str = "REACTION_CREATE_FAILED";
    pub const REACTION_NOT_FOUND: &str = "REACTION_NOT_FOUND";
    pub const REACTION_START_FAILED: &str = "REACTION_START_FAILED";
    pub const REACTION_STOP_FAILED: &str = "REACTION_STOP_FAILED";
    pub const REACTION_DELETE_FAILED: &str = "REACTION_DELETE_FAILED";

    pub const CONFIG_READ_ONLY: &str = "CONFIG_READ_ONLY";
    pub const DUPLICATE_RESOURCE: &str = "DUPLICATE_RESOURCE";
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}

/// API error response structure
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error code for programmatic handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ErrorDetail>,
}

/// Additional error details
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Component type if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_type: Option<String>,
    /// Component ID if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    /// Technical error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_details: Option<String>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Add details to the error response
    pub fn with_details(mut self, details: ErrorDetail) -> Self {
        self.details = Some(details);
        self
    }

    /// Convert to a specific status code
    pub fn with_status(self) -> (StatusCode, axum::Json<Self>) {
        let status = status_from_code(&self.code);
        (status, axum::Json(self))
    }
}

/// Convert an error code to an HTTP status code
fn status_from_code(code: &str) -> StatusCode {
    match code {
        error_codes::SOURCE_NOT_FOUND
        | error_codes::QUERY_NOT_FOUND
        | error_codes::REACTION_NOT_FOUND => StatusCode::NOT_FOUND,

        error_codes::CONFIG_READ_ONLY | error_codes::DUPLICATE_RESOURCE => StatusCode::CONFLICT,

        error_codes::INVALID_REQUEST => StatusCode::BAD_REQUEST,

        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Convert DrasiError to ErrorResponse
impl From<DrasiError> for ErrorResponse {
    fn from(err: DrasiError) -> Self {
        use DrasiError::*;

        match &err {
            ComponentNotFound { component_type, component_id } => {
                let code = match component_type.as_str() {
                    "source" => error_codes::SOURCE_NOT_FOUND,
                    "query" => error_codes::QUERY_NOT_FOUND,
                    "reaction" => error_codes::REACTION_NOT_FOUND,
                    _ => error_codes::INTERNAL_ERROR,
                };

                ErrorResponse::new(code, format!("{} '{}' not found", component_type, component_id))
            }
            AlreadyExists { component_type, component_id } => {
                ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    format!("{} '{}' already exists", component_type, component_id),
                )
            }
            InvalidConfig { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            InvalidState { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            Validation { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            OperationFailed { component_type, component_id, operation, reason } => {
                ErrorResponse::new(
                    error_codes::INTERNAL_ERROR,
                    format!("Failed to {} {} '{}': {}", operation, component_type, component_id, reason),
                )
            }
            Internal(ref err) => {
                ErrorResponse::new(error_codes::INTERNAL_ERROR, err.to_string())
            }
        }
    }
}

/// Convert DrasiError to HTTP status code
pub fn drasi_error_to_status(err: &DrasiError) -> StatusCode {
    use DrasiError::*;

    match err {
        ComponentNotFound { .. } => StatusCode::NOT_FOUND,
        AlreadyExists { .. } => StatusCode::CONFLICT,
        InvalidConfig { .. } | InvalidState { .. } | Validation { .. } => StatusCode::BAD_REQUEST,
        OperationFailed { .. } | Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
