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

pub mod api;
pub mod builder;
pub mod builder_result;
pub mod config;
pub mod persistence;
pub mod server;

// Main exports for library users
pub use builder::DrasiServerBuilder;
pub use builder_result::DrasiServerWithHandles;
pub use config::{DrasiServerConfig, ServerSettings};
pub use server::DrasiServer;

// Re-export from drasi-server-core (public API only)
pub use drasi_server_core::{
    // Application handle types
    ApplicationReactionHandle,
    ApplicationSourceHandle,
    // Error types
    DrasiError,
    // Core server
    DrasiServerCore,
    DrasiServerCoreConfig as ServerConfig,
    // Property utilities
    PropertyMapBuilder,
    // Builder types
    Query,
    // Config types (still public for file-based config)
    QueryConfig,
    Reaction,
    ReactionConfig,
    RuntimeConfig,
    Source,
    SourceConfig,
    SubscriptionOptions,
};

// Re-export types from internal modules (these are visible but marked as internal)
// We need these for the wrapper API functionality
pub use drasi_server_core::channels::ComponentStatus;
pub use drasi_server_core::config::{DrasiServerCoreSettings, QueryJoinConfig, QueryJoinKeyConfig};
