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

//! REST API implementation for Drasi Server.
//!
//! This module provides the HTTP API endpoints for managing sources, queries, and reactions.
//! The API uses URL-based versioning with all endpoints prefixed with `/api/v1/`.
//!
//! ## API Structure
//!
//! ```text
//! /health                                    - Health check (unversioned)
//! /api/versions                              - List available API versions
//! /api/v1/instances                          - List DrasiLib instances
//! /api/v1/instances/{id}/sources             - Source management
//! /api/v1/instances/{id}/queries             - Query management
//! /api/v1/instances/{id}/reactions           - Reaction management
//! /api/v1/sources                            - First instance sources (convenience)
//! /api/v1/queries                            - First instance queries (convenience)
//! /api/v1/reactions                          - First instance reactions (convenience)
//! ```
//!
//! ## Module Organization
//!
//! - `shared` - Common types and handlers shared across API versions
//! - `v1` - API version 1 implementation
//! - `version` - Version constants and utilities
//! - `models` - Data Transfer Objects (DTOs) for API requests/responses
//! - `mappings` - Conversion between DTOs and domain models

pub mod mappings;
pub mod models;
pub mod shared;
pub mod v1;
pub mod version;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod joins_tests;

// Re-export commonly used types from shared module
pub use shared::error::*;
pub use shared::responses::*;

// Re-export v1 handlers and types for convenience
pub use v1::handlers::*;
pub use v1::openapi::inject_plugin_schemas;
pub use v1::openapi::ApiDocV1;
pub use v1::routes::build_v1_router;

// Re-export version utilities
pub use version::{ApiVersion, API_CURRENT_VERSION};
