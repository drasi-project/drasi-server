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
pub mod server;

// Main exports for library users
pub use builder::DrasiServerBuilder;
pub use builder_result::DrasiServerWithHandles;
pub use server::DrasiServer;

// Re-export from drasi-server-core
pub use drasi_server_core::{
    // Application types
    ApplicationHandle,
    ApplicationReactionHandle,
    ApplicationSourceHandle,
    ComponentEvent,
    // Channel types
    ComponentStatus,
    // Error types
    DrasiError,
    DrasiServerCore,
    DrasiServerCoreConfig as ServerConfig,
    PropertyMapBuilder,
    Query,
    QueryConfig,
    QueryManager,
    QueryResult,
    Reaction,
    ReactionConfig,
    ReactionManager,
    RuntimeConfig,
    // Core components
    Source,
    // Config types
    SourceConfig,
    SourceManager,
    SubscriptionOptions,
};

// Re-export from submodules in drasi_server_core that aren't in main lib
pub use drasi_server_core::channels::{BootstrapRequest, ComponentType, EventChannels};
pub use drasi_server_core::config::{
    ConfigPersistence, DrasiServerCoreSettings as ServerSettings, QueryJoinConfig,
    QueryJoinKeyConfig, QueryRuntime, ReactionRuntime, SourceRuntime,
};
pub use drasi_server_core::queries::LabelExtractor;
pub use drasi_server_core::routers::{BootstrapRouter, DataRouter, SubscriptionRouter};
