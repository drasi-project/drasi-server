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

use drasi_lib::plugin_core::{
    IndexBackendPlugin, Reaction as ReactionTrait, Source as SourceTrait,
};
use drasi_lib::{DrasiError, DrasiLib, DrasiLibBuilder, Query};
use std::sync::Arc;

/// Builder for creating a DrasiServer instance programmatically
pub struct DrasiServerBuilder {
    core_builder: DrasiLibBuilder,
    enable_api: bool,
    port: Option<u16>,
    host: Option<String>,
    config_file_path: Option<String>,
}

impl Default for DrasiServerBuilder {
    fn default() -> Self {
        Self {
            core_builder: DrasiLib::builder(),
            enable_api: false,
            port: Some(8080),
            host: Some("127.0.0.1".to_string()),
            config_file_path: None,
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

    /// Add a pre-built source instance (ownership transferred)
    pub fn with_source(mut self, source: impl SourceTrait + 'static) -> Self {
        self.core_builder = self.core_builder.with_source(source);
        self
    }

    /// Add a pre-built reaction instance (ownership transferred)
    pub fn with_reaction(mut self, reaction: impl ReactionTrait + 'static) -> Self {
        self.core_builder = self.core_builder.with_reaction(reaction);
        self
    }

    /// Add an index provider for persistent storage
    ///
    /// By default, DrasiLib uses in-memory indexes. Use this method to inject
    /// a persistent index provider like RocksDB.
    pub fn with_index_provider(mut self, provider: Arc<dyn IndexBackendPlugin>) -> Self {
        self.core_builder = self.core_builder.with_index_provider(provider);
        self
    }

    /// Add a query using the builder API
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

        self.core_builder = self.core_builder.with_query(query_builder.build());
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

    /// Build the DrasiLib instance
    pub async fn build_core(self) -> Result<DrasiLib, DrasiError> {
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

    /// Build a DrasiLib instance, start it, and return a handle
    ///
    /// Note: Application source/reaction handles were removed during the plugin architecture refactor.
    /// Use the builder pattern in drasi-lib directly for programmatic integration.
    pub async fn build_with_handles(
        self,
    ) -> Result<crate::builder_result::DrasiServerWithHandles, DrasiError> {
        // Build the core server (already initialized by builder)
        let core = self.build_core().await?;

        // Start the server
        core.start().await?;

        Ok(crate::builder_result::DrasiServerWithHandles {
            server: Arc::new(core),
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
            .with_simple_query(
                "test_query",
                "MATCH (n) RETURN n",
                vec!["test_source".to_string()],
            )
            .with_port(9090);

        assert!(builder.enable_api);
        assert_eq!(builder.port, Some(9090));
    }
}
