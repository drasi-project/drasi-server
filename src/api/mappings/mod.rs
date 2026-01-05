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

//! Mapping service for converting API DTOs to domain models with value resolution.
//!
//! This module provides a clean separation between API models (DTOs) and domain models,
//! with support for resolving environment variables and secrets.
//!
//! # Structure
//!
//! - `core/` - Core mapping infrastructure (resolver, mapper traits)
//! - `sources/` - Source configuration mappers  
//! - `reactions/` - Reaction configuration mappers

// Core infrastructure
pub mod core {
    pub mod mapper;
    pub mod resolver;

    pub use mapper::{ConfigMapper, DtoMapper, MappingError};
    pub use resolver::{EnvironmentVariableResolver, ResolverError, SecretResolver, ValueResolver};
}

// Server settings mapper
pub mod server_settings;

// Query mappers
pub mod queries;

// Source mappers
pub mod sources;

// Reaction mappers
pub mod reactions;

// Re-export commonly used types at module root for convenience
pub use core::*;
pub use queries::QueryConfigMapper;
pub use reactions::*;
pub use server_settings::{map_server_settings, ResolvedServerSettings};
pub use sources::*;
