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
#[cfg(feature = "builtin-plugins")]
pub mod builtin_plugins;
pub mod config;
#[cfg(feature = "dynamic-plugins")]
pub mod dynamic_loading;
pub mod factories;
pub mod instance_registry;
pub mod persistence;
#[cfg(feature = "dynamic-plugins")]
pub mod plugin_install;
pub mod plugin_lockfile;
pub mod plugin_registry;
pub mod server;

// Main exports for library users
pub use builder::DrasiServerBuilder;
pub use builder_result::DrasiServerWithHandles;
#[cfg(feature = "builtin-plugins")]
pub use builtin_plugins::register_builtin_plugins;
pub use config::{
    load_config_file, save_config_file, ConfigError, DrasiLibInstanceConfig, DrasiServerConfig,
    PluginDependency, ReactionConfig, ResolvedInstanceConfig, SourceConfig, StateStoreConfig,
    default_plugin_registry,
};
pub use factories::{create_reaction, create_source, create_state_store_provider};
pub use plugin_registry::PluginRegistry;
pub use server::register_core_plugins;
pub use server::DrasiServer;

// Re-export the Plugin SDK for library users
pub use drasi_plugin_sdk;

// Re-export API models and mappings for external use
pub use api::mappings;
pub use api::models;

// Re-export from drasi-lib (public API only)
pub use drasi_lib::{
    // Error types
    DrasiError,
    // Core server
    DrasiLib,
    DrasiLibConfig as ServerConfig,
    // Builder types
    Query,
    // Config types for API and file-based config
    QueryConfig,
    RuntimeConfig,
};

// Re-export types from internal modules (these are visible but marked as internal)
// We need these for the wrapper API functionality
pub use drasi_lib::channels::ComponentStatus;
pub use drasi_lib::config::{QueryJoinConfig, QueryJoinKeyConfig};
