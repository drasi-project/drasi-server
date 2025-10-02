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
    QueryConfig, ReactionConfig, SourceConfig, RuntimeConfig,
    config::{DrasiServerCoreSettings as ServerSettings, QueryLanguage},
    DrasiError, DrasiServerCore, ApplicationHandle
};
use std::sync::Arc;
use std::collections::HashMap;

/// Builder for creating a DrasiServer instance programmatically
pub struct DrasiServerBuilder {
    server_settings: ServerSettings,
    source_configs: Vec<SourceConfig>,
    query_configs: Vec<QueryConfig>,
    reaction_configs: Vec<ReactionConfig>,
    enable_api: bool,
    api_port: Option<u16>,
    api_host: Option<String>,
    enable_config_persistence: bool,
    config_file_path: Option<String>,
    application_source_names: Vec<String>,
    application_reaction_names: Vec<String>,
}

impl Default for DrasiServerBuilder {
    fn default() -> Self {
        Self {
            server_settings: ServerSettings {
                host: "127.0.0.1".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                max_connections: 100,
                shutdown_timeout_seconds: 30,
                disable_persistence: false,
            },
            source_configs: Vec::new(),
            query_configs: Vec::new(),
            reaction_configs: Vec::new(),
            enable_api: false,
            api_port: None,
            api_host: None,
            enable_config_persistence: false,
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

    /// Set the server log level
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.server_settings.log_level = level.into();
        self
    }

    /// Disable persistence of configuration changes
    pub fn disable_persistence(mut self) -> Self {
        self.server_settings.disable_persistence = true;
        self
    }

    /// Add a source configuration
    pub fn with_source(mut self, config: SourceConfig) -> Self {
        // Track application sources
        if config.source_type == "internal.application" {
            self.application_source_names.push(config.id.clone());
        }
        self.source_configs.push(config);
        self
    }

    /// Add a source with name and type, using default properties
    pub fn with_simple_source(mut self, id: impl Into<String>, source_type: impl Into<String>) -> Self {
        self.source_configs.push(SourceConfig {
            id: id.into(),
            source_type: source_type.into(),
            auto_start: true,
            properties: std::collections::HashMap::new(),
            bootstrap_provider: None,
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
        });
        self
    }

    /// Add a reaction configuration
    pub fn with_reaction(mut self, config: ReactionConfig) -> Self {
        // Track application reactions
        if config.reaction_type == "internal.application" {
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
    pub fn enable_api_with_port(mut self, port: u16) -> Self {
        self.enable_api = true;
        self.api_port = Some(port);
        self
    }

    /// Enable the REST API on a specific host and port
    pub fn enable_api_with_host_port(mut self, host: impl Into<String>, port: u16) -> Self {
        self.enable_api = true;
        self.api_host = Some(host.into());
        self.api_port = Some(port);
        self
    }

    /// Enable configuration persistence to a file
    pub fn enable_config_persistence(mut self, file_path: impl Into<String>) -> Self {
        self.enable_config_persistence = true;
        self.config_file_path = Some(file_path.into());
        self
    }
    
    /// Add an application source that can be programmatically controlled
    pub fn with_application_source(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.application_source_names.push(id.clone());
        self.source_configs.push(SourceConfig {
            id,
            source_type: "internal.application".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        });
        self
    }
    
    /// Add an application reaction that sends results to the application
    pub fn with_application_reaction(mut self, id: impl Into<String>, queries: Vec<String>) -> Self {
        let id = id.into();
        self.application_reaction_names.push(id.clone());
        self.reaction_configs.push(ReactionConfig {
            id,
            reaction_type: "internal.application".to_string(),
            queries,
            auto_start: true,
            properties: HashMap::new(),
        });
        self
    }

    /// Build the DrasiServerCore instance
    pub async fn build_core(self) -> Result<DrasiServerCore, DrasiError> {
        // Create RuntimeConfig from builder settings
        let runtime_config = RuntimeConfig {
            server: self.server_settings,
            sources: self.source_configs,
            queries: self.query_configs,
            reactions: self.reaction_configs,
        };

        // Create server core
        let mut server_core = DrasiServerCore::new(Arc::new(runtime_config));
        
        // Initialize components
        server_core.initialize().await?;

        Ok(server_core)
    }

    /// Build a DrasiServer instance with optional API
    pub async fn build(self) -> Result<crate::server::DrasiServer, DrasiError> {
        let api_enabled = self.enable_api;
        let api_host = self.api_host.clone().unwrap_or_else(|| self.server_settings.host.clone());
        let api_port = self.api_port.unwrap_or(self.server_settings.port);
        let config_persistence = self.enable_config_persistence;
        let config_file = self.config_file_path.clone();

        // Build the core server
        let core = self.build_core().await?;

        // Create the full server with optional features
        let server = crate::server::DrasiServer::from_core(
            core,
            api_enabled,
            api_host,
            api_port,
            config_persistence,
            config_file,
        );

        Ok(server)
    }
    
    /// Build a DrasiServerCore instance and return application handles
    pub async fn build_with_handles(self) -> Result<crate::builder_result::DrasiServerWithHandles, DrasiError> {
        let app_source_names = self.application_source_names.clone();
        let app_reaction_names = self.application_reaction_names.clone();
        
        // Build the core server
        let mut core = self.build_core().await?;
        
        // Initialize the core
        core.initialize().await?;
        
        // Convert to Arc and start
        let core = Arc::new(core);
        core.start().await?;
        
        // Collect application handles
        let mut handles = HashMap::new();
        
        // Get source handles
        for source_name in app_source_names {
            if let Some(source_handle) = core.source_manager().get_application_handle(&source_name).await {
                handles.insert(
                    source_name.clone(),
                    ApplicationHandle::source_only(source_handle),
                );
            }
        }
        
        // Get reaction handles
        for reaction_name in app_reaction_names {
            if let Some(reaction_handle) = core.reaction_manager().get_application_handle(&reaction_name).await {
                if let Some(existing) = handles.get_mut(&reaction_name) {
                    // If we already have a source handle with the same name, combine them
                    if let Some(source) = existing.source.clone() {
                        *existing = ApplicationHandle::new(source, reaction_handle);
                    }
                } else {
                    handles.insert(
                        reaction_name.clone(),
                        ApplicationHandle::reaction_only(reaction_handle),
                    );
                }
            }
        }
        
        Ok(crate::builder_result::DrasiServerWithHandles {
            server: core,
            handles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = DrasiServerBuilder::new();
        assert_eq!(builder.server_settings.host, "127.0.0.1");
        assert_eq!(builder.server_settings.port, 8080);
        assert_eq!(builder.server_settings.log_level, "info");
        assert!(!builder.enable_api);
        assert!(!builder.enable_config_persistence);
    }

    #[test]
    fn test_builder_fluent_api() {
        let builder = DrasiServerBuilder::new()
            .with_log_level("debug")
            .with_simple_source("test_source", "internal.mock")
            .with_simple_query("test_query", "MATCH (n) RETURN n", vec!["test_source".to_string()])
            .with_log_reaction("test_reaction", vec!["test_query".to_string()])
            .enable_api_with_port(9090)
            .enable_config_persistence("test.yaml");

        assert_eq!(builder.server_settings.log_level, "debug");
        assert_eq!(builder.source_configs.len(), 1);
        assert_eq!(builder.query_configs.len(), 1);
        assert_eq!(builder.reaction_configs.len(), 1);
        assert!(builder.enable_api);
        assert_eq!(builder.api_port, Some(9090));
        assert!(builder.enable_config_persistence);
        assert_eq!(builder.config_file_path, Some("test.yaml".to_string()));
    }
}