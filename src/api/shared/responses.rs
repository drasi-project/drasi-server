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

//! Common response types shared across API versions.

use drasi_lib::channels::ComponentStatus;
use serde::Serialize;
use utoipa::ToSchema;

/// Health check response
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    /// Health status of the server
    pub status: String,
    /// Current server timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Response listing a component with its status
#[derive(Serialize, ToSchema)]
pub struct ComponentListItem {
    /// ID of the component
    pub id: String,
    /// Current status of the component
    pub status: ComponentStatus,
    /// Error message if the component is in error state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Hypermedia links for this component
    pub links: ComponentLinks,
    /// Optional component configuration (only present when view=full)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// Hypermedia links for a component
#[derive(Serialize, ToSchema)]
pub struct ComponentLinks {
    /// Link to the status view of the component
    #[serde(rename = "self")]
    pub self_link: String,
    /// Link to the full configuration view of the component
    pub full: String,
}

/// Response listing a DrasiLib instance
#[derive(Serialize, ToSchema)]
pub struct InstanceListItem {
    /// ID of the DrasiLib instance
    pub id: String,
}

/// Generic API response wrapper
#[derive(Serialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful
    pub success: bool,
    /// Response data if successful
    pub data: Option<T>,
    /// Error message if unsuccessful
    pub error: Option<String>,
}

/// Generic API Response schema for OpenAPI documentation
#[derive(Serialize, ToSchema)]
#[schema(as = ApiResponse)]
pub struct ApiResponseSchema {
    /// Whether the request was successful
    pub success: bool,
    /// Response data if successful
    pub data: Option<serde_json::Value>,
    /// Error message if unsuccessful
    pub error: Option<String>,
}

/// Simple status message response
#[derive(Serialize, ToSchema)]
pub struct StatusResponse {
    /// Status message
    pub message: String,
}

/// Response listing available API versions
#[derive(Serialize, ToSchema)]
pub struct ApiVersionsResponse {
    /// List of available API versions
    pub versions: Vec<String>,
    /// The current/latest API version
    pub current: String,
}

impl<T> ApiResponse<T> {
    /// Create a successful response with data
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}
