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

use anyhow::{Context, Result};
use drasi_lib::identity::{IdentityProvider, PasswordIdentityProvider};
use drasi_lib::secret_store::SecretStoreProvider;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::{Reaction, Source};
use log::info;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

use crate::api::mappings::DtoMapper;
use crate::api::models::{
    BootstrapProviderConfig, ConfigValue, IdentityProviderConfig, BUILTIN_PASSWORD_KIND,
};
use crate::config::{ReactionConfig, SecretStoreConfig, SourceConfig, StateStoreConfig};
use crate::plugin_registry::PluginRegistry;

use drasi_host_sdk::{ConfigResolverFn, SecretStoreValueResolverAdapter};
use drasi_plugin_sdk::ffi::secret_store::FfiGetSecretResult;
use drasi_plugin_sdk::ffi::FfiStr;
use drasi_plugin_sdk::resolver::{EnvironmentVariableResolver, ValueResolver};
use drasi_plugin_sdk::ConfigValue as SdkConfigValue;

// ============================================================================
// Host-side config value resolver (FFI callback for plugins)
// ============================================================================

/// Context passed to the host config resolver callback.
///
/// Contains a channel sender to dispatch resolution requests to a dedicated
/// resolver thread that owns the SDK resolvers (EnvironmentVariableResolver,
/// SecretStoreValueResolverAdapter).
pub struct ConfigResolverContext {
    resolver_tx: std::sync::mpsc::SyncSender<ResolveRequest>,
}

/// A request sent to the dedicated resolver thread.
struct ResolveRequest {
    config_value: SdkConfigValue<String>,
    response_tx: std::sync::mpsc::SyncSender<Result<String, String>>,
}

/// Host-side `extern "C"` callback that plugins invoke (via `DtoMapper`) to
/// resolve `ConfigValue` references (secrets, env vars) back through the host.
///
/// The plugin serializes the `ConfigValue` to JSON and passes it here.
/// The host deserializes it and dispatches to the appropriate SDK resolver.
pub extern "C" fn host_resolve_config_value(
    ctx: *const c_void,
    config_value_json: FfiStr,
) -> FfiGetSecretResult {
    if ctx.is_null() {
        return FfiGetSecretResult::err("Config resolver context is null".to_string());
    }

    let context = unsafe { &*(ctx as *const ConfigResolverContext) };
    let json_str = unsafe { config_value_json.to_string() };

    // Deserialize into SDK ConfigValue using the same serde logic the SDK uses.
    let config_value: SdkConfigValue<String> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            return FfiGetSecretResult::err(format!("Invalid config value JSON: {e}"));
        }
    };

    // Static values don't need resolution — return directly.
    if let SdkConfigValue::Static(ref s) = config_value {
        return FfiGetSecretResult::ok(s.clone());
    }

    // Dispatch to the resolver thread for Secret and EnvironmentVariable variants.
    let (response_tx, response_rx) = std::sync::mpsc::sync_channel(1);
    let request = ResolveRequest {
        config_value,
        response_tx,
    };

    if context.resolver_tx.send(request).is_err() {
        return FfiGetSecretResult::err("Config resolver thread is no longer running".to_string());
    }

    match response_rx.recv() {
        Ok(Ok(value)) => FfiGetSecretResult::ok(value),
        Ok(Err(e)) => FfiGetSecretResult::err(e),
        Err(_) => {
            FfiGetSecretResult::err("Config resolver thread dropped response channel".to_string())
        }
    }
}

/// Build a leaked `ConfigResolverContext` pointer for injection into plugins.
///
/// Spawns a dedicated resolver thread that uses the SDK's `ValueResolver`
/// implementations to handle all `ConfigValue` variants. The returned pointer
/// is intentionally leaked (process-lifetime) because plugins store it globally.
pub fn build_config_resolver_context(
    provider: Arc<dyn SecretStoreProvider>,
    runtime_handle: tokio::runtime::Handle,
) -> *mut c_void {
    let (tx, rx) = std::sync::mpsc::sync_channel::<ResolveRequest>(64);

    // Build the SDK resolvers
    let env_resolver = EnvironmentVariableResolver;
    let secret_resolver = SecretStoreValueResolverAdapter::new(provider);

    // Spawn a dedicated OS thread that runs resolution using the SDK resolvers.
    std::thread::Builder::new()
        .name("config-resolver".to_string())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let result = match &req.config_value {
                    SdkConfigValue::EnvironmentVariable { .. } => runtime_handle
                        .block_on(env_resolver.resolve_to_string(&req.config_value))
                        .map_err(|e| e.to_string()),
                    SdkConfigValue::Secret { .. } => runtime_handle
                        .block_on(secret_resolver.resolve_to_string(&req.config_value))
                        .map_err(|e| e.to_string()),
                    SdkConfigValue::Static(s) => Ok(s.clone()),
                };
                let _ = req.response_tx.send(result);
            }
        })
        .expect("Failed to spawn config-resolver thread");

    let ctx = Box::new(ConfigResolverContext { resolver_tx: tx });
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

    let bootstrap_descriptor = if let Some(bootstrap_config) = config
        .bootstrap_provider
        .as_ref()
        .and_then(|r| r.as_inline())
    {
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

    if let (Some(bootstrap_config), Some(bp_descriptor)) = (
        config
            .bootstrap_provider
            .as_ref()
            .and_then(|r| r.as_inline()),
        bootstrap_descriptor,
    ) {
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
        let bp_desc = if let Some(bp_config) = config
            .bootstrap_provider
            .as_ref()
            .and_then(|r| r.as_inline())
        {
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

    if let (Some(bootstrap_config), Some(bp_descriptor)) = (
        config
            .bootstrap_provider
            .as_ref()
            .and_then(|r| r.as_inline()),
        bootstrap_descriptor,
    ) {
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

/// Build a `{id -> BootstrapProviderConfig}` map from a slice of top-level
/// bootstrap provider configs.
///
/// Each entry must carry an `id`. Fails on a missing id or a duplicate id.
/// Unlike identity providers, bootstrap provider *instances* are not built
/// here: because `Source::set_bootstrap_provider` takes an owned value, each
/// referencing source instantiates its own provider from the shared config at
/// source-creation time.
pub fn build_bootstrap_provider_config_map(
    configs: &[BootstrapProviderConfig],
) -> Result<HashMap<String, BootstrapProviderConfig>> {
    let mut map: HashMap<String, BootstrapProviderConfig> = HashMap::new();
    for cfg in configs {
        let id = cfg.id().ok_or_else(|| {
            anyhow::anyhow!(
                "Top-level bootstrapProvider (kind '{}') is missing required 'id'",
                cfg.kind()
            )
        })?;
        if map.contains_key(id) {
            return Err(anyhow::anyhow!("Duplicate bootstrapProvider id '{id}'"));
        }
        map.insert(id.to_string(), cfg.clone());
    }
    Ok(map)
}

/// Resolve a source's `bootstrapProvider` reference (if any) against the
/// top-level bootstrap provider config map, rewriting it to an inline
/// definition so the source-creation path can instantiate it.
///
/// Inline bootstrap providers and sources without a bootstrap provider are
/// left unchanged. Returns an error if the referenced id is not declared.
pub fn resolve_source_bootstrap_provider(
    config: &mut SourceConfig,
    bootstrap_providers: &HashMap<String, BootstrapProviderConfig>,
) -> Result<()> {
    if let Some(id) = config
        .bootstrap_provider
        .as_ref()
        .and_then(|r| r.as_reference())
        .map(str::to_string)
    {
        let resolved = bootstrap_providers.get(&id).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Source '{}' references unknown bootstrapProvider '{id}'. Declared providers: {:?}",
                config.id(),
                bootstrap_providers.keys().collect::<Vec<_>>()
            )
        })?;
        config.bootstrap_provider =
            Some(crate::api::models::BootstrapProviderRef::Inline(resolved));
    }
    Ok(())
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

/// Create a single identity provider instance from configuration.
///
/// For the built-in `password` kind, this constructs a
/// `PasswordIdentityProvider` directly from drasi-lib without consulting the
/// plugin registry. All other kinds are resolved via
/// `PluginRegistry::get_identity_provider(kind)` and built through the
/// plugin's `create_identity_provider` factory.
pub async fn create_identity_provider(
    registry: &PluginRegistry,
    config: &IdentityProviderConfig,
) -> Result<Arc<dyn IdentityProvider>> {
    if config.kind == BUILTIN_PASSWORD_KIND {
        // Deserialize the inner config into a typed DTO so that `username` and
        // `password` participate in the `ConfigValue` envelope system. This
        // allows them to be supplied as plain strings, `${ENV_VAR}` POSIX
        // references, or structured `{kind: Secret, name: ...}` /
        // `{kind: EnvironmentVariable, ...}` objects — same as every other
        // plugin-provided config field. Without this, secrets and env vars
        // would be read as their literal text.
        #[derive(serde::Deserialize)]
        struct PasswordIdpDto {
            username: ConfigValue<String>,
            password: ConfigValue<String>,
        }

        let dto: PasswordIdpDto =
            serde_json::from_value(config.config.clone()).with_context(|| {
                format!(
                    "identity provider '{}': invalid 'password' configuration \
                     (expected 'username' and 'password' fields)",
                    config.id
                )
            })?;

        let mapper = DtoMapper::new();
        let username = mapper.resolve_string(&dto.username).with_context(|| {
            format!(
                "identity provider '{}': failed to resolve 'username'",
                config.id
            )
        })?;
        let password = mapper.resolve_string(&dto.password).with_context(|| {
            format!(
                "identity provider '{}': failed to resolve 'password'",
                config.id
            )
        })?;

        return Ok(Arc::new(PasswordIdentityProvider::new(
            &username, &password,
        )));
    }

    let descriptor = registry
        .get_identity_provider(&config.kind)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown identity provider kind: '{}'. Available: {:?}",
                config.kind,
                registry.identity_provider_kinds()
            )
        })?;

    let provider = descriptor
        .create_identity_provider(&config.config)
        .await
        .with_context(|| {
            format!(
                "Failed to create identity provider '{}' (kind '{}')",
                config.id, config.kind,
            )
        })?;

    Ok(Arc::from(provider))
}

/// Acquire the registry read lock and create a single identity provider.
pub async fn create_identity_provider_locked(
    registry: &tokio::sync::RwLock<PluginRegistry>,
    config: &IdentityProviderConfig,
) -> Result<Arc<dyn IdentityProvider>> {
    if config.kind == BUILTIN_PASSWORD_KIND {
        let reg = registry.read().await;
        return create_identity_provider(&reg, config).await;
    }

    // For plugin-backed providers, clone the descriptor under the lock and
    // drop the guard before awaiting plugin construction.
    let descriptor = {
        let reg = registry.read().await;
        reg.get_identity_provider(&config.kind)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown identity provider kind: '{}'. Available: {:?}",
                    config.kind,
                    reg.identity_provider_kinds()
                )
            })?
    };

    let provider = descriptor
        .create_identity_provider(&config.config)
        .await
        .with_context(|| {
            format!(
                "Failed to create identity provider '{}' (kind '{}')",
                config.id, config.kind,
            )
        })?;

    Ok(Arc::from(provider))
}

/// Build a `{id -> provider}` map from a slice of identity-provider configs.
///
/// Fails on duplicate ids or if any plugin-backed kind is not registered.
pub async fn build_identity_provider_map(
    registry: &tokio::sync::RwLock<PluginRegistry>,
    configs: &[IdentityProviderConfig],
) -> Result<HashMap<String, Arc<dyn IdentityProvider>>> {
    let mut map: HashMap<String, Arc<dyn IdentityProvider>> = HashMap::new();
    for cfg in configs {
        if map.contains_key(&cfg.id) {
            return Err(anyhow::anyhow!(
                "Duplicate identityProvider id '{}'",
                cfg.id
            ));
        }
        let provider = create_identity_provider_locked(registry, cfg).await?;
        info!(
            "Configured identity provider '{}' (kind '{}')",
            cfg.id, cfg.kind
        );
        map.insert(cfg.id.clone(), provider);
    }
    Ok(map)
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
            id: None,
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

    // ==========================================================================
    // Built-in Password Identity Provider — ConfigValue Envelope Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_password_identity_provider_static_values() {
        let registry = test_registry();
        let config = IdentityProviderConfig {
            kind: BUILTIN_PASSWORD_KIND.to_string(),
            id: "pg-static".to_string(),
            config: serde_json::json!({
                "username": "drasi",
                "password": "s3cret",
            }),
        };

        create_identity_provider(&registry, &config)
            .await
            .expect("static values should work");
    }

    #[tokio::test]
    async fn test_password_identity_provider_env_var_reference() {
        // SAFETY: tests are single-threaded per-process for env var manipulation
        // but cargo test runs them in parallel by default. Use a unique var name
        // to avoid collisions.
        let var_name = "DRASI_TEST_PG_PASSWORD_ENVELOPE_ENV";
        std::env::set_var(var_name, "resolved-from-env");

        let registry = test_registry();
        let config = IdentityProviderConfig {
            kind: BUILTIN_PASSWORD_KIND.to_string(),
            id: "pg-env".to_string(),
            config: serde_json::json!({
                "username": "drasi",
                "password": format!("${{{var_name}}}"),
            }),
        };

        let result = create_identity_provider(&registry, &config).await;
        std::env::remove_var(var_name);
        result.expect("`${VAR}` reference should resolve via ConfigValue envelope");
    }

    #[tokio::test]
    async fn test_password_identity_provider_env_var_with_default() {
        let registry = test_registry();
        let config = IdentityProviderConfig {
            kind: BUILTIN_PASSWORD_KIND.to_string(),
            id: "pg-default".to_string(),
            config: serde_json::json!({
                "username": "drasi",
                "password": "${DRASI_DEFINITELY_UNSET_VAR:-fallback-pw}",
            }),
        };

        create_identity_provider(&registry, &config)
            .await
            .expect("`${VAR:-default}` should fall back to the default");
    }

    #[tokio::test]
    async fn test_password_identity_provider_structured_env_var() {
        let var_name = "DRASI_TEST_PG_PASSWORD_STRUCTURED_ENV";
        std::env::set_var(var_name, "structured-resolved");

        let registry = test_registry();
        let config = IdentityProviderConfig {
            kind: BUILTIN_PASSWORD_KIND.to_string(),
            id: "pg-structured".to_string(),
            config: serde_json::json!({
                "username": "drasi",
                "password": {
                    "kind": "EnvironmentVariable",
                    "name": var_name,
                },
            }),
        };

        let result = create_identity_provider(&registry, &config).await;
        std::env::remove_var(var_name);
        result.expect("structured EnvironmentVariable reference should resolve");
    }

    #[tokio::test]
    async fn test_password_identity_provider_missing_field_errors() {
        let registry = test_registry();
        let config = IdentityProviderConfig {
            kind: BUILTIN_PASSWORD_KIND.to_string(),
            id: "pg-missing".to_string(),
            config: serde_json::json!({
                "username": "drasi",
                // password missing
            }),
        };

        let err = match create_identity_provider(&registry, &config).await {
            Ok(_) => panic!("missing password field must be rejected"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("password") || msg.contains("invalid"),
            "Unexpected error: {msg}"
        );
    }
}
