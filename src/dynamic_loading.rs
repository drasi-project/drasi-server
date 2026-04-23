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
use drasi_plugin_sdk::{
    BootstrapPluginDescriptor, ReactionPluginDescriptor, SourcePluginDescriptor,
};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// File patterns for discovering cdylib plugins.
/// Includes both Unix (`lib` prefix) and Windows (no prefix) naming conventions.
const PLUGIN_FILE_PATTERNS: &[&str] = &[
    "libdrasi_source_*",
    "libdrasi_reaction_*",
    "libdrasi_bootstrap_*",
    "drasi_source_*",
    "drasi_reaction_*",
    "drasi_bootstrap_*",
];

/// Statistics from a cdylib plugin loading operation.
#[derive(Debug, Default)]
pub struct PluginLoadStats {
    pub plugins_loaded: usize,
    pub plugins_failed: usize,
    pub source_descriptors: usize,
    pub reaction_descriptors: usize,
    pub bootstrap_descriptors: usize,
    /// Per-plugin information for orchestrator registration.
    pub loaded_plugins: Vec<StartupPluginRecord>,
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

    let mut stats = PluginLoadStats::default();

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

    let total_descriptors =
        stats.source_descriptors + stats.reaction_descriptors + stats.bootstrap_descriptors;

    if stats.plugins_loaded > 0 {
        info!(
            "cdylib plugin loading complete: {} loaded, {} descriptors ({} sources, {} reactions, {} bootstraps)",
            stats.plugins_loaded,
            total_descriptors,
            stats.source_descriptors,
            stats.reaction_descriptors,
            stats.bootstrap_descriptors,
        );
    } else {
        debug!("No cdylib plugins found in '{}'", dir.display());
    }

    Ok(stats)
}

/// Simple glob pattern matching for plugin file patterns (e.g., `libdrasi_source_*`).
fn matches_glob(pattern: &str, name: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}
