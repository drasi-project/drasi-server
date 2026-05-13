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

//! Factory functions for creating source, reaction, and state store instances from config.
//!
//! This module provides factory functions that use the PluginRegistry to look up
//! descriptors and create instances from generic config structs.

use anyhow::Result;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::{Reaction, Source};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;

use crate::api::mappings::DtoMapper;
use crate::api::models::BootstrapProviderConfig;
use crate::config::{ReactionConfig, SourceConfig, StateStoreConfig};
use crate::plugin_registry::PluginRegistry;

/// Create a source instance from a SourceConfig using the plugin registry.
///
/// Looks up the source and bootstrap descriptors under the registry reference,
/// then creates instances without holding a borrow on the registry across await
/// points. Callers that hold an `Arc<RwLock<PluginRegistry>>` should use
/// [`create_source_locked`] instead to avoid holding the lock across async
/// creation calls.
pub async fn create_source(
    registry: &PluginRegistry,
    config: SourceConfig,
) -> Result<Box<dyn Source + 'static>> {
    // Clone Arc descriptors so we don't borrow `registry` across awaits
    let descriptor = registry.get_source(&config.kind).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown source kind: '{}'. Available: {:?}",
            config.kind,
            registry.source_kinds()
        )
    })?;

    let bootstrap_descriptor = if let Some(bootstrap_config) = &config.bootstrap_provider {
        let kind = bootstrap_config.kind();
        Some(registry.get_bootstrapper(kind).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown bootstrap kind: '{}'. Available: {:?}",
                kind,
                registry.bootstrapper_kinds()
            )
        })?)
    } else {
        None
    };

    // All registry borrows are done — create instances without holding the registry
    let source = descriptor
        .create_source(&config.id, &config.config, config.auto_start)
        .await?;

    if let (Some(bootstrap_config), Some(bp_descriptor)) =
        (&config.bootstrap_provider, bootstrap_descriptor)
    {
        let provider = bp_descriptor
            .create_bootstrap_provider(&bootstrap_config.config, &config.config)
            .await?;
        info!("Setting bootstrap provider for source '{}'", config.id());
        source.set_bootstrap_provider(provider).await;
    }

    Ok(source)
}

/// Create a source from config, acquiring and releasing the registry lock
/// internally so the caller never holds a read guard across await points.
pub async fn create_source_locked(
    registry: &tokio::sync::RwLock<PluginRegistry>,
    config: SourceConfig,
) -> Result<(Box<dyn Source + 'static>, HashMap<String, String>)> {
    let (descriptor, bootstrap_descriptor, plugin_meta) = {
        let reg = registry.read().await;
        let desc = reg.get_source(&config.kind).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown source kind: '{}'. Available: {:?}",
                config.kind,
                reg.source_kinds()
            )
        })?;
        let bp_desc = if let Some(bp_config) = &config.bootstrap_provider {
            let kind = bp_config.kind();
            Some(reg.get_bootstrapper(kind).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown bootstrap kind: '{}'. Available: {:?}",
                    kind,
                    reg.bootstrapper_kinds()
                )
            })?)
        } else {
            None
        };
        let meta = get_source_plugin_metadata(&reg, &config.kind);
        (desc, bp_desc, meta)
    }; // lock dropped here

    let source = descriptor
        .create_source(&config.id, &config.config, config.auto_start)
        .await?;

    if let (Some(bootstrap_config), Some(bp_descriptor)) =
        (&config.bootstrap_provider, bootstrap_descriptor)
    {
        let provider = bp_descriptor
            .create_bootstrap_provider(&bootstrap_config.config, &config.config)
            .await?;
        info!("Setting bootstrap provider for source '{}'", config.id());
        source.set_bootstrap_provider(provider).await;
    }

    Ok((source, plugin_meta))
}

/// Create a bootstrap provider from configuration using the plugin registry.
pub async fn create_bootstrap_provider(
    registry: &PluginRegistry,
    bootstrap_config: &BootstrapProviderConfig,
    source_config_json: &serde_json::Value,
) -> Result<Box<dyn drasi_lib::bootstrap::BootstrapProvider + 'static>> {
    let kind = bootstrap_config.kind();
    let descriptor = registry.get_bootstrapper(kind).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown bootstrap kind: '{}'. Available: {:?}",
            kind,
            registry.bootstrapper_kinds()
        )
    })?;

    descriptor
        .create_bootstrap_provider(&bootstrap_config.config, source_config_json)
        .await
}

/// Create a reaction instance from a ReactionConfig using the plugin registry.
///
/// Looks up the reaction descriptor under the registry reference, then creates
/// the instance. Callers that hold an `Arc<RwLock<PluginRegistry>>` should use
/// [`create_reaction_locked`] instead to avoid holding the lock across async
/// creation calls.
pub async fn create_reaction(
    registry: &PluginRegistry,
    config: ReactionConfig,
) -> Result<Box<dyn Reaction + 'static>> {
    let descriptor = registry
        .get_reaction(&config.kind)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown reaction kind: '{}'. Available: {:?}",
                config.kind,
                registry.reaction_kinds()
            )
        })?;

    descriptor
        .create_reaction(
            &config.id,
            config.queries.clone(),
            &config.config,
            config.auto_start,
        )
        .await
}

/// Create a reaction from config, acquiring and releasing the registry lock
/// internally so the caller never holds a read guard across await points.
pub async fn create_reaction_locked(
    registry: &tokio::sync::RwLock<PluginRegistry>,
    config: ReactionConfig,
) -> Result<(Box<dyn Reaction + 'static>, HashMap<String, String>)> {
    let (descriptor, plugin_meta) = {
        let reg = registry.read().await;
        let desc = reg.get_reaction(&config.kind).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown reaction kind: '{}'. Available: {:?}",
                config.kind,
                reg.reaction_kinds()
            )
        })?;
        let meta = get_reaction_plugin_metadata(&reg, &config.kind);
        (desc, meta)
    }; // lock dropped here

    let reaction = descriptor
        .create_reaction(
            &config.id,
            config.queries.clone(),
            &config.config,
            config.auto_start,
        )
        .await?;

    Ok((reaction, plugin_meta))
}

/// Create a state store provider from a StateStoreConfig.
pub fn create_state_store_provider(
    config: StateStoreConfig,
) -> Result<Arc<dyn StateStoreProvider + Send + Sync + 'static>> {
    let mapper = DtoMapper::new();

    match config {
        StateStoreConfig::Redb { path } => {
            use drasi_state_store_redb::RedbStateStoreProvider;

            let resolved_path: String = mapper.resolve_typed(&path)?;
            info!("Creating REDB state store provider with path: {resolved_path}");

            let provider = RedbStateStoreProvider::new(&resolved_path)?;
            Ok(Arc::new(provider))
        }
    }
}

/// Get plugin metadata for a source kind from the registry.
///
/// Returns a HashMap with `pluginId` and `pluginVersion`
/// if the kind is backed by a registered plugin. Core (statically-linked)
/// plugins return an empty map.
pub fn get_source_plugin_metadata(
    registry: &PluginRegistry,
    kind: &str,
) -> HashMap<String, String> {
    let mut meta = HashMap::new();
    if let Some(reg) = registry.get_source_registration(kind) {
        if !reg.plugin_id.is_empty() {
            meta.insert("pluginId".to_string(), reg.plugin_id.clone());
            meta.insert(
                "pluginVersion".to_string(),
                reg.descriptor.config_version().to_string(),
            );
        }
    }
    meta
}

/// Get plugin metadata for a reaction kind from the registry.
///
/// Returns a HashMap with `pluginId` and `pluginVersion`
/// if the kind is backed by a registered plugin. Core (statically-linked)
/// plugins return an empty map.
pub fn get_reaction_plugin_metadata(
    registry: &PluginRegistry,
    kind: &str,
) -> HashMap<String, String> {
    let mut meta = HashMap::new();
    if let Some(reg) = registry.get_reaction_registration(kind) {
        if !reg.plugin_id.is_empty() {
            meta.insert("pluginId".to_string(), reg.plugin_id.clone());
            meta.insert(
                "pluginVersion".to_string(),
                reg.descriptor.config_version().to_string(),
            );
        }
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_registry() -> PluginRegistry {
        let mut registry = PluginRegistry::new();
        // Register core plugins (noop, application)
        crate::server::register_core_plugins(&mut registry);
        registry
    }

    // ==========================================================================
    // State Store Provider Factory Tests
    // ==========================================================================

    #[test]
    fn test_create_redb_state_store_provider() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let path = temp_dir.path().join("state.redb");

        let config = StateStoreConfig::Redb {
            path: crate::api::models::ConfigValue::Static(path.to_string_lossy().to_string()),
        };

        let provider = create_state_store_provider(config).expect("Failed to create REDB provider");
        assert!(std::sync::Arc::strong_count(&provider) >= 1);
    }

    #[test]
    fn test_create_redb_state_store_provider_creates_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let path = temp_dir.path().join("test_store.redb");

        let config = StateStoreConfig::Redb {
            path: crate::api::models::ConfigValue::Static(path.to_string_lossy().to_string()),
        };

        let _provider = create_state_store_provider(config).expect("Failed to create provider");
        assert!(path.exists(), "REDB file should be created");
    }

    // ==========================================================================
    // Bootstrap Provider Factory Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_create_noop_bootstrap_provider() {
        let registry = test_registry();
        let bootstrap_config = BootstrapProviderConfig {
            kind: "noop".to_string(),
            config: serde_json::json!({}),
        };
        let source_config_json = serde_json::json!({});

        let result =
            create_bootstrap_provider(&registry, &bootstrap_config, &source_config_json).await;
        assert!(result.is_ok(), "Failed to create noop bootstrap provider");
    }

    // ==========================================================================
    // Error Handling Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_unknown_source_kind_rejected() {
        let registry = test_registry();
        let config = SourceConfig {
            kind: "nonexistent".to_string(),
            id: "test".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            identity_provider: None,
            config: serde_json::json!({}),
        };

        let result = create_source(&registry, config).await;
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("Unknown source kind"),
            "Unexpected error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_unknown_reaction_kind_rejected() {
        let registry = test_registry();
        let config = ReactionConfig {
            kind: "nonexistent".to_string(),
            id: "test".to_string(),
            queries: vec![],
            auto_start: true,
            identity_provider: None,
            config: serde_json::json!({}),
        };

        let result = create_reaction(&registry, config).await;
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("Unknown reaction kind"),
            "Unexpected error: {err_msg}"
        );
    }
}
