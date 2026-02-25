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
//! Each plugin shared library must export a common `drasi_plugin_init` function:
//!
//! ```rust,ignore
//! #[no_mangle]
//! pub extern "C" fn drasi_plugin_init() -> *mut PluginRegistration {
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
/// All plugins export a common `drasi_plugin_init` symbol.
const PLUGIN_INIT_SYMBOL: &str = "drasi_plugin_init";

/// The build hash symbol exported by each plugin (optional pre-check).
const BUILD_HASH_SYMBOL: &str = "drasi_plugin_build_hash";

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
    /// Pre-loaded transitive dependencies (e.g. libstd) — must stay alive.
    #[cfg(unix)]
    preloaded: Vec<libloading::os::unix::Library>,
    #[cfg(windows)]
    _marker: (),
}

/// Load the shared runtime library (`libdrasi_plugin_runtime`).
///
/// On Unix, this first pre-loads `libstd-*.so` with `RTLD_GLOBAL` (the dynamic
/// linker caches `LD_LIBRARY_PATH` at process startup, so `setenv` after that
/// point has no effect on `dlopen`). Then loads the runtime itself with
/// `RTLD_GLOBAL` so its symbols are available to all subsequently loaded plugins.
///
/// On Windows, this is a no-op — Windows resolves runtime symbols automatically
/// via import libraries when plugins are loaded.
///
/// Must be called **before** loading any plugins.
pub fn load_shared_runtime(dir: &Path) -> Result<RuntimeHandle> {
    #[cfg(unix)]
    {
        // Pre-load libstd with RTLD_GLOBAL. The dynamic linker caches library
        // search paths at process startup, so setting LD_LIBRARY_PATH later has
        // no effect. Instead we explicitly load libstd from the Rust sysroot (or
        // from the plugins directory if it's been copied there for deployment).
        let preloaded = preload_rust_std(dir);

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
        Ok(RuntimeHandle { library, preloaded })
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

    // Collect candidate plugin paths
    let mut plugin_paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to read directory entry: {}", err);
                continue;
            }
        };

        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !PLUGIN_EXTENSIONS.contains(&ext) {
            continue;
        }

        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !PLUGIN_PREFIXES.iter().any(|prefix| file_stem.starts_with(prefix)) {
            debug!("Skipping non-plugin library: {}", path.display());
            continue;
        }

        plugin_paths.push(path);
    }

    stats.found = plugin_paths.len();

    // First pass: try to load all plugins. Some may fail if they depend on
    // other plugin libraries that haven't been loaded yet.
    let mut retry_paths: Vec<PathBuf> = Vec::new();

    for path in &plugin_paths {
        let path_str = path.display().to_string();
        match load_single_plugin(path, registry) {
            Ok(Some(loaded)) => {
                info!(
                    "  [dynamic] loaded: {} ({} descriptors)",
                    path_str, loaded.descriptor_count
                );
                stats.loaded += 1;
                stats.descriptors += loaded.descriptor_count;
                loaded_plugins.push(loaded.handle);
            }
            Ok(None) => {
                // Not a plugin (no drasi_plugin_init symbol) — skip silently
                stats.found -= 1; // Don't count non-plugins
            }
            Err(_) => {
                retry_paths.push(path.clone());
            }
        }
    }

    // Retry pass: plugins that failed may now succeed because their
    // dependencies were loaded (with RTLD_GLOBAL) in the first pass.
    for path in &retry_paths {
        let path_str = path.display().to_string();
        match load_single_plugin(path, registry) {
            Ok(Some(loaded)) => {
                info!(
                    "  [dynamic] loaded (retry): {} ({} descriptors)",
                    path_str, loaded.descriptor_count
                );
                stats.loaded += 1;
                stats.descriptors += loaded.descriptor_count;
                loaded_plugins.push(loaded.handle);
            }
            Ok(None) => {
                // Not a plugin — skip silently
                stats.found -= 1;
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
///
/// Returns `Ok(None)` if the library does not export the plugin init symbol
/// (i.e. it's not a Drasi plugin — just a regular shared library that happens
/// to match the naming convention).
fn load_single_plugin(
    path: &Path,
    registry: &mut PluginRegistry,
) -> Result<Option<SingleLoadResult>> {
    let path_str = path.display().to_string();

    // SAFETY: Loading a shared library is inherently unsafe. We require that:
    // 1. The plugin is compiled with the same Rust toolchain version
    // 2. The plugin uses the same drasi-plugin-sdk version
    // 3. The plugin exports the expected symbol with the correct signature
    //
    // We use RTLD_GLOBAL so that plugins which depend on other plugin crates
    // (e.g. grpc-adaptive depends on grpc) can resolve those symbols.
    // Symbol lookups via Library::get() are still scoped to the specific handle.
    #[cfg(unix)]
    let library = unsafe {
        let flags = libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL;
        let lib = libloading::os::unix::Library::open(Some(path), flags)
            .with_context(|| format!("Failed to open shared library: {path_str}"))?;
        Library::from(lib)
    };

    #[cfg(not(unix))]
    let library = unsafe {
        Library::new(path)
            .with_context(|| format!("Failed to open shared library: {path_str}"))?
    };

    // First, check if this library exports the plugin init symbol.
    // If not, it's not a Drasi plugin — skip it without warnings.
    let has_init = unsafe {
        library
            .get::<unsafe extern "C" fn() -> *mut PluginRegistration>(
                PLUGIN_INIT_SYMBOL.as_bytes(),
            )
            .is_ok()
    };
    if !has_init {
        debug!(
            "Library '{}' does not export '{}', skipping (not a plugin)",
            path_str, PLUGIN_INIT_SYMBOL
        );
        return Ok(None);
    }

    // Pre-check: verify build hash BEFORE calling drasi_plugin_init().
    // This uses a simple extern "C" fn() -> *const u8 that doesn't involve
    // any complex Rust types, making it safe even with ABI mismatches.
    let server_hash = drasi_plugin_runtime::BUILD_HASH;
    match unsafe { library.get::<unsafe extern "C" fn() -> *const u8>(BUILD_HASH_SYMBOL.as_bytes()) }
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
            // Plugin doesn't export build hash symbol — skip the pre-check but warn.
            // The build hash is still verified post-init via registration.build_hash.
            warn!(
                "Plugin '{}' does not export '{}', skipping build hash pre-check",
                path_str, BUILD_HASH_SYMBOL
            );
        }
    }

    // Resolve the entry point symbol (we already verified it exists above)
    let init_fn: Symbol<unsafe extern "C" fn() -> *mut PluginRegistration> = unsafe {
        library
            .get(PLUGIN_INIT_SYMBOL.as_bytes())
            .with_context(|| {
                format!(
                    "Plugin '{}' failed to resolve '{}' symbol",
                    path_str, PLUGIN_INIT_SYMBOL
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

    Ok(Some(SingleLoadResult {
        handle: LoadedPlugin {
            path: path_str,
            library,
        },
        descriptor_count,
    }))
}

/// Pre-load `libstd-*.{so,dylib}` with `RTLD_GLOBAL` so that the shared runtime
/// and plugins can resolve their dependency on Rust's standard library.
///
/// Searches the plugins directory first (for deployment scenarios where libstd is
/// copied alongside plugins), then falls back to the Rust sysroot lib directory
/// embedded at build time.
#[cfg(unix)]
fn preload_rust_std(plugins_dir: &Path) -> Vec<libloading::os::unix::Library> {
    let ext = if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let prefix = "libstd-";
    let mut loaded = Vec::new();

    // Search order: plugins directory, then Rust sysroot
    let search_dirs: Vec<PathBuf> = {
        let mut dirs = vec![plugins_dir.to_path_buf()];
        if let Some(sysroot) = option_env!("DRASI_RUST_LIB_DIR") {
            dirs.push(PathBuf::from(sysroot));
        }
        dirs
    };

    for dir in &search_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) && name.ends_with(ext) {
                let path = entry.path();
                debug!("Pre-loading Rust std library: {}", path.display());
                match unsafe {
                    let flags =
                        libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL;
                    libloading::os::unix::Library::open(Some(&path), flags)
                } {
                    Ok(lib) => {
                        info!("Loaded libstd (RTLD_GLOBAL): {}", path.display());
                        loaded.push(lib);
                        return loaded; // Only one libstd needed
                    }
                    Err(e) => {
                        warn!("Failed to pre-load {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    if loaded.is_empty() {
        warn!(
            "Could not find libstd-*.{ext} in plugins dir or Rust sysroot. \
             Dynamic plugins may fail to load. Set LD_LIBRARY_PATH to include \
             the Rust sysroot lib directory."
        );
    }

    loaded
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

    #[test]
    fn test_plugin_init_symbol_constant() {
        assert_eq!(PLUGIN_INIT_SYMBOL, "drasi_plugin_init");
        assert_eq!(BUILD_HASH_SYMBOL, "drasi_plugin_build_hash");
    }

    #[test]
    fn test_plugin_prefixes_filter_matches() {
        let valid_names = [
            "libdrasi_source_mock",
            "libdrasi_reaction_log",
            "libdrasi_bootstrap_postgres",
            "drasi_source_http",      // Windows (no lib prefix)
            "drasi_reaction_sse",
            "drasi_bootstrap_noop",
        ];
        for name in &valid_names {
            assert!(
                PLUGIN_PREFIXES.iter().any(|prefix| name.starts_with(prefix)),
                "{name} should match PLUGIN_PREFIXES"
            );
        }
    }

    #[test]
    fn test_plugin_prefixes_filter_rejects() {
        let invalid_names = [
            "libdrasi_plugin_runtime",
            "libstd",
            "libdrasi_core",
            "libdrasi_lib",
            "libserde",
            "libtokio",
        ];
        for name in &invalid_names {
            assert!(
                !PLUGIN_PREFIXES.iter().any(|prefix| name.starts_with(prefix)),
                "{name} should NOT match PLUGIN_PREFIXES"
            );
        }
    }

    #[test]
    fn test_default_plugin_dir_returns_something() {
        // default_plugin_dir returns the binary's parent directory
        let dir = default_plugin_dir();
        assert!(dir.is_ok(), "default_plugin_dir should succeed");
        let dir = dir.unwrap();
        assert!(dir.exists(), "plugin dir should exist");
    }

    #[test]
    fn test_load_skips_runtime_library() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let ext = PLUGIN_EXTENSIONS[0];
        // Create a file that looks like the runtime library — should be skipped
        std::fs::write(
            temp_dir
                .path()
                .join(format!("libdrasi_plugin_runtime.{ext}")),
            "not a real library",
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory(temp_dir.path().to_str().unwrap(), &mut registry).unwrap();
        // Runtime library should be skipped by prefix filter (doesn't match PLUGIN_PREFIXES)
        assert_eq!(stats.found, 0);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_load_skips_non_plugin_libraries() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let ext = PLUGIN_EXTENSIONS[0];
        // Create files that are .so but don't match plugin prefixes
        std::fs::write(
            temp_dir.path().join(format!("libserde.{ext}")),
            "not a plugin",
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join(format!("libtokio.{ext}")),
            "not a plugin",
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        let (stats, handles) =
            load_plugins_from_directory(temp_dir.path().to_str().unwrap(), &mut registry).unwrap();
        assert_eq!(stats.found, 0);
        assert!(handles.is_empty());
    }
}
