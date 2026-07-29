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

//! Dynamic plugin loading using the Drasi Host SDK.
//!
//! Plugins are self-contained cdylib `.so`/`.dylib`/`.dll` files with their own
//! tokio runtime, communicating with the host via `#[repr(C)]` vtable structs.
//!
//! Each plugin is fully self-contained and communicates through a stable C ABI.
//! No shared runtime, `RTLD_GLOBAL`, or identical compiler versions are required.

use crate::plugin_registry::PluginRegistry;
use anyhow::Result;
use drasi_host_sdk::callbacks::{self, CallbackContext};
use drasi_host_sdk::loader::{PluginLoader, PluginLoaderConfig};
use drasi_host_sdk::plugin_types::{PluginCategory, PluginKindEntry};
use drasi_host_sdk::ConfigResolverFn;
use drasi_plugin_sdk::descriptor::SecretStorePluginDescriptor;
use drasi_plugin_sdk::{
    BootstrapPluginDescriptor, IdentityProviderPluginDescriptor, ReactionPluginDescriptor,
    SourcePluginDescriptor,
};
use log::{debug, info, warn};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// File patterns for discovering cdylib plugins.
///
/// Matches on the Drasi plugin filename prefix rather than enumerating every
/// plugin type.  This means a new plugin type is never silently skipped just
/// because its type name was not added to this list.
///
/// Any matched file still has to export `drasi_plugin_metadata()` — files that
/// do not are logged and skipped by `PluginLoader::load_all`, so matching more
/// broadly costs at most one extra `dlopen` rather than loading something that
/// is not a plugin.
///
/// The two entries cover:
/// - Unix shared libraries (`libdrasi_<type>_<name>.so` / `.dylib`)
/// - Windows DLLs (`drasi_<type>_<name>.dll`)
/// Both underscored and hyphenated type names (e.g. `secret_store` vs
/// `secret-store`) are matched because both begin with the `drasi_` prefix.
const PLUGIN_FILE_PATTERNS: &[&str] = &["libdrasi_*", "drasi_*"];

/// Statistics from a cdylib plugin loading operation.
#[derive(Debug, Default)]
pub struct PluginLoadStats {
    pub plugins_loaded: usize,
    pub plugins_failed: usize,
    /// Number of shared-library files in the plugin directory that did not
    /// match any plugin file pattern and were therefore not attempted.
    /// A non-zero value may indicate a misconfigured or unrecognised plugin.
    pub plugins_skipped: usize,
    pub source_descriptors: usize,
    pub reaction_descriptors: usize,
    pub bootstrap_descriptors: usize,
    pub secret_store_descriptors: usize,
    pub identity_provider_descriptors: usize,
    /// Per-plugin information for orchestrator registration.
    pub loaded_plugins: Vec<StartupPluginRecord>,
    /// Config resolver injection handles for all loaded plugin cdylibs.
    /// Stored as raw fn ptrs because `LoadedPlugin` is consumed during registration.
    config_resolver_injectors: Vec<ConfigResolverInjector>,
}

/// Saved config resolver injection handle for a single loaded plugin cdylib.
struct ConfigResolverInjector {
    set_fn: extern "C" fn(*mut c_void, ConfigResolverFn),
}

// Debug impl for ConfigResolverInjector
impl std::fmt::Debug for ConfigResolverInjector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigResolverInjector").finish()
    }
}

impl PluginLoadStats {
    /// Inject a config value resolver callback into all loaded plugins.
    ///
    /// Must be called after the secret store is created and before any
    /// source/reaction creation calls, so the plugins' `DtoMapper` can
    /// resolve `ConfigValue::Secret` references through the host.
    pub fn inject_config_resolver_into_all(&self, ctx: *mut c_void, callback: ConfigResolverFn) {
        for injector in &self.config_resolver_injectors {
            (injector.set_fn)(ctx, callback);
        }
    }
}

/// Information about a single plugin loaded at startup.
///
/// Used by the orchestrator to create PluginInfo records.
#[derive(Debug, Clone)]
pub struct StartupPluginRecord {
    pub plugin_id: String,
    pub file_path: PathBuf,
    pub kinds: Vec<PluginKindEntry>,
    pub plugin_version: String,
    pub sdk_version: String,
}

/// Load cdylib plugins from a directory and register their descriptors.
///
/// Uses the Drasi Host SDK to scan, load, validate, and wire plugins.
/// When a `callback_context` is provided, plugin logs and lifecycle events
/// are routed into DrasiLib's ComponentLogRegistry and ComponentEventHistory,
/// making them visible through the REST API.
///
/// When `allowed_files` is `Some`, only plugins whose filename matches the
/// allowlist will be loaded. This is used when `--skip-verification` is NOT set
/// to ensure only verified plugins are loaded.
pub fn load_plugins(
    dir: &Path,
    registry: &mut PluginRegistry,
    callback_context: Option<Arc<CallbackContext>>,
    allowed_files: Option<&std::collections::HashSet<String>>,
) -> Result<PluginLoadStats> {
    if !dir.exists() {
        debug!("cdylib plugin directory does not exist: {}", dir.display());
        return Ok(PluginLoadStats::default());
    }

    info!("Loading cdylib plugins from: {}", dir.display());

    // Count shared-library files in the directory that are not matched by any
    // plugin file pattern so callers can distinguish an empty directory from
    // one that contains unrecognised libraries.
    let skipped_count = count_unmatched_shared_libs(dir, PLUGIN_FILE_PATTERNS);
    if skipped_count > 0 {
        warn!(
            "{skipped_count} shared-library file(s) in '{}' did not match any plugin file pattern and will not be loaded",
            dir.display()
        );
    }

    let config = if let Some(allowed) = allowed_files {
        // When an allowlist is provided, only load verified plugins.
        // Warn about any plugin files on disk that are being skipped.
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_plugin = PLUGIN_FILE_PATTERNS
                    .iter()
                    .any(|pat| matches_glob(pat, &name));
                if is_plugin && !allowed.contains(&name) {
                    warn!("Skipping unverified plugin: {name} (plugin verification is enabled)",);
                }
            }
        }
        PluginLoaderConfig {
            plugin_dir: dir.to_path_buf(),
            file_patterns: allowed.iter().cloned().collect(),
        }
    } else {
        PluginLoaderConfig {
            plugin_dir: dir.to_path_buf(),
            file_patterns: PLUGIN_FILE_PATTERNS.iter().map(|s| s.to_string()).collect(),
        }
    };

    let loader = PluginLoader::new(config);

    // Build context pointer for callbacks (null if no context provided)
    let ctx_ptr = callback_context
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut());

    let loaded = loader.load_all(
        ctx_ptr,
        callbacks::default_log_callback_fn(),
        ctx_ptr,
        callbacks::default_lifecycle_callback_fn(),
    )?;

    let mut stats = PluginLoadStats {
        plugins_skipped: skipped_count,
        ..PluginLoadStats::default()
    };

    for mut plugin in loaded {
        let meta = plugin.metadata_info.as_deref().unwrap_or("no metadata");

        // Parse version info from metadata string (format: "sdk=X core=Y plugin=Z target=...")
        let plugin_version = meta
            .split_whitespace()
            .find(|s| s.starts_with("plugin="))
            .and_then(|s| s.strip_prefix("plugin="))
            .unwrap_or("")
            .to_string();
        let sdk_version = meta
            .split_whitespace()
            .find(|s| s.starts_with("sdk="))
            .and_then(|s| s.strip_prefix("sdk="))
            .unwrap_or("")
            .to_string();

        let mut plugin_kinds = Vec::new();

        // Save the config resolver injection handle before consuming the plugin.
        // This allows the server to inject a config resolver callback later
        // (after the secret store is created) so plugins can resolve secrets.
        stats
            .config_resolver_injectors
            .push(ConfigResolverInjector {
                set_fn: plugin.config_resolver_injection_fn(),
            });

        // Derive a plugin_id from the first descriptor kind.
        // This mirrors how the lifecycle manager groups descriptors by plugin.
        let mut plugin_id_parts: Vec<String> = Vec::new();

        for proxy in std::mem::take(&mut plugin.source_plugins) {
            let kind = proxy.kind().to_string();
            if plugin_id_parts.is_empty() {
                plugin_id_parts.push(format!("source/{kind}"));
            }
            info!("  [cdylib] source: {kind} ({meta})");
            plugin_kinds.push(PluginKindEntry {
                category: PluginCategory::Source,
                kind: kind.clone(),
                config_version: proxy.config_version().to_string(),
                config_schema_name: proxy.config_schema_name().to_string(),
            });
            registry.register_source_with_metadata(Arc::new(proxy), &plugin_id_parts[0]);
            stats.source_descriptors += 1;
        }

        for proxy in std::mem::take(&mut plugin.reaction_plugins) {
            let kind = proxy.kind().to_string();
            if plugin_id_parts.is_empty() {
                plugin_id_parts.push(format!("reaction/{kind}"));
            }
            info!("  [cdylib] reaction: {kind} ({meta})");
            plugin_kinds.push(PluginKindEntry {
                category: PluginCategory::Reaction,
                kind: kind.clone(),
                config_version: proxy.config_version().to_string(),
                config_schema_name: proxy.config_schema_name().to_string(),
            });
            registry.register_reaction_with_metadata(Arc::new(proxy), &plugin_id_parts[0]);
            stats.reaction_descriptors += 1;
        }

        for proxy in std::mem::take(&mut plugin.bootstrap_plugins) {
            let kind = proxy.kind().to_string();
            if plugin_id_parts.is_empty() {
                plugin_id_parts.push(format!("bootstrap/{kind}"));
            }
            info!("  [cdylib] bootstrap: {kind} ({meta})");
            plugin_kinds.push(PluginKindEntry {
                category: PluginCategory::Bootstrap,
                kind: kind.clone(),
                config_version: proxy.config_version().to_string(),
                config_schema_name: proxy.config_schema_name().to_string(),
            });
            registry.register_bootstrapper_with_metadata(Arc::new(proxy), &plugin_id_parts[0]);
            stats.bootstrap_descriptors += 1;
        }

        for proxy in std::mem::take(&mut plugin.secret_store_plugins) {
            let kind = proxy.kind().to_string();
            if plugin_id_parts.is_empty() {
                plugin_id_parts.push(format!("secret_store/{kind}"));
            }
            info!("  [cdylib] secret_store: {kind} ({meta})");
            plugin_kinds.push(PluginKindEntry {
                category: PluginCategory::SecretStore,
                kind: kind.clone(),
                config_version: proxy.config_version().to_string(),
                config_schema_name: proxy.config_schema_name().to_string(),
            });
            registry.register_secret_store_with_metadata(Arc::new(proxy), &plugin_id_parts[0]);
            stats.secret_store_descriptors += 1;
        }

        for proxy in std::mem::take(&mut plugin.identity_provider_plugins) {
            let kind = proxy.kind().to_string();
            if plugin_id_parts.is_empty() {
                plugin_id_parts.push(format!("identity/{kind}"));
            }
            info!("  [cdylib] identity: {kind} ({meta})");
            plugin_kinds.push(PluginKindEntry {
                category: PluginCategory::IdentityProvider,
                kind: kind.clone(),
                config_version: proxy.config_version().to_string(),
                config_schema_name: proxy.config_schema_name().to_string(),
            });
            registry.register_identity_provider_with_metadata(Arc::new(proxy), &plugin_id_parts[0]);
            stats.identity_provider_descriptors += 1;
        }

        let derived_plugin_id = plugin_id_parts
            .into_iter()
            .next()
            .unwrap_or_else(|| "unknown".to_string());

        stats.loaded_plugins.push(StartupPluginRecord {
            plugin_id: derived_plugin_id,
            file_path: plugin.file_path.clone(),
            kinds: plugin_kinds,
            plugin_version,
            sdk_version,
        });

        stats.plugins_loaded += 1;
    }

    let total_descriptors = stats.source_descriptors
        + stats.reaction_descriptors
        + stats.bootstrap_descriptors
        + stats.secret_store_descriptors
        + stats.identity_provider_descriptors;

    if stats.plugins_loaded > 0 {
        info!(
            "cdylib plugin loading complete: {} loaded, {} skipped, {} descriptors ({} sources, {} reactions, {} bootstraps, {} secret_stores, {} identity providers)",
            stats.plugins_loaded,
            stats.plugins_skipped,
            total_descriptors,
            stats.source_descriptors,
            stats.reaction_descriptors,
            stats.bootstrap_descriptors,
            stats.secret_store_descriptors,
            stats.identity_provider_descriptors,
        );
    } else {
        debug!("No cdylib plugins found in '{}'", dir.display());
    }

    Ok(stats)
}

/// Returns the number of shared-library files in `dir` whose names do not
/// match any of the given `patterns`.
///
/// A "shared library" is any regular file whose name ends with `.so`, `.dylib`,
/// or `.dll` (case-sensitive, matching the file-system conventions on each
/// platform).  Only filenames that look like shared libraries are considered —
/// other directory entries (directories, README files, etc.) are ignored.
fn count_unmatched_shared_libs(dir: &Path, patterns: &[&str]) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            is_shared_lib(&name) && !patterns.iter().any(|pat| matches_glob(pat, &name))
        })
        .count()
}

/// Returns `true` if `name` looks like a shared-library filename.
fn is_shared_lib(name: &str) -> bool {
    name.ends_with(".so") || name.contains(".so.") || name.ends_with(".dylib") || name.ends_with(".dll")
}

/// Simple glob pattern matching for plugin file patterns (e.g., `libdrasi_source_*`).
fn matches_glob(pattern: &str, name: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── matches_glob ────────────────────────────────────────────────────────

    #[test]
    fn matches_glob_prefix_wildcard() {
        assert!(matches_glob("libdrasi_*", "libdrasi_source_mock.so"));
        assert!(matches_glob("libdrasi_*", "libdrasi_reaction_log.so"));
        assert!(matches_glob("libdrasi_*", "libdrasi_secret-store_file.so"));
        assert!(matches_glob("drasi_*", "drasi_source_mock.dll"));
    }

    #[test]
    fn matches_glob_prefix_wildcard_no_false_positives() {
        assert!(!matches_glob("libdrasi_*", "notdrasi_source_mock.so"));
        assert!(!matches_glob("drasi_*", "libdrasi_source_mock.so"));
    }

    #[test]
    fn matches_glob_exact() {
        assert!(matches_glob("exact_name.so", "exact_name.so"));
        assert!(!matches_glob("exact_name.so", "other_name.so"));
    }

    // ── is_shared_lib ───────────────────────────────────────────────────────

    #[test]
    fn is_shared_lib_recognises_platforms() {
        assert!(is_shared_lib("libfoo.so"));
        assert!(is_shared_lib("libfoo.so.1"));
        assert!(is_shared_lib("libfoo.so.1.2.3"));
        assert!(is_shared_lib("libfoo.dylib"));
        assert!(is_shared_lib("foo.dll"));
    }

    #[test]
    fn is_shared_lib_ignores_other_files() {
        assert!(!is_shared_lib("README.md"));
        assert!(!is_shared_lib("plugin.yaml"));
        assert!(!is_shared_lib("libfoo.a"));
        assert!(!is_shared_lib("libfoo.rlib"));
    }

    // ── PLUGIN_FILE_PATTERNS covers all known plugin types ──────────────────

    #[test]
    fn plugin_file_patterns_cover_all_known_types() {
        let known_files = [
            // underscored type names
            "libdrasi_source_mock.so",
            "libdrasi_reaction_log.so",
            "libdrasi_bootstrap_postgres.so",
            "libdrasi_secret_store_file.so",
            "libdrasi_identity_provider_oidc.so",
            // hyphenated type names (OCI registry convention)
            "libdrasi_secret-store_file.so",
            // Windows / no lib prefix
            "drasi_source_mock.dll",
            "drasi_reaction_log.dll",
            "drasi_bootstrap_postgres.dll",
            "drasi_secret_store_file.dll",
            "drasi_identity_provider_oidc.dll",
        ];
        for file in &known_files {
            let matched = PLUGIN_FILE_PATTERNS
                .iter()
                .any(|pat| matches_glob(pat, file));
            assert!(matched, "PLUGIN_FILE_PATTERNS did not match {file}");
        }
    }

    // ── count_unmatched_shared_libs ─────────────────────────────────────────

    #[test]
    fn count_unmatched_shared_libs_counts_unrecognised_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();

        // A drasi plugin — should be matched and NOT counted as skipped.
        std::fs::write(p.join("libdrasi_source_mock.so"), b"").unwrap();
        // A non-drasi shared library — should be counted as skipped.
        std::fs::write(p.join("libsomething_else.so"), b"").unwrap();
        // A non-library file — should be ignored entirely.
        std::fs::write(p.join("README.md"), b"").unwrap();

        let skipped = count_unmatched_shared_libs(p, PLUGIN_FILE_PATTERNS);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn count_unmatched_shared_libs_returns_zero_when_all_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(p.join("libdrasi_source_mock.so"), b"").unwrap();
        std::fs::write(p.join("libdrasi_reaction_log.so"), b"").unwrap();

        let skipped = count_unmatched_shared_libs(p, PLUGIN_FILE_PATTERNS);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn count_unmatched_shared_libs_returns_zero_for_missing_dir() {
        let skipped = count_unmatched_shared_libs(
            std::path::Path::new("/nonexistent/path"),
            PLUGIN_FILE_PATTERNS,
        );
        assert_eq!(skipped, 0);
    }
}
