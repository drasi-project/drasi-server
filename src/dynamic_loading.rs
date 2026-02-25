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

//! Dynamic plugin loading from shared libraries.
//!
//! This module scans a directory for plugin shared libraries (`.so`, `.dylib`,
//! `.dll`) and loads them at runtime, resolving a well-known entry point symbol
//! to obtain [`PluginRegistration`] instances.
//!
//! # Loading Sequence
//!
//! 1. **Load shared runtime** (Unix only): The `libdrasi_plugin_runtime.{so,dylib}`
//!    is loaded with `RTLD_GLOBAL` so its symbols (tokio, serde, etc.) are
//!    available to all subsequently loaded plugins. On Windows, this is handled
//!    automatically via import libraries.
//!
//! 2. **Scan plugin directory**: By default, the directory containing the server
//!    binary is scanned. Files matching `libdrasi_source_*`, `libdrasi_reaction_*`,
//!    or `libdrasi_bootstrap_*` are loaded as plugins.
//!
//! 3. **Build hash check**: Before calling `plugin_init()`, the server checks
//!    `plugin_build_hash()` to verify the plugin was built with the same compiler
//!    and runtime version. This prevents UB from ABI mismatches.
//!
//! 4. **Registration**: The `plugin_init()` entry point returns a
//!    `PluginRegistration` containing descriptor trait objects that are registered
//!    in the plugin registry.
//!
//! # Plugin Entry Point Convention
//!
//! Each plugin shared library must export a function named
//! `drasi_<crate_name>_plugin_init` (with hyphens replaced by underscores)
//! matching the crate name. For example, the `drasi-source-mock` crate exports:
//!
//! ```rust,ignore
//! #[no_mangle]
//! pub extern "C" fn drasi_source_mock_plugin_init() -> *mut PluginRegistration {
//!     let registration = PluginRegistration::new()
//!         .with_source(Box::new(MySourceDescriptor));
//!     Box::into_raw(Box::new(registration))
//! }
//! ```
//!
//! # Safety
//!
//! Dynamic loading relies on the plugin being compiled with the same Rust
//! toolchain and SDK version as the server. The build hash pre-check catches
//! mismatches before any Rust types cross the boundary.

use crate::plugin_registry::PluginRegistry;
use anyhow::{Context, Result};
use drasi_plugin_sdk::PluginRegistration;
use libloading::{Library, Symbol};
use log::{debug, error, info, warn};
use std::path::{Path, PathBuf};

/// Suffix appended to the crate name to form the entry point symbol.
/// Each plugin exports `drasi_<crate_name>_plugin_init` (with hyphens as underscores).
const PLUGIN_INIT_SUFFIX: &str = "_plugin_init";

/// Suffix for the build hash symbol exported by each plugin.
const BUILD_HASH_SUFFIX: &str = "_build_hash";

/// File extensions recognized as plugin shared libraries.
#[cfg(target_os = "linux")]
const PLUGIN_EXTENSIONS: &[&str] = &["so"];

#[cfg(target_os = "macos")]
const PLUGIN_EXTENSIONS: &[&str] = &["dylib"];

#[cfg(target_os = "windows")]
const PLUGIN_EXTENSIONS: &[&str] = &["dll"];

/// Prefixes that identify a file as a plugin (vs runtime or other libs).
const PLUGIN_PREFIXES: &[&str] = &[
    "libdrasi_source_",
    "libdrasi_reaction_",
    "libdrasi_bootstrap_",
    // Windows doesn't use "lib" prefix
    "drasi_source_",
    "drasi_reaction_",
    "drasi_bootstrap_",
];

/// Statistics from a plugin loading operation.
#[derive(Debug, Default)]
pub struct LoadStats {
    /// Number of plugin files found.
    pub found: usize,
    /// Number of plugins successfully loaded.
    pub loaded: usize,
    /// Number of plugins that failed to load.
    pub failed: usize,
    /// Total descriptors registered from loaded plugins.
    pub descriptors: usize,
}

/// Loaded plugin library handle.
///
/// Keeps the [`Library`] alive for the lifetime of the server. Dropping this
/// will unload the shared library, invalidating any descriptors obtained from it.
pub struct LoadedPlugin {
    /// Path to the shared library file.
    pub path: String,
    /// The library handle — must be kept alive as long as descriptors are in use.
    #[allow(dead_code)]
    library: Library,
}

/// Handle for the loaded shared runtime library.
///
/// Must be kept alive for the entire server lifetime since plugins depend on it.
#[allow(dead_code)]
pub struct RuntimeHandle {
    #[cfg(unix)]
    library: libloading::os::unix::Library,
    #[cfg(windows)]
    _marker: (),
}

/// Load the shared runtime library (`libdrasi_plugin_runtime`).
///
/// On Unix, this loads the runtime with `RTLD_GLOBAL` so its symbols are
/// available to all subsequently loaded plugin libraries.
///
/// On Windows, this is a no-op — Windows resolves runtime symbols automatically
/// via import libraries when plugins are loaded.
///
/// Must be called **before** loading any plugins.
pub fn load_shared_runtime(dir: &Path) -> Result<RuntimeHandle> {
    #[cfg(unix)]
    {
        let runtime_name = if cfg!(target_os = "macos") {
            "libdrasi_plugin_runtime.dylib"
        } else {
            "libdrasi_plugin_runtime.so"
        };

        let runtime_path = dir.join(runtime_name);
        if !runtime_path.exists() {
            anyhow::bail!(
                "Shared runtime not found at '{}'. \
                 Ensure drasi-plugin-runtime is built as a dylib.",
                runtime_path.display()
            );
        }

        info!("Loading shared runtime: {}", runtime_path.display());
        let library = unsafe {
            let flags = libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL;
            libloading::os::unix::Library::open(Some(&runtime_path), flags)
                .with_context(|| {
                    format!("Failed to load shared runtime: {}", runtime_path.display())
                })?
        };
        info!("Shared runtime loaded (RTLD_GLOBAL)");
        Ok(RuntimeHandle { library })
    }

    #[cfg(windows)]
    {
        let runtime_path = dir.join("drasi_plugin_runtime.dll");
        if !runtime_path.exists() {
            anyhow::bail!(
                "Shared runtime not found at '{}'. \
                 Ensure drasi-plugin-runtime is built as a dylib.",
                runtime_path.display()
            );
        }
        info!(
            "Shared runtime found at '{}' (Windows: loaded implicitly via import libraries)",
            runtime_path.display()
        );
        Ok(RuntimeHandle { _marker: () })
    }
}

/// Returns the default plugin directory: the directory containing the server binary.
pub fn default_plugin_dir() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .context("Failed to determine server binary path")?;
    let dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Server binary has no parent directory"))?;
    Ok(dir.to_path_buf())
}

/// Load all plugin shared libraries from a directory and register them.
///
/// Scans `dir` for files with platform-appropriate extensions (`.so`, `.dylib`,
/// `.dll`), loads each one, resolves the `drasi_plugin_init` symbol, and
/// registers the returned descriptors into the [`PluginRegistry`].
///
/// Plugins that fail to load are logged and skipped — they do not prevent other
/// plugins from loading.
///
/// Returns a tuple of [`LoadStats`] and the list of [`LoadedPlugin`] handles.
/// The caller must keep the `LoadedPlugin` handles alive for the lifetime of the
/// server to prevent the shared libraries from being unloaded.
pub fn load_plugins_from_directory(
    dir: &str,
    registry: &mut PluginRegistry,
) -> Result<(LoadStats, Vec<LoadedPlugin>)> {
    let dir_path = Path::new(dir);

    if !dir_path.exists() {
        debug!("Plugins directory '{}' does not exist, skipping", dir);
        return Ok((LoadStats::default(), Vec::new()));
    }

    if !dir_path.is_dir() {
        warn!("Plugins path '{}' is not a directory, skipping", dir);
        return Ok((LoadStats::default(), Vec::new()));
    }

    info!("Loading dynamic plugins from: {}", dir);

    let mut stats = LoadStats::default();
    let mut loaded_plugins = Vec::new();

    let entries = std::fs::read_dir(dir_path)
        .with_context(|| format!("Failed to read plugins directory: {dir}"))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to read directory entry: {}", err);
                continue;
            }
        };

        let path = entry.path();

        // Skip non-files
        if !path.is_file() {
            continue;
        }

        // Check file extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !PLUGIN_EXTENSIONS.contains(&ext) {
            continue;
        }

        // Skip non-plugin shared libraries (runtime, std, serde, etc.)
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !PLUGIN_PREFIXES.iter().any(|prefix| file_stem.starts_with(prefix)) {
            debug!("Skipping non-plugin library: {}", path.display());
            continue;
        }

        stats.found += 1;
        let path_str = path.display().to_string();

        match load_single_plugin(&path, registry) {
            Ok(loaded) => {
                info!(
                    "  [dynamic] loaded: {} ({} descriptors)",
                    path_str, loaded.descriptor_count
                );
                stats.loaded += 1;
                stats.descriptors += loaded.descriptor_count;
                loaded_plugins.push(loaded.handle);
            }
            Err(err) => {
                error!("Failed to load plugin '{}': {:#}", path_str, err);
                stats.failed += 1;
            }
        }
    }

    if stats.found > 0 {
        info!(
            "Dynamic plugin loading complete: {} found, {} loaded, {} failed, {} descriptors",
            stats.found, stats.loaded, stats.failed, stats.descriptors
        );
    } else {
        debug!("No plugin files found in '{}'", dir);
    }

    Ok((stats, loaded_plugins))
}

/// Result of loading a single plugin.
struct SingleLoadResult {
    handle: LoadedPlugin,
    descriptor_count: usize,
}

/// Load a single plugin shared library and register its descriptors.
fn load_single_plugin(
    path: &Path,
    registry: &mut PluginRegistry,
) -> Result<SingleLoadResult> {
    let path_str = path.display().to_string();

    // Derive the entry point symbol name from the filename.
    // e.g., "libdrasi_source_mock.so" -> "drasi_source_mock_plugin_init"
    let symbol_name = derive_symbol_name(path)
        .with_context(|| format!("Cannot derive plugin symbol name from: {path_str}"))?;

    // SAFETY: Loading a shared library is inherently unsafe. We require that:
    // 1. The plugin is compiled with the same Rust toolchain version
    // 2. The plugin uses the same drasi-plugin-sdk version
    // 3. The plugin exports the expected symbol with the correct signature
    let library = unsafe {
        Library::new(path)
            .with_context(|| format!("Failed to open shared library: {path_str}"))?
    };

    // Pre-check: verify build hash BEFORE calling plugin_init().
    // This uses a simple extern "C" fn() -> *const u8 that doesn't involve
    // any complex Rust types, making it safe even with ABI mismatches.
    let hash_symbol_name = derive_build_hash_symbol_name(path)
        .with_context(|| format!("Cannot derive build hash symbol name from: {path_str}"))?;

    let server_hash = drasi_plugin_runtime::BUILD_HASH;
    match unsafe { library.get::<unsafe extern "C" fn() -> *const u8>(hash_symbol_name.as_bytes()) }
    {
        Ok(hash_fn) => {
            let plugin_hash_ptr = unsafe { hash_fn() };
            if !plugin_hash_ptr.is_null() {
                let plugin_hash = unsafe { std::ffi::CStr::from_ptr(plugin_hash_ptr as *const i8) }
                    .to_str()
                    .unwrap_or("invalid");
                if plugin_hash != server_hash {
                    anyhow::bail!(
                        "Plugin '{}' build hash mismatch: plugin={}, server={}. \
                         Both must be built with the same Rust toolchain and drasi-plugin-runtime.",
                        path_str,
                        plugin_hash,
                        server_hash,
                    );
                }
                debug!("Build hash verified for '{}'", path_str);
            }
        }
        Err(_) => {
            // Plugin doesn't export build hash — skip the pre-check but warn
            warn!(
                "Plugin '{}' does not export '{}', skipping build hash check",
                path_str, hash_symbol_name
            );
        }
    }

    // Resolve the entry point symbol
    let init_fn: Symbol<unsafe extern "C" fn() -> *mut PluginRegistration> = unsafe {
        library
            .get(symbol_name.as_bytes())
            .with_context(|| {
                format!(
                    "Plugin '{}' does not export '{}' symbol",
                    path_str, symbol_name
                )
            })?
    };

    // Call the entry point to get the registration
    // SAFETY: We trust that the plugin was compiled correctly and the function
    // returns a valid Box<PluginRegistration> via Box::into_raw.
    let registration_ptr = unsafe { init_fn() };

    if registration_ptr.is_null() {
        anyhow::bail!("Plugin '{}' returned null from drasi_plugin_init", path_str);
    }

    // Take ownership of the registration
    // SAFETY: The pointer was created by Box::into_raw in the plugin
    let registration = unsafe { Box::from_raw(registration_ptr) };

    // Validate SDK version compatibility
    let plugin_sdk_version = registration.sdk_version;
    let server_sdk_version = drasi_plugin_sdk::SDK_VERSION;

    if plugin_sdk_version != server_sdk_version {
        anyhow::bail!(
            "Plugin '{}' SDK version mismatch: plugin={}, server={}. \
             Both must use the same drasi-plugin-sdk version.",
            path_str,
            plugin_sdk_version,
            server_sdk_version,
        );
    }

    // Validate Rust compiler version compatibility
    let server_rustc = env!("DRASI_RUSTC_VERSION");
    if registration.rust_version != server_rustc {
        anyhow::bail!(
            "Plugin '{}' Rust version mismatch: plugin='{}', server='{}'. \
             Both must be compiled with the same Rust toolchain.",
            path_str,
            registration.rust_version,
            server_rustc,
        );
    }

    // Validate Tokio version compatibility
    let server_tokio = drasi_plugin_runtime::TOKIO_VERSION;
    if registration.tokio_version != server_tokio {
        anyhow::bail!(
            "Plugin '{}' Tokio version mismatch: plugin={}, server={}. \
             Both must use the same Tokio version via drasi-plugin-runtime.",
            path_str,
            registration.tokio_version,
            server_tokio,
        );
    }

    // Validate build hash (redundant with pre-check but covers edge cases)
    if registration.build_hash != server_hash {
        anyhow::bail!(
            "Plugin '{}' build hash mismatch (from registration): plugin={}, server={}.",
            path_str,
            registration.build_hash,
            server_hash,
        );
    }

    let descriptor_count = registration.descriptor_count();

    if descriptor_count == 0 {
        warn!(
            "Plugin '{}' registered no descriptors (empty registration)",
            path_str,
        );
    }

    // Log individual descriptors
    for s in &registration.sources {
        info!("  [dynamic] source: {} (from {})", s.kind(), path_str);
    }
    for r in &registration.reactions {
        info!("  [dynamic] reaction: {} (from {})", r.kind(), path_str);
    }
    for b in &registration.bootstrappers {
        info!("  [dynamic] bootstrap: {} (from {})", b.kind(), path_str);
    }

    // Register all descriptors
    registry.register_all(*registration);

    Ok(SingleLoadResult {
        handle: LoadedPlugin {
            path: path_str,
            library,
        },
        descriptor_count,
    })
}

/// Derive the plugin init symbol name from the shared library filename.
///
/// Convention: `lib<crate_name>.so` → `<crate_name>_plugin_init`
///
/// Examples:
/// - `libdrasi_source_mock.so` → `drasi_source_mock_plugin_init`
/// - `libdrasi_reaction_http.so` → `drasi_reaction_http_plugin_init`
fn derive_symbol_name(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

    // Strip the "lib" prefix (standard on Unix)
    let crate_name = stem.strip_prefix("lib").unwrap_or(stem);

    Ok(format!("{crate_name}{PLUGIN_INIT_SUFFIX}"))
}

/// Derive the build hash symbol name from the shared library filename.
///
/// Convention: `lib<crate_name>.so` → `<crate_name>_build_hash`
fn derive_build_hash_symbol_name(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

    let crate_name = stem.strip_prefix("lib").unwrap_or(stem);

    Ok(format!("{crate_name}{BUILD_HASH_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_nonexistent_directory() {
        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory("/nonexistent/path", &mut registry).unwrap();
        assert_eq!(stats.found, 0);
        assert_eq!(stats.loaded, 0);
        assert_eq!(stats.failed, 0);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_load_from_empty_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory(temp_dir.path().to_str().unwrap(), &mut registry).unwrap();
        assert_eq!(stats.found, 0);
        assert_eq!(stats.loaded, 0);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_load_skips_non_library_files() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        // Create non-library files
        std::fs::write(temp_dir.path().join("readme.txt"), "not a plugin").unwrap();
        std::fs::write(temp_dir.path().join("config.yaml"), "key: value").unwrap();

        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory(temp_dir.path().to_str().unwrap(), &mut registry).unwrap();
        assert_eq!(stats.found, 0);
        assert_eq!(stats.loaded, 0);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_load_handles_invalid_library() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        // Create a fake .so file that isn't a valid shared library
        // Must match PLUGIN_PREFIXES to pass the filename filter
        let ext = PLUGIN_EXTENSIONS[0];
        std::fs::write(
            temp_dir.path().join(format!("libdrasi_source_bad.{ext}")),
            "not a real shared library",
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        let (stats, _handles) =
            load_plugins_from_directory(temp_dir.path().to_str().unwrap(), &mut registry).unwrap();
        assert_eq!(stats.found, 1);
        assert_eq!(stats.loaded, 0);
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn test_load_from_file_path_not_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not_a_dir.txt");
        std::fs::write(&file_path, "content").unwrap();

        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory(file_path.to_str().unwrap(), &mut registry).unwrap();
        assert_eq!(stats.found, 0);
        assert_eq!(stats.loaded, 0);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_load_stats_default() {
        let stats = LoadStats::default();
        assert_eq!(stats.found, 0);
        assert_eq!(stats.loaded, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.descriptors, 0);
    }
}
