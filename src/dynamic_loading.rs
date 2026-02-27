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
use drasi_plugin_sdk::{
    BootstrapPluginDescriptor, ReactionPluginDescriptor, SourcePluginDescriptor,
};
use log::{debug, info};
use std::path::Path;
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
}

/// Load cdylib plugins from a directory and register their descriptors.
///
/// Uses the Drasi Host SDK to scan, load, validate, and wire plugins.
/// When a `callback_context` is provided, plugin logs and lifecycle events
/// are routed into DrasiLib's ComponentLogRegistry and ComponentEventHistory,
/// making them visible through the REST API.
pub fn load_plugins(
    dir: &Path,
    registry: &mut PluginRegistry,
    callback_context: Option<Arc<CallbackContext>>,
) -> Result<PluginLoadStats> {
    if !dir.exists() {
        debug!("cdylib plugin directory does not exist: {}", dir.display());
        return Ok(PluginLoadStats::default());
    }

    info!("Loading cdylib plugins from: {}", dir.display());

    let config = PluginLoaderConfig {
        plugin_dir: dir.to_path_buf(),
        file_patterns: PLUGIN_FILE_PATTERNS.iter().map(|s| s.to_string()).collect(),
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

    for plugin in loaded {
        let meta = plugin.metadata_info.as_deref().unwrap_or("no metadata");

        for proxy in plugin.source_plugins {
            let kind = proxy.kind().to_string();
            info!("  [cdylib] source: {} ({})", kind, meta);
            registry.register_source(Arc::new(proxy));
            stats.source_descriptors += 1;
        }

        for proxy in plugin.reaction_plugins {
            let kind = proxy.kind().to_string();
            info!("  [cdylib] reaction: {} ({})", kind, meta);
            registry.register_reaction(Arc::new(proxy));
            stats.reaction_descriptors += 1;
        }

        for proxy in plugin.bootstrap_plugins {
            let kind = proxy.kind().to_string();
            info!("  [cdylib] bootstrap: {} ({})", kind, meta);
            registry.register_bootstrapper(Arc::new(proxy));
            stats.bootstrap_descriptors += 1;
        }

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
