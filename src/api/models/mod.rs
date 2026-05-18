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

//! API models module - DTO types for configuration.
//!
//! This module contains all Data Transfer Object (DTO) types used in the API.
//! DTOs are organized into submodules matching the structure of the mappings module.
//!
//! # Organization
//!
//! - **`sources/`**: DTOs for data source configurations
//!   - `postgres` - PostgreSQL source
//!   - `http_source` - HTTP source
//!   - `grpc_source` - gRPC source
//!   - `mock` - Mock source for testing
//!
//! - **Reaction configs**: Provided dynamically by plugin descriptors
//!
//! - **`queries/`**: DTOs for query configurations
//!   - `query` - Continuous query configuration
//!
//! - **`config_value`**: Generic configuration value types for static/environment variable/secret references

// Bootstrap provider module
pub mod bootstrap;

// Organized submodules
pub mod observability;
pub mod queries;
pub mod reaction;
pub mod solution;
pub mod source;
pub mod state_store;

// Re-export all DTO types for convenient access
pub use bootstrap::BootstrapProviderConfig;
pub use drasi_plugin_sdk::config_value::*;
pub use observability::*;
pub use queries::*;
pub use reaction::ReactionConfig;
pub use source::SourceConfig;
pub use state_store::{RedbStateStoreConfigDto, StateStoreConfig};
