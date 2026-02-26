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

//! Plugin registry for managing dynamically-registered plugin descriptors.
//!
//! The [`PluginRegistry`] holds all known plugin descriptors (sources, reactions,
//! bootstrappers) and provides lookup, creation, and schema introspection methods.
//! It replaces the hardcoded factory match arms and enum variants with a dynamic
//! dispatch mechanism.

use drasi_plugin_sdk::{
    BootstrapPluginDescriptor, ReactionPluginDescriptor, SourcePluginDescriptor,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of all known plugin descriptors.
///
/// Plugins are registered at startup either from:
/// - Statically-linked built-in plugins (via [`register_builtin_plugins`])
/// - Dynamically-loaded shared libraries (future)
/// - Programmatic registration via the builder API
///
/// The registry is immutable after construction and shared across the server
/// via `Arc<PluginRegistry>`.
pub struct PluginRegistry {
    sources: HashMap<String, Arc<dyn SourcePluginDescriptor>>,
    reactions: HashMap<String, Arc<dyn ReactionPluginDescriptor>>,
    bootstrappers: HashMap<String, Arc<dyn BootstrapPluginDescriptor>>,
}

/// Information about a registered plugin kind.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginKindInfo {
    /// The plugin kind identifier.
    pub kind: String,
    /// The semver version of the plugin's configuration DTO.
    pub config_version: String,
    /// The plugin's configuration schema as a JSON string.
    pub config_schema_json: String,
    /// The OpenAPI schema name for this plugin's config DTO.
    pub config_schema_name: String,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            reactions: HashMap::new(),
            bootstrappers: HashMap::new(),
        }
    }

    /// Register a source plugin descriptor.
    ///
    /// If a source with the same kind is already registered, it is replaced.
    pub fn register_source(&mut self, descriptor: Arc<dyn SourcePluginDescriptor>) {
        let kind = descriptor.kind().to_string();
        self.sources.insert(kind, descriptor);
    }

    /// Register a reaction plugin descriptor.
    pub fn register_reaction(&mut self, descriptor: Arc<dyn ReactionPluginDescriptor>) {
        let kind = descriptor.kind().to_string();
        self.reactions.insert(kind, descriptor);
    }

    /// Register a bootstrap plugin descriptor.
    pub fn register_bootstrapper(&mut self, descriptor: Arc<dyn BootstrapPluginDescriptor>) {
        let kind = descriptor.kind().to_string();
        self.bootstrappers.insert(kind, descriptor);
    }

    /// Look up a source plugin descriptor by kind.
    pub fn get_source(&self, kind: &str) -> Option<&Arc<dyn SourcePluginDescriptor>> {
        self.sources.get(kind)
    }

    /// Look up a reaction plugin descriptor by kind.
    pub fn get_reaction(&self, kind: &str) -> Option<&Arc<dyn ReactionPluginDescriptor>> {
        self.reactions.get(kind)
    }

    /// Look up a bootstrap plugin descriptor by kind.
    pub fn get_bootstrapper(&self, kind: &str) -> Option<&Arc<dyn BootstrapPluginDescriptor>> {
        self.bootstrappers.get(kind)
    }

    /// List all registered source kinds.
    pub fn source_kinds(&self) -> Vec<&str> {
        let mut kinds: Vec<&str> = self.sources.keys().map(String::as_str).collect();
        kinds.sort();
        kinds
    }

    /// List all registered reaction kinds.
    pub fn reaction_kinds(&self) -> Vec<&str> {
        let mut kinds: Vec<&str> = self.reactions.keys().map(String::as_str).collect();
        kinds.sort();
        kinds
    }

    /// List all registered bootstrapper kinds.
    pub fn bootstrapper_kinds(&self) -> Vec<&str> {
        let mut kinds: Vec<&str> = self.bootstrappers.keys().map(String::as_str).collect();
        kinds.sort();
        kinds
    }

    /// Get detailed info about all registered source plugins.
    pub fn source_plugin_infos(&self) -> Vec<PluginKindInfo> {
        let mut infos: Vec<PluginKindInfo> = self
            .sources
            .values()
            .map(|d| PluginKindInfo {
                kind: d.kind().to_string(),
                config_version: d.config_version().to_string(),
                config_schema_json: d.config_schema_json(),
                config_schema_name: d.config_schema_name().to_string(),
            })
            .collect();
        infos.sort_by(|a, b| a.kind.cmp(&b.kind));
        infos
    }

    /// Get detailed info about all registered reaction plugins.
    pub fn reaction_plugin_infos(&self) -> Vec<PluginKindInfo> {
        let mut infos: Vec<PluginKindInfo> = self
            .reactions
            .values()
            .map(|d| PluginKindInfo {
                kind: d.kind().to_string(),
                config_version: d.config_version().to_string(),
                config_schema_json: d.config_schema_json(),
                config_schema_name: d.config_schema_name().to_string(),
            })
            .collect();
        infos.sort_by(|a, b| a.kind.cmp(&b.kind));
        infos
    }

    /// Get detailed info about all registered bootstrap plugins.
    pub fn bootstrapper_plugin_infos(&self) -> Vec<PluginKindInfo> {
        let mut infos: Vec<PluginKindInfo> = self
            .bootstrappers
            .values()
            .map(|d| PluginKindInfo {
                kind: d.kind().to_string(),
                config_version: d.config_version().to_string(),
                config_schema_json: d.config_schema_json(),
                config_schema_name: d.config_schema_name().to_string(),
            })
            .collect();
        infos.sort_by(|a, b| a.kind.cmp(&b.kind));
        infos
    }

    /// Returns true if the registry contains no descriptors.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.reactions.is_empty() && self.bootstrappers.is_empty()
    }

    /// Returns the total number of registered descriptors.
    pub fn descriptor_count(&self) -> usize {
        self.sources.len() + self.reactions.len() + self.bootstrappers.len()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("sources", &self.source_kinds())
            .field("reactions", &self.reaction_kinds())
            .field("bootstrappers", &self.bootstrapper_kinds())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drasi_lib::sources::Source;

    struct MockSourceDescriptor {
        kind: &'static str,
    }

    #[async_trait]
    impl SourcePluginDescriptor for MockSourceDescriptor {
        fn kind(&self) -> &str {
            self.kind
        }
        fn config_version(&self) -> &str {
            "1.0.0"
        }
        fn config_schema_json(&self) -> String {
            r#"{"MockSourceConfig":{"type":"object","properties":{"host":{"type":"string"}}}}"#
                .to_string()
        }
        fn config_schema_name(&self) -> &str {
            "MockSourceConfig"
        }
        async fn create_source(
            &self,
            _id: &str,
            _config_json: &serde_json::Value,
            _auto_start: bool,
        ) -> anyhow::Result<Box<dyn Source>> {
            anyhow::bail!("mock: not implemented")
        }
    }

    struct MockReactionDescriptor {
        kind: &'static str,
    }

    #[async_trait]
    impl ReactionPluginDescriptor for MockReactionDescriptor {
        fn kind(&self) -> &str {
            self.kind
        }
        fn config_version(&self) -> &str {
            "1.0.0"
        }
        fn config_schema_json(&self) -> String {
            r#"{"MockReactionConfig":{"type":"object"}}"#.to_string()
        }
        fn config_schema_name(&self) -> &str {
            "MockReactionConfig"
        }
        async fn create_reaction(
            &self,
            _id: &str,
            _query_ids: Vec<String>,
            _config_json: &serde_json::Value,
            _auto_start: bool,
        ) -> anyhow::Result<Box<dyn drasi_lib::reactions::Reaction>> {
            anyhow::bail!("mock: not implemented")
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.descriptor_count(), 0);
    }

    #[test]
    fn test_register_source() {
        let mut registry = PluginRegistry::new();
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "mock" }));

        assert_eq!(registry.source_kinds(), vec!["mock"]);
        assert!(registry.get_source("mock").is_some());
        assert!(registry.get_source("nonexistent").is_none());
        assert_eq!(registry.descriptor_count(), 1);
    }

    #[test]
    fn test_register_reaction() {
        let mut registry = PluginRegistry::new();
        registry.register_reaction(Arc::new(MockReactionDescriptor { kind: "log" }));

        assert_eq!(registry.reaction_kinds(), vec!["log"]);
        assert!(registry.get_reaction("log").is_some());
    }

    #[test]
    fn test_source_plugin_infos() {
        let mut registry = PluginRegistry::new();
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "mock" }));

        let infos = registry.source_plugin_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].kind, "mock");
        assert_eq!(infos[0].config_version, "1.0.0");
        assert!(infos[0].config_schema_json.contains("object"));
    }

    #[test]
    fn test_duplicate_kind_replaces() {
        let mut registry = PluginRegistry::new();
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "mock" }));
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "mock" }));

        assert_eq!(registry.source_kinds(), vec!["mock"]);
        assert_eq!(registry.descriptor_count(), 1);
    }

    #[test]
    fn test_debug_output() {
        let mut registry = PluginRegistry::new();
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "postgres" }));
        registry.register_reaction(Arc::new(MockReactionDescriptor { kind: "log" }));

        let debug = format!("{registry:?}");
        assert!(debug.contains("postgres"));
        assert!(debug.contains("log"));
    }

    #[test]
    fn test_kinds_are_sorted() {
        let mut registry = PluginRegistry::new();
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "zeta" }));
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "alpha" }));
        registry.register_source(Arc::new(MockSourceDescriptor { kind: "beta" }));

        assert_eq!(registry.source_kinds(), vec!["alpha", "beta", "zeta"]);
    }
}
