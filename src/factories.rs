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

//! Factory functions for creating source, reaction, state store, and secret store instances from config.
//!
//! This module provides factory functions that use the PluginRegistry to look up
//! descriptors and create instances from generic config structs.

use anyhow::Result;
use drasi_lib::secret_store::SecretStoreProvider;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::{Reaction, Source};
use log::info;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

use crate::api::mappings::DtoMapper;
use crate::api::models::BootstrapProviderConfig;
use crate::config::{ReactionConfig, SecretStoreConfig, SourceConfig, StateStoreConfig};
use crate::plugin_registry::PluginRegistry;

use drasi_host_sdk::ConfigResolverFn;
use drasi_plugin_sdk::ffi::secret_store::FfiGetSecretResult;
use drasi_plugin_sdk::ffi::FfiStr;

// ============================================================================
// Host-side config value resolver (FFI callback for plugins)
// ============================================================================

/// Context passed to the host config resolver callback.
///
/// Contains the secret store provider and a tokio runtime handle for
/// resolving async `get_secret` calls from within synchronous FFI callbacks.
pub struct ConfigResolverContext {
    pub provider: Arc<dyn SecretStoreProvider>,
    pub runtime_handle: tokio::runtime::Handle,
}

/// Host-side `extern "C"` callback that plugins invoke (via `DtoMapper`) to
/// resolve `ConfigValue` references (secrets, env vars) back through the host.
///
/// The plugin serializes the `ConfigValue` to JSON and passes it here.
/// The host parses it, resolves the value using the appropriate store,
/// and returns the resolved string.
pub extern "C" fn host_resolve_config_value(
    ctx: *const c_void,
    config_value_json: FfiStr,
) -> FfiGetSecretResult {
    if ctx.is_null() {
        return FfiGetSecretResult::err("Config resolver context is null".to_string());
    }

    let context = unsafe { &*(ctx as *const ConfigResolverContext) };
    let json_str = unsafe { config_value_json.to_string() };

    // Parse the ConfigValue JSON to determine the kind
    let json_value: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            return FfiGetSecretResult::err(format!("Invalid config value JSON: {e}"));
        }
    };

    // Dispatch based on kind
    if let Some(kind) = json_value.get("kind").and_then(|v| v.as_str()) {
        match kind {
            "Secret" => {
                let name = match json_value.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => {
                        return FfiGetSecretResult::err(
                            "Secret config value missing 'name' field".to_string(),
                        );
                    }
                };

                let provider = context.provider.clone();
                let handle = context.runtime_handle.clone();

                // Resolve async get_secret from a non-tokio thread to avoid
                // blocking the plugin's tokio worker that called us.
                match std::thread::spawn(move || handle.block_on(provider.get_secret(&name))).join()
                {
                    Ok(Ok(value)) => FfiGetSecretResult::ok(value),
                    Ok(Err(e)) => FfiGetSecretResult::err(format!("Failed to resolve secret: {e}")),
                    Err(_) => {
                        FfiGetSecretResult::err("Secret resolution thread panicked".to_string())
                    }
                }
            }
            "EnvironmentVariable" => {
                let name = match json_value.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => {
                        return FfiGetSecretResult::err(
                            "EnvironmentVariable config value missing 'name' field".to_string(),
                        );
                    }
                };
                let default = json_value
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                match std::env::var(&name) {
                    Ok(v) => FfiGetSecretResult::ok(v),
                    Err(_) => match default {
                        Some(d) => FfiGetSecretResult::ok(d),
                        None => FfiGetSecretResult::err(format!(
                            "Environment variable '{name}' not found and no default provided"
                        )),
                    },
                }
            }
            other => FfiGetSecretResult::err(format!("Unknown config value kind: '{other}'")),
        }
    } else {
        // No "kind" field — treat as a static string value
        match json_value.as_str() {
            Some(s) => FfiGetSecretResult::ok(s.to_string()),
            None => FfiGetSecretResult::ok(json_str),
        }
    }
}

/// Build a leaked `ConfigResolverContext` pointer for injection into plugins.
///
/// The returned pointer is intentionally leaked (process-lifetime) because
/// plugins store it globally and need it for as long as they're loaded.
pub fn build_config_resolver_context(
    provider: Arc<dyn SecretStoreProvider>,
    runtime_handle: tokio::runtime::Handle,
) -> *mut c_void {
    let ctx = Box::new(ConfigResolverContext {
        provider,
        runtime_handle,
    });
    Box::into_raw(ctx) as *mut c_void
}

/// Get the config resolver callback function pointer.
pub fn config_resolver_callback() -> ConfigResolverFn {
    host_resolve_config_value
}

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

/// Create a secret store provider from a SecretStoreConfig using the plugin registry.
///
/// Looks up the `SecretStorePluginDescriptor` by kind from the registry,
/// then calls `create_secret_store()` with the config JSON.
pub async fn create_secret_store_from_registry(
    registry: &tokio::sync::RwLock<PluginRegistry>,
    config: &SecretStoreConfig,
) -> Result<Arc<dyn SecretStoreProvider>> {
    let descriptor = {
        let reg = registry.read().await;
        reg.get_secret_store(&config.kind).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "No secret store plugin registered for kind '{}'. \
                     Available: {:?}. Make sure the plugin is loaded.",
                config.kind,
                reg.secret_store_kinds()
            )
        })?
    };

    info!(
        "Creating secret store provider (kind: {}, config_version: {})",
        descriptor.kind(),
        descriptor.config_version()
    );

    let provider = descriptor.create_secret_store(&config.config).await?;
    // Box<dyn SecretStoreProvider> → Arc<dyn SecretStoreProvider>
    let arc: Arc<dyn SecretStoreProvider> = Arc::from(provider);
    Ok(arc)
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
