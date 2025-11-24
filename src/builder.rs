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

use drasi_lib::{DrasiError, DrasiServerCore, DrasiServerCoreBuilder, Query, Reaction, Source};
use std::collections::HashMap;
use std::sync::Arc;

/// Builder for creating a DrasiServer instance programmatically
pub struct DrasiServerBuilder {
    core_builder: DrasiServerCoreBuilder,
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
            core_builder: DrasiServerCore::builder(),
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

    /// Set the server ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.core_builder = self.core_builder.with_id(id);
        self
    }

    /// Add a source using the new builder API
    /// The source should be built using Source::application("id").build() or similar
    pub fn with_source_config(
        mut self,
        id: impl Into<String>,
        source_type: impl Into<String>,
    ) -> Self {
        let id = id.into();
        let source_type = source_type.into();

        // Track application sources
        if source_type == "application" {
            self.application_source_names.push(id.clone());
        }

        // Build the appropriate source configuration
        let source_config = if source_type == "application" {
            Source::application(&id).auto_start(true).build()
        } else if source_type == "mock" {
            Source::mock(&id).auto_start(true).build()
        } else {
            // For other source types, default to mock (in real usage, you'd handle each type)
            Source::mock(&id).auto_start(true).build()
        };

        self.core_builder = self.core_builder.add_source(source_config);
        self
    }

    /// Add a source with name and type, using default properties
    pub fn with_simple_source(self, id: impl Into<String>, source_type: impl Into<String>) -> Self {
        self.with_source_config(id, source_type)
    }

    /// Add a query using the new builder API
    /// The query should be built using Query::cypher("id").build() or similar
    pub fn with_query_config(
        mut self,
        id: impl Into<String>,
        query_str: impl Into<String>,
        sources: Vec<String>,
    ) -> Self {
        let mut query_builder = Query::cypher(id).query(query_str);

        for source in sources {
            query_builder = query_builder.from_source(source);
        }

        self.core_builder = self.core_builder.add_query(query_builder.build());
        self
    }

    /// Add a query with simple parameters
    pub fn with_simple_query(
        self,
        id: impl Into<String>,
        query_str: impl Into<String>,
        sources: Vec<String>,
    ) -> Self {
        self.with_query_config(id, query_str, sources)
    }

    /// Add a reaction using the new builder API
    /// The reaction should be built using Reaction::log("id").build() or similar
    pub fn with_reaction_config(
        mut self,
        id: impl Into<String>,
        reaction_type: impl Into<String>,
        queries: Vec<String>,
    ) -> Self {
        let id = id.into();
        let reaction_type = reaction_type.into();

        // Track application reactions
        if reaction_type == "application" {
            self.application_reaction_names.push(id.clone());
        }

        // Build the appropriate reaction configuration
        let reaction_config = if reaction_type == "application" {
            let mut builder = Reaction::application(&id);
            for query in queries {
                builder = builder.subscribe_to(query);
            }
            builder.build()
        } else if reaction_type == "log" {
            let mut builder = Reaction::log(&id);
            for query in queries {
                builder = builder.subscribe_to(query);
            }
            builder.build()
        } else {
            // Default to log reaction
            let mut builder = Reaction::log(&id);
            for query in queries {
                builder = builder.subscribe_to(query);
            }
            builder.build()
        };

        self.core_builder = self.core_builder.add_reaction(reaction_config);
        self
    }

    /// Add a simple log reaction
    pub fn with_log_reaction(self, id: impl Into<String>, queries: Vec<String>) -> Self {
        self.with_reaction_config(id, "log", queries)
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

        let source = Source::application(&id).auto_start(true).build();
        self.core_builder = self.core_builder.add_source(source);
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

        let mut reaction_builder = Reaction::application(&id);
        for query in queries {
            reaction_builder = reaction_builder.subscribe_to(query);
        }

        self.core_builder = self.core_builder.add_reaction(reaction_builder.build());
        self
    }

    /// Build the DrasiServerCore instance
    pub async fn build_core(self) -> Result<DrasiServerCore, DrasiError> {
        // Build and return the core using the new API
        self.core_builder.build().await
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

        // Start the server
        core.start().await?;

        // Collect application handles using the new public API
        let mut source_handles = HashMap::new();
        let mut reaction_handles = HashMap::new();

        // Get source handles using the public API
        for source_name in app_source_names {
            match core.source_handle(&source_name).await {
                Ok(handle) => {
                    source_handles.insert(source_name, handle);
                }
                Err(e) => {
                    log::warn!("Failed to get handle for source '{}': {}", source_name, e);
                }
            }
        }

        // Get reaction handles using the public API
        for reaction_name in app_reaction_names {
            match core.reaction_handle(&reaction_name).await {
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
            server: Arc::new(core),
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

        assert!(builder.enable_api);
        assert_eq!(builder.port, Some(9090));
    }
}
