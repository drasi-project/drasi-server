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

//! Error types and error handling utilities shared across API versions.

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
            ComponentNotFound {
                component_type,
                component_id,
            } => {
                let code = match component_type.as_str() {
                    "source" => error_codes::SOURCE_NOT_FOUND,
                    "query" => error_codes::QUERY_NOT_FOUND,
                    "reaction" => error_codes::REACTION_NOT_FOUND,
                    _ => error_codes::INTERNAL_ERROR,
                };

                ErrorResponse::new(code, format!("{component_type} '{component_id}' not found"))
            }
            AlreadyExists {
                component_type,
                component_id,
            } => ErrorResponse::new(
                error_codes::DUPLICATE_RESOURCE,
                format!("{component_type} '{component_id}' already exists"),
            ),
            InvalidConfig { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            InvalidState { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            Validation { message } => {
                ErrorResponse::new(error_codes::INVALID_REQUEST, message.clone())
            }
            OperationFailed {
                component_type,
                component_id,
                operation,
                reason,
            } => ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to {operation} {component_type} '{component_id}': {reason}"),
            ),
            Internal(ref err) => ErrorResponse::new(error_codes::INTERNAL_ERROR, err.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // ErrorResponse Tests
    // ==========================================================================

    #[test]
    fn test_error_response_new() {
        let response = ErrorResponse::new("TEST_CODE", "Test message");
        assert_eq!(response.code, "TEST_CODE");
        assert_eq!(response.message, "Test message");
        assert!(response.details.is_none());
    }

    #[test]
    fn test_error_response_with_details() {
        let details = ErrorDetail {
            component_type: Some("source".to_string()),
            component_id: Some("test-source".to_string()),
            technical_details: Some("connection failed".to_string()),
        };

        let response = ErrorResponse::new("TEST_CODE", "Test message").with_details(details);

        assert_eq!(response.code, "TEST_CODE");
        assert!(response.details.is_some());
        let d = response.details.unwrap();
        assert_eq!(d.component_type, Some("source".to_string()));
        assert_eq!(d.component_id, Some("test-source".to_string()));
        assert_eq!(d.technical_details, Some("connection failed".to_string()));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::new("TEST_CODE", "Test message");
        let json = serde_json::to_string(&response).expect("Failed to serialize");

        assert!(json.contains("\"code\":\"TEST_CODE\""));
        assert!(json.contains("\"message\":\"Test message\""));
        // details should be omitted when None
        assert!(!json.contains("details"));
    }

    #[test]
    fn test_error_response_serialization_with_details() {
        let details = ErrorDetail {
            component_type: Some("query".to_string()),
            component_id: None,
            technical_details: None,
        };

        let response = ErrorResponse::new("TEST_CODE", "Test message").with_details(details);
        let json = serde_json::to_string(&response).expect("Failed to serialize");

        assert!(json.contains("\"details\""));
        assert!(json.contains("\"component_type\":\"query\""));
        // Null fields should be omitted
        assert!(!json.contains("component_id"));
        assert!(!json.contains("technical_details"));
    }

    // ==========================================================================
    // status_from_code Tests
    // ==========================================================================

    #[test]
    fn test_status_from_code_not_found() {
        assert_eq!(
            status_from_code(error_codes::SOURCE_NOT_FOUND),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_from_code(error_codes::QUERY_NOT_FOUND),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_from_code(error_codes::REACTION_NOT_FOUND),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_status_from_code_conflict() {
        assert_eq!(
            status_from_code(error_codes::CONFIG_READ_ONLY),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_from_code(error_codes::DUPLICATE_RESOURCE),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn test_status_from_code_bad_request() {
        assert_eq!(
            status_from_code(error_codes::INVALID_REQUEST),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_status_from_code_internal_error() {
        assert_eq!(
            status_from_code(error_codes::INTERNAL_ERROR),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        // Unknown codes should also be internal server error
        assert_eq!(
            status_from_code("UNKNOWN_CODE"),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_status_from_code_operation_failures() {
        // All operation failures should be internal server error
        assert_eq!(
            status_from_code(error_codes::SOURCE_CREATE_FAILED),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_from_code(error_codes::QUERY_START_FAILED),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_from_code(error_codes::REACTION_DELETE_FAILED),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    // ==========================================================================
    // DrasiError Conversion Tests
    // ==========================================================================

    #[test]
    fn test_drasi_error_component_not_found_source() {
        let err = DrasiError::ComponentNotFound {
            component_type: "source".to_string(),
            component_id: "test-source".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::SOURCE_NOT_FOUND);
        assert!(response.message.contains("source"));
        assert!(response.message.contains("test-source"));
    }

    #[test]
    fn test_drasi_error_component_not_found_query() {
        let err = DrasiError::ComponentNotFound {
            component_type: "query".to_string(),
            component_id: "test-query".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::QUERY_NOT_FOUND);
    }

    #[test]
    fn test_drasi_error_component_not_found_reaction() {
        let err = DrasiError::ComponentNotFound {
            component_type: "reaction".to_string(),
            component_id: "test-reaction".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::REACTION_NOT_FOUND);
    }

    #[test]
    fn test_drasi_error_already_exists() {
        let err = DrasiError::AlreadyExists {
            component_type: "source".to_string(),
            component_id: "duplicate-source".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::DUPLICATE_RESOURCE);
        assert!(response.message.contains("already exists"));
    }

    #[test]
    fn test_drasi_error_invalid_config() {
        let err = DrasiError::InvalidConfig {
            message: "Missing required field".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::INVALID_REQUEST);
        assert!(response.message.contains("Missing required field"));
    }

    #[test]
    fn test_drasi_error_validation() {
        let err = DrasiError::Validation {
            message: "Port must be > 0".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::INVALID_REQUEST);
    }

    #[test]
    fn test_drasi_error_operation_failed() {
        let err = DrasiError::OperationFailed {
            component_type: "source".to_string(),
            component_id: "failing-source".to_string(),
            operation: "start".to_string(),
            reason: "connection refused".to_string(),
        };

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::INTERNAL_ERROR);
        assert!(response.message.contains("start"));
        assert!(response.message.contains("connection refused"));
    }

    #[test]
    fn test_drasi_error_internal() {
        let err = DrasiError::Internal(anyhow::anyhow!("Something went wrong"));

        let response: ErrorResponse = err.into();
        assert_eq!(response.code, error_codes::INTERNAL_ERROR);
        assert!(response.message.contains("Something went wrong"));
    }

    // ==========================================================================
    // drasi_error_to_status Tests
    // ==========================================================================

    #[test]
    fn test_drasi_error_to_status_not_found() {
        let err = DrasiError::ComponentNotFound {
            component_type: "source".to_string(),
            component_id: "test".to_string(),
        };
        assert_eq!(drasi_error_to_status(&err), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_drasi_error_to_status_conflict() {
        let err = DrasiError::AlreadyExists {
            component_type: "query".to_string(),
            component_id: "test".to_string(),
        };
        assert_eq!(drasi_error_to_status(&err), StatusCode::CONFLICT);
    }

    #[test]
    fn test_drasi_error_to_status_bad_request() {
        let err1 = DrasiError::InvalidConfig {
            message: "test".to_string(),
        };
        let err2 = DrasiError::InvalidState {
            message: "test".to_string(),
        };
        let err3 = DrasiError::Validation {
            message: "test".to_string(),
        };

        assert_eq!(drasi_error_to_status(&err1), StatusCode::BAD_REQUEST);
        assert_eq!(drasi_error_to_status(&err2), StatusCode::BAD_REQUEST);
        assert_eq!(drasi_error_to_status(&err3), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_drasi_error_to_status_internal_error() {
        let err1 = DrasiError::OperationFailed {
            component_type: "source".to_string(),
            component_id: "test".to_string(),
            operation: "start".to_string(),
            reason: "failed".to_string(),
        };
        let err2 = DrasiError::Internal(anyhow::anyhow!("test"));

        assert_eq!(
            drasi_error_to_status(&err1),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            drasi_error_to_status(&err2),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    // ==========================================================================
    // Error Codes Tests
    // ==========================================================================

    #[test]
    fn test_error_codes_are_unique() {
        let codes = vec![
            error_codes::SOURCE_CREATE_FAILED,
            error_codes::SOURCE_NOT_FOUND,
            error_codes::SOURCE_START_FAILED,
            error_codes::SOURCE_STOP_FAILED,
            error_codes::SOURCE_DELETE_FAILED,
            error_codes::QUERY_CREATE_FAILED,
            error_codes::QUERY_NOT_FOUND,
            error_codes::QUERY_START_FAILED,
            error_codes::QUERY_STOP_FAILED,
            error_codes::QUERY_DELETE_FAILED,
            error_codes::QUERY_RESULTS_UNAVAILABLE,
            error_codes::REACTION_CREATE_FAILED,
            error_codes::REACTION_NOT_FOUND,
            error_codes::REACTION_START_FAILED,
            error_codes::REACTION_STOP_FAILED,
            error_codes::REACTION_DELETE_FAILED,
            error_codes::CONFIG_READ_ONLY,
            error_codes::DUPLICATE_RESOURCE,
            error_codes::INVALID_REQUEST,
            error_codes::INTERNAL_ERROR,
        ];

        let mut unique: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for code in &codes {
            assert!(unique.insert(code), "Duplicate error code found: {code}");
        }
        assert_eq!(unique.len(), codes.len());
    }
}
