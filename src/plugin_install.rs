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

//! Plugin auto-install from OCI registries.
//!
//! When `autoInstallPlugins: true` is set in the server config, missing plugins
//! declared in the `plugins:` list are automatically downloaded from the configured
//! registry before the server starts loading plugins.
//!
//! Supports a lockfile (`plugins.lock`) for reproducible installs.

use crate::config::{DrasiServerConfig, PluginDependency};
use crate::plugin_lockfile::{LockedPlugin, PluginLockfile, PluginSignatureInfo};
use crate::plugin_operations::PluginOperations;
use anyhow::{bail, Context, Result};
use drasi_host_sdk::registry::{
    CosignVerifier, OciRegistryClient, PluginResolver, RegistryConfig, ResolvedPlugin,
    SignatureStatus,
};
use log::{info, warn};
use std::path::Path;

/// Install missing plugins declared in the server configuration.
///
/// For each plugin in `config.plugins`, checks if a matching binary exists
/// in `plugins_dir`. If not, resolves and downloads from the configured registry.
///
/// If `locked` is true, installs must match an existing `plugins.lock` exactly.
/// The lockfile is updated after successful installs (when not in locked mode).
///
/// Returns a list of resolved plugins (both existing and newly downloaded).
pub async fn auto_install_plugins(
    config: &DrasiServerConfig,
    plugins_dir: &Path,
    locked: bool,
) -> Result<Vec<ResolvedPlugin>> {
    use drasi_host_sdk::registry::PluginSourceKind;

    if !config.auto_install_plugins || config.plugins.is_empty() {
        return Ok(Vec::new());
    }

    let registry_url = config
        .plugin_registry
        .as_deref()
        .unwrap_or("ghcr.io/drasi-project");

    info!(
        "Auto-installing {} plugin(s) from {}{}...",
        config.plugins.len(),
        registry_url,
        if locked { " (locked)" } else { "" }
    );

    // Check if the registry is a local directory
    if let PluginSourceKind::LocalDir(dir) = PluginSourceKind::parse(registry_url) {
        return auto_install_from_local_dir(config, plugins_dir, &dir).await;
    }

    // Read existing lockfile
    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();

    if locked && lockfile.plugins.is_empty() {
        bail!("--locked flag used but no plugins.lock file found");
    }

    // Build registry config with auth from environment
    let auth = PluginOperations::registry_auth();
    let registry_config = RegistryConfig {
        default_registry: registry_url.to_string(),
        auth,
    };

    // Always attempt verification during install to record signature info.
    // The verify_plugins flag only controls whether unverified plugins are blocked at load time.
    let mut verification = PluginOperations::verification_config(config);
    verification.enabled = true;

    let client =
        OciRegistryClient::with_verifier(registry_config, CosignVerifier::new(verification));

    // Build host version info from compiled-in dependency versions
    let host_info = PluginOperations::host_version_info();

    let resolver = PluginResolver::new(&client, &host_info);

    // Ensure plugins directory exists
    std::fs::create_dir_all(plugins_dir).context("failed to create plugins directory")?;

    let mut resolved = Vec::new();
    let mut lockfile_updated = false;

    for plugin_dep in &config.plugins {
        match install_if_missing(
            &client,
            &resolver,
            plugin_dep,
            plugins_dir,
            registry_url,
            locked,
            &lockfile,
        )
        .await
        {
            Ok((rp, sig_status)) => {
                // Convert verification status to lockfile signature info
                let sig_info = match sig_status {
                    SignatureStatus::Verified(v) => Some(PluginSignatureInfo {
                        verified: true,
                        issuer: v.issuer,
                        subject: v.subject,
                    }),
                    _ => None,
                };

                // Compute file hash for integrity verification
                let file_hash = {
                    let file_path = plugins_dir.join(&rp.filename);
                    crate::plugin_lockfile::compute_file_hash(&file_path).ok()
                };

                // Update lockfile with resolved info
                let locked_entry = LockedPlugin {
                    reference: rp.reference.clone(),
                    version: rp.version.clone(),
                    digest: rp.digest.clone(),
                    sdk_version: rp.sdk_version.clone(),
                    core_version: rp.core_version.clone(),
                    lib_version: rp.lib_version.clone(),
                    platform: rp.platform.clone(),
                    filename: rp.filename.clone(),
                    file_hash,
                    git_commit: None,
                    build_timestamp: None,
                    signature: sig_info,
                };
                if lockfile.get(&plugin_dep.reference) != Some(&locked_entry) {
                    lockfile.insert(plugin_dep.reference.clone(), locked_entry);
                    lockfile_updated = true;
                }
                resolved.push(rp);
            }
            Err(e) => {
                warn!("Failed to install plugin '{}': {}", plugin_dep.reference, e);
            }
        }
    }

    // Write updated lockfile (only when not in locked mode)
    if lockfile_updated && !locked {
        lockfile.write(lockfile_dir)?;
    }

    if !resolved.is_empty() {
        info!(
            "Plugin auto-install complete: {} plugin(s) ready",
            resolved.len()
        );
    }

    Ok(resolved)
}

/// Install a single plugin if it's not already present.
/// Returns the resolved plugin and optional verification result.
async fn install_if_missing(
    client: &OciRegistryClient,
    resolver: &PluginResolver<'_>,
    dep: &PluginDependency,
    plugins_dir: &Path,
    default_registry: &str,
    locked: bool,
    lockfile: &PluginLockfile,
) -> Result<(ResolvedPlugin, SignatureStatus)> {
    // In locked mode, use the lockfile entry instead of resolving
    if locked {
        let locked_entry = lockfile.get(&dep.reference).with_context(|| {
            format!(
                "plugin '{}' not found in plugins.lock (required by --locked)",
                dep.reference
            )
        })?;

        let resolved = ResolvedPlugin {
            reference: locked_entry.reference.clone(),
            version: locked_entry.version.clone(),
            sdk_version: locked_entry.sdk_version.clone(),
            core_version: locked_entry.core_version.clone(),
            lib_version: locked_entry.lib_version.clone(),
            platform: locked_entry.platform.clone(),
            digest: locked_entry.digest.clone(),
            filename: locked_entry.filename.clone(),
        };

        let dest_path = plugins_dir.join(&resolved.filename);
        if dest_path.exists() {
            // Best-effort verification for existing plugins
            let verification = client
                .verifier()
                .verify_plugin(&resolved.reference, &client.auth())
                .await;
            info!(
                "  ✓ {} v{} — already installed (locked)",
                dep.reference, resolved.version
            );
            return Ok((resolved, verification));
        }

        // Download using the locked digest reference
        info!(
            "  ↓ {} v{} — downloading (locked)...",
            dep.reference, resolved.version
        );

        let download = client
            .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
            .await
            .with_context(|| format!("failed to download '{}'", dep.reference))?;

        info!(
            "  ✓ {} v{} — installed → {}",
            dep.reference, resolved.version, resolved.filename
        );

        return Ok((resolved, download.verification));
    }

    // Normal mode: resolve from registry
    let resolved = resolver
        .resolve(&dep.reference, default_registry)
        .await
        .with_context(|| format!("failed to resolve '{}'", dep.reference))?;

    // Check if binary already exists
    let dest_path = plugins_dir.join(&resolved.filename);
    if dest_path.exists() {
        // Best-effort verification for existing plugins
        let verification = client
            .verifier()
            .verify_plugin(&resolved.reference, &client.auth())
            .await;
        info!(
            "  ✓ {} v{} — already installed",
            dep.reference, resolved.version
        );
        return Ok((resolved, verification));
    }

    // Download the binary
    info!(
        "  ↓ {} v{} ({}) — downloading...",
        dep.reference, resolved.version, resolved.platform
    );

    let download = client
        .download_plugin(&resolved.reference, plugins_dir, &resolved.filename)
        .await
        .with_context(|| format!("failed to download '{}'", dep.reference))?;

    info!(
        "  ✓ {} v{} — installed → {}",
        dep.reference, resolved.version, resolved.filename
    );

    Ok((resolved, download.verification))
}

/// Auto-install plugins from a local directory.
///
/// For each plugin in `config.plugins`, resolves and copies from the local dir.
async fn auto_install_from_local_dir(
    config: &DrasiServerConfig,
    plugins_dir: &Path,
    dir: &Path,
) -> Result<Vec<ResolvedPlugin>> {
    use drasi_host_sdk::registry::LocalDirRegistry;

    let local = LocalDirRegistry::new(dir);

    std::fs::create_dir_all(plugins_dir).context("failed to create plugins directory")?;

    let lockfile_dir = plugins_dir;
    let mut lockfile = PluginLockfile::read(lockfile_dir)?.unwrap_or_default();
    let mut lockfile_updated = false;
    let mut resolved = Vec::new();

    for plugin_dep in &config.plugins {
        match local.resolve(&plugin_dep.reference) {
            Ok(info) => {
                let dest_path = plugins_dir.join(&info.filename);
                if dest_path.exists() {
                    info!("  ✓ {} — already installed (local)", plugin_dep.reference);
                } else {
                    info!(
                        "  ← {} — copying from {}...",
                        plugin_dep.reference,
                        dir.display()
                    );
                    local.install(&info, plugins_dir).with_context(|| {
                        format!(
                            "failed to install '{}' from local dir",
                            plugin_dep.reference
                        )
                    })?;
                    info!(
                        "  ✓ {} — installed → {}",
                        plugin_dep.reference, info.filename
                    );
                }

                let locked_entry = LockedPlugin {
                    reference: format!("file://{}", info.file_path.display()),
                    version: info.version.clone(),
                    digest: String::new(),
                    sdk_version: info.sdk_version.clone(),
                    core_version: String::new(),
                    lib_version: String::new(),
                    platform: env!("TARGET_TRIPLE").to_string(),
                    filename: info.filename.clone(),
                    file_hash: crate::plugin_lockfile::compute_file_hash(
                        &plugins_dir.join(&info.filename),
                    )
                    .ok(),
                    git_commit: None,
                    build_timestamp: None,
                    signature: None,
                };
                if lockfile.get(&plugin_dep.reference) != Some(&locked_entry) {
                    lockfile.insert(plugin_dep.reference.clone(), locked_entry);
                    lockfile_updated = true;
                }

                // Build a ResolvedPlugin for compatibility with callers
                resolved.push(ResolvedPlugin {
                    reference: format!("file://{}", info.file_path.display()),
                    version: info.version,
                    sdk_version: info.sdk_version,
                    core_version: String::new(),
                    lib_version: String::new(),
                    platform: env!("TARGET_TRIPLE").to_string(),
                    digest: String::new(),
                    filename: info.filename,
                });
            }
            Err(e) => {
                warn!(
                    "Failed to install plugin '{}' from local dir: {}",
                    plugin_dep.reference, e
                );
            }
        }
    }

    if lockfile_updated {
        lockfile.write(lockfile_dir)?;
    }

    if !resolved.is_empty() {
        info!(
            "Plugin auto-install from local dir complete: {} plugin(s) ready",
            resolved.len()
        );
    }

    Ok(resolved)
}
