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

use drasi_server_core::{
    config::{DrasiServerCoreSettings as ServerSettings, QueryLanguage},
    DrasiError, DrasiServerCore, QueryConfig, ReactionConfig, SourceConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid;

/// Builder for creating a DrasiServer instance programmatically
pub struct DrasiServerBuilder {
    server_settings: ServerSettings,
    source_configs: Vec<SourceConfig>,
    query_configs: Vec<QueryConfig>,
    reaction_configs: Vec<ReactionConfig>,
    enable_api: bool,
    port: Option<u16>,
    host: Option<String>,
    config_file_path: Option<String>,
    application_source_names: Vec<String>,
    application_reaction_names: Vec<String>,
}

impl Default for DrasiServerBuilder {
    fn default() -> Self {
        Self {
            server_settings: ServerSettings {
                id: uuid::Uuid::new_v4().to_string(),
                priority_queue_capacity: None,
                broadcast_channel_capacity: None,
            },
            source_configs: Vec::new(),
            query_configs: Vec::new(),
            reaction_configs: Vec::new(),
            enable_api: false,
            port: Some(8080),
            host: Some("127.0.0.1".to_string()),
            config_file_path: None,
            application_source_names: Vec::new(),
            application_reaction_names: Vec::new(),
        }
    }
}

impl DrasiServerBuilder {
    /// Create a new DrasiServerBuilder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source configuration
    pub fn with_source(mut self, config: SourceConfig) -> Self {
        // Track application sources (both new and legacy formats)
        if config.source_type == "application" {
            self.application_source_names.push(config.id.clone());
        }
        self.source_configs.push(config);
        self
    }

    /// Add a source with name and type, using default properties
    pub fn with_simple_source(
        mut self,
        id: impl Into<String>,
        source_type: impl Into<String>,
    ) -> Self {
        self.source_configs.push(SourceConfig {
            id: id.into(),
            source_type: source_type.into(),
            auto_start: true,
            properties: std::collections::HashMap::new(),
            bootstrap_provider: None,
            broadcast_channel_capacity: None,
        });
        self
    }

    /// Add a query configuration
    pub fn with_query(mut self, config: QueryConfig) -> Self {
        self.query_configs.push(config);
        self
    }

    /// Add a query with simple parameters
    pub fn with_simple_query(
        mut self,
        id: impl Into<String>,
        query: impl Into<String>,
        sources: Vec<String>,
    ) -> Self {
        self.query_configs.push(QueryConfig {
            id: id.into(),
            query: query.into(),
            query_language: QueryLanguage::default(),
            sources,
            auto_start: true,
            properties: std::collections::HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
            broadcast_channel_capacity: None,
        });
        self
    }

    /// Add a reaction configuration
    pub fn with_reaction(mut self, config: ReactionConfig) -> Self {
        // Track application reactions (both new and legacy formats)
        if config.reaction_type == "application" {
            self.application_reaction_names.push(config.id.clone());
        }
        self.reaction_configs.push(config);
        self
    }

    /// Add a simple log reaction
    pub fn with_log_reaction(mut self, id: impl Into<String>, queries: Vec<String>) -> Self {
        self.reaction_configs.push(ReactionConfig {
            id: id.into(),
            reaction_type: "log".to_string(),
            queries,
            auto_start: true,
            priority_queue_capacity: None,
            properties: std::collections::HashMap::new(),
        });
        self
    }

    /// Enable the REST API on the default port
    pub fn enable_api(mut self) -> Self {
        self.enable_api = true;
        self
    }

    /// Enable the REST API on a specific port
    pub fn with_port(mut self, port: u16) -> Self {
        self.enable_api = true;
        self.port = Some(port);
        self
    }

    /// Enable the REST API on a specific host and port
    pub fn with_host_port(mut self, host: impl Into<String>, port: u16) -> Self {
        self.enable_api = true;
        self.host = Some(host.into());
        self.port = Some(port);
        self
    }

    /// Add an application source that can be programmatically controlled
    pub fn with_application_source(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.application_source_names.push(id.clone());
        self.source_configs.push(SourceConfig {
            id,
            source_type: "application".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
            broadcast_channel_capacity: None,
        });
        self
    }

    /// Add an application reaction that sends results to the application
    pub fn with_application_reaction(
        mut self,
        id: impl Into<String>,
        queries: Vec<String>,
    ) -> Self {
        let id = id.into();
        self.application_reaction_names.push(id.clone());
        self.reaction_configs.push(ReactionConfig {
            id,
            reaction_type: "application".to_string(),
            queries,
            auto_start: true,
            priority_queue_capacity: None,
            properties: HashMap::new(),
        });
        self
    }

    /// Build the DrasiServerCore instance
    pub async fn build_core(self) -> Result<DrasiServerCore, DrasiError> {
        // Use the public builder API from drasi-server-core
        let mut builder = DrasiServerCore::builder().with_id(&self.server_settings.id);

        // Add all sources
        for source_config in self.source_configs {
            builder = builder.add_source(source_config);
        }

        // Add all queries
        for query_config in self.query_configs {
            builder = builder.add_query(query_config);
        }

        // Add all reactions
        for reaction_config in self.reaction_configs {
            builder = builder.add_reaction(reaction_config);
        }

        // Build and initialize (the builder does both)
        builder.build().await
    }

    /// Set the config file path for persistence
    pub fn with_config_file(mut self, path: impl Into<String>) -> Self {
        self.config_file_path = Some(path.into());
        self
    }

    /// Build a DrasiServer instance with optional API
    pub async fn build(self) -> Result<crate::server::DrasiServer, DrasiError> {
        let api_enabled = self.enable_api;
        let host = self.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());
        let port = self.port.unwrap_or(8080);
        let config_file = self.config_file_path.clone();

        // Build the core server
        let core = self.build_core().await?;

        // Create the full server with optional features
        let server =
            crate::server::DrasiServer::from_core(core, api_enabled, host, port, config_file);

        Ok(server)
    }

    /// Build a DrasiServerCore instance and return application handles
    pub async fn build_with_handles(
        self,
    ) -> Result<crate::builder_result::DrasiServerWithHandles, DrasiError> {
        let app_source_names = self.application_source_names.clone();
        let app_reaction_names = self.application_reaction_names.clone();

        // Build the core server (already initialized by builder)
        let core = self.build_core().await?;

        // Convert to Arc and start
        let core = Arc::new(core);
        core.start().await?;

        // Collect application handles using the new public API
        let mut source_handles = HashMap::new();
        let mut reaction_handles = HashMap::new();

        // Get source handles using the new source_handle() method
        for source_name in app_source_names {
            match core.source_handle(&source_name) {
                Ok(handle) => {
                    source_handles.insert(source_name, handle);
                }
                Err(e) => {
                    log::warn!("Failed to get handle for source '{}': {}", source_name, e);
                }
            }
        }

        // Get reaction handles using the new reaction_handle() method
        for reaction_name in app_reaction_names {
            match core.reaction_handle(&reaction_name) {
                Ok(handle) => {
                    reaction_handles.insert(reaction_name, handle);
                }
                Err(e) => {
                    log::warn!(
                        "Failed to get handle for reaction '{}': {}",
                        reaction_name,
                        e
                    );
                }
            }
        }

        Ok(crate::builder_result::DrasiServerWithHandles {
            server: core,
            source_handles,
            reaction_handles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = DrasiServerBuilder::new();
        assert_eq!(builder.host, Some("127.0.0.1".to_string()));
        assert_eq!(builder.port, Some(8080));
        assert!(!builder.enable_api);
    }

    #[test]
    fn test_builder_fluent_api() {
        let builder = DrasiServerBuilder::new()
            .with_simple_source("test_source", "mock")
            .with_simple_query(
                "test_query",
                "MATCH (n) RETURN n",
                vec!["test_source".to_string()],
            )
            .with_log_reaction("test_reaction", vec!["test_query".to_string()])
            .with_port(9090);

        assert_eq!(builder.source_configs.len(), 1);
        assert_eq!(builder.query_configs.len(), 1);
        assert_eq!(builder.reaction_configs.len(), 1);
        assert!(builder.enable_api);
        assert_eq!(builder.port, Some(9090));
    }
}
