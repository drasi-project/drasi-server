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
use crate::plugin_lockfile::{
    compute_file_hash, LockedPlugin, PluginLockfile, PluginSignatureInfo,
};
use crate::plugin_operations::PluginOperations;
use anyhow::{bail, Context, Result};
use drasi_host_sdk::loader::{plugin_kind_from_filename, scan_plugin_metadata};
use drasi_host_sdk::registry::{
    CosignVerifier, DownloadResult, OciRegistryClient, PluginResolver, RegistryConfig,
    ResolvedPlugin, SignatureStatus,
};
use log::{info, warn};
use std::path::{Path, PathBuf};

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
    let mut install_failures = Vec::new();

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
                install_failures.push(format!("{}: {e}", plugin_dep.reference));
            }
        }
    }

    // Write updated lockfile (only when not in locked mode)
    if lockfile_updated && !locked {
        lockfile.write(lockfile_dir)?;
    }

    if !install_failures.is_empty() {
        bail!(
            "failed to install required plugin(s):\n  - {}",
            install_failures.join("\n  - ")
        );
    }

    if !resolved.is_empty() {
        info!(
            "Plugin auto-install complete: {} plugin(s) ready",
            resolved.len()
        );
    }

    Ok(resolved)
}

fn major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn lock_entry_matches_resolution(entry: &LockedPlugin, resolved: &ResolvedPlugin) -> bool {
    entry.reference == resolved.reference
        && entry.version == resolved.version
        && entry.digest == resolved.digest
        && entry.sdk_version == resolved.sdk_version
        && entry.core_version == resolved.core_version
        && entry.lib_version == resolved.lib_version
        && entry.platform == resolved.platform
        && entry.filename == resolved.filename
}

fn validate_plugin_binary(path: &Path, expected_version: Option<&str>) -> Result<()> {
    let metadata = scan_plugin_metadata(path)
        .with_context(|| format!("could not read embedded metadata from {}", path.display()))?;
    let plugin_abi = major_minor(&metadata.sdk_version).with_context(|| {
        format!(
            "plugin '{}' reports invalid ABI version '{}'",
            path.display(),
            metadata.sdk_version
        )
    })?;
    let host_abi_version = drasi_plugin_sdk::ffi::metadata::FFI_SDK_VERSION;
    let host_abi = major_minor(host_abi_version)
        .with_context(|| format!("server reports invalid ABI version '{host_abi_version}'"))?;

    if plugin_abi != host_abi {
        bail!(
            "plugin ABI mismatch: plugin={}, server={} (major.minor must match)",
            metadata.sdk_version,
            host_abi_version
        );
    }
    if metadata.target_triple != env!("TARGET_TRIPLE") {
        bail!(
            "plugin target mismatch: plugin={}, server={}",
            metadata.target_triple,
            env!("TARGET_TRIPLE")
        );
    }
    if let Some(expected) = expected_version {
        if !expected.is_empty() && metadata.version != expected {
            bail!(
                "plugin version mismatch: cached={}, resolved={}",
                metadata.version,
                expected
            );
        }
    }

    Ok(())
}

pub(crate) fn plugin_compatibility_errors(dir: &Path) -> Vec<(PathBuf, String)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            return vec![(
                dir.to_path_buf(),
                format!("unable to read plugin directory: {error}"),
            )];
        }
    };

    let mut errors = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push((
                    dir.to_path_buf(),
                    format!("unable to read plugin directory entry: {error}"),
                ));
                continue;
            }
        };
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        if plugin_kind_from_filename(&filename).is_none() {
            continue;
        }

        let path = entry.path();
        if let Err(error) = validate_plugin_binary(&path, None) {
            errors.push((path, error.to_string()));
        }
    }
    errors
}

fn validate_cached_plugin(
    path: &Path,
    resolved: &ResolvedPlugin,
    locked_entry: Option<&LockedPlugin>,
) -> Result<()> {
    let entry = locked_entry.context("no matching plugins.lock entry")?;
    if !lock_entry_matches_resolution(entry, resolved) {
        bail!("plugins.lock entry does not match the resolved plugin");
    }

    let expected_hash = entry
        .file_hash
        .as_deref()
        .context("plugins.lock entry has no file hash")?;
    let actual_hash = compute_file_hash(path)?;
    if actual_hash != expected_hash {
        bail!(
            "plugin file hash mismatch: expected {}, got {}",
            expected_hash,
            actual_hash
        );
    }

    validate_plugin_binary(path, Some(&resolved.version))?;

    Ok(())
}

#[cfg(not(windows))]
fn replace_staged_file(staged_path: &Path, destination: &Path, backup: &Path) -> Result<()> {
    let _ = backup;
    std::fs::rename(staged_path, destination).with_context(|| {
        format!(
            "failed to atomically install replacement plugin {}",
            destination.display()
        )
    })
}

#[cfg(windows)]
fn replace_staged_file(staged_path: &Path, destination: &Path, backup: &Path) -> Result<()> {
    let had_destination = destination.exists();
    if had_destination {
        std::fs::rename(destination, backup).with_context(|| {
            format!(
                "failed to stage existing plugin {} for replacement",
                destination.display()
            )
        })?;
    }

    if let Err(error) = std::fs::rename(staged_path, destination) {
        if had_destination {
            let _ = std::fs::rename(backup, destination);
        }
        return Err(error).with_context(|| {
            format!(
                "failed to install replacement plugin {}",
                destination.display()
            )
        });
    }

    if had_destination {
        std::fs::remove_file(backup)
            .with_context(|| format!("failed to remove plugin backup {}", backup.display()))?;
    }
    Ok(())
}

async fn download_plugin_replacing(
    client: &OciRegistryClient,
    reference: &str,
    plugins_dir: &Path,
    filename: &str,
) -> Result<DownloadResult> {
    std::fs::create_dir_all(plugins_dir).context("failed to create plugins directory")?;
    let operation_id = uuid::Uuid::new_v4();
    let staging_dir = plugins_dir.join(format!(".plugin-download-{operation_id}"));
    let backup_path = plugins_dir.join(format!(".plugin-backup-{operation_id}"));

    let download = match client
        .download_plugin(reference, &staging_dir, filename)
        .await
    {
        Ok(download) => download,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(error);
        }
    };

    let destination = plugins_dir.join(filename);
    let replacement = replace_staged_file(&download.path, &destination, &backup_path);
    let _ = std::fs::remove_dir_all(&staging_dir);
    replacement?;

    Ok(DownloadResult {
        path: destination,
        verification: download.verification,
    })
}

fn copy_plugin_replacing(source: &Path, plugins_dir: &Path, filename: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(plugins_dir).context("failed to create plugins directory")?;
    let operation_id = uuid::Uuid::new_v4();
    let staging_path = plugins_dir.join(format!(".plugin-copy-{operation_id}"));
    let backup_path = plugins_dir.join(format!(".plugin-backup-{operation_id}"));
    let destination = plugins_dir.join(filename);

    std::fs::copy(source, &staging_path).with_context(|| {
        format!(
            "failed to stage local plugin {} for installation",
            source.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))?;
    }

    if let Err(error) = replace_staged_file(&staging_path, &destination, &backup_path) {
        let _ = std::fs::remove_file(&staging_path);
        return Err(error);
    }

    Ok(destination)
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
            match validate_cached_plugin(&dest_path, &resolved, Some(locked_entry)) {
                Ok(()) => {
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
                Err(error) => {
                    warn!(
                        "  ↻ {} v{} — replacing invalid cache: {}",
                        dep.reference, resolved.version, error
                    );
                }
            }
        }

        // Download using the locked digest reference
        info!(
            "  ↓ {} v{} — downloading (locked)...",
            dep.reference, resolved.version
        );

        let download =
            download_plugin_replacing(client, &resolved.reference, plugins_dir, &resolved.filename)
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
        match validate_cached_plugin(&dest_path, &resolved, lockfile.get(&dep.reference)) {
            Ok(()) => {
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
            Err(error) => {
                warn!(
                    "  ↻ {} v{} — replacing invalid cache: {}",
                    dep.reference, resolved.version, error
                );
            }
        }
    }

    // Download the binary
    info!(
        "  ↓ {} v{} ({}) — downloading...",
        dep.reference, resolved.version, resolved.platform
    );

    let download =
        download_plugin_replacing(client, &resolved.reference, plugins_dir, &resolved.filename)
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
    let mut install_failures = Vec::new();

    for plugin_dep in &config.plugins {
        match local.resolve(&plugin_dep.reference) {
            Ok(info) => {
                validate_plugin_binary(&info.file_path, Some(&info.version)).with_context(
                    || {
                        format!(
                            "local plugin '{}' is incompatible with this Drasi Server",
                            plugin_dep.reference
                        )
                    },
                )?;

                let dest_path = plugins_dir.join(&info.filename);
                let source_hash = compute_file_hash(&info.file_path)?;
                let destination_matches = dest_path.exists()
                    && compute_file_hash(&dest_path)
                        .map(|destination_hash| destination_hash == source_hash)
                        .unwrap_or(false);

                if destination_matches {
                    info!("  ✓ {} — already installed (local)", plugin_dep.reference);
                } else {
                    info!(
                        "  ← {} — copying from {}...",
                        plugin_dep.reference,
                        dir.display()
                    );
                    copy_plugin_replacing(&info.file_path, plugins_dir, &info.filename)
                        .with_context(|| {
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
                install_failures.push(format!("{}: {e}", plugin_dep.reference));
            }
        }
    }

    if lockfile_updated {
        lockfile.write(lockfile_dir)?;
    }

    if !install_failures.is_empty() {
        bail!(
            "failed to install required plugin(s) from local directory:\n  - {}",
            install_failures.join("\n  - ")
        );
    }

    if !resolved.is_empty() {
        info!(
            "Plugin auto-install from local dir complete: {} plugin(s) ready",
            resolved.len()
        );
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved_plugin() -> ResolvedPlugin {
        ResolvedPlugin {
            reference: "ghcr.io/drasi-project/source/http@sha256:abc".to_string(),
            version: "0.2.8".to_string(),
            sdk_version: "0.10.0".to_string(),
            core_version: "0.5.8".to_string(),
            lib_version: "0.8.9".to_string(),
            platform: "darwin/arm64".to_string(),
            digest: "sha256:abc".to_string(),
            filename: "libdrasi_source_http.dylib".to_string(),
        }
    }

    fn locked_plugin() -> LockedPlugin {
        let resolved = resolved_plugin();
        LockedPlugin {
            reference: resolved.reference,
            version: resolved.version,
            digest: resolved.digest,
            sdk_version: resolved.sdk_version,
            core_version: resolved.core_version,
            lib_version: resolved.lib_version,
            platform: resolved.platform,
            filename: resolved.filename,
            file_hash: Some("hash".to_string()),
            git_commit: None,
            build_timestamp: None,
            signature: None,
        }
    }

    #[test]
    fn parses_major_minor_version() {
        assert_eq!(major_minor("0.13.0"), Some((0, 13)));
        assert_eq!(major_minor("1.2"), Some((1, 2)));
        assert_eq!(major_minor("invalid"), None);
    }

    #[test]
    fn matching_lock_entry_can_reuse_resolved_plugin() {
        assert!(lock_entry_matches_resolution(
            &locked_plugin(),
            &resolved_plugin()
        ));
    }

    #[test]
    fn changed_digest_invalidates_lock_entry() {
        let mut resolved = resolved_plugin();
        resolved.digest = "sha256:def".to_string();
        assert!(!lock_entry_matches_resolution(&locked_plugin(), &resolved));
    }

    #[test]
    fn reports_unreadable_plugin_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("libdrasi_source_invalid.dylib");
        std::fs::write(&plugin, b"not a dynamic library").unwrap();
        std::fs::write(dir.path().join("unrelated.dylib"), b"ignored").unwrap();

        let errors = plugin_compatibility_errors(dir.path());

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, plugin);
        assert!(errors[0]
            .1
            .contains("could not read embedded metadata from"));
    }

    #[test]
    fn rejects_hash_mismatch_before_reading_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("libdrasi_source_http.dylib");
        std::fs::write(&plugin, b"not a dynamic library").unwrap();
        let locked = locked_plugin();

        let error = validate_cached_plugin(&plugin, &resolved_plugin(), Some(&locked)).unwrap_err();

        assert!(error.to_string().contains("plugin file hash mismatch"));
    }

    #[test]
    fn replaces_existing_plugin_from_staging() {
        let dir = tempfile::tempdir().unwrap();
        let staged = dir.path().join(".staged");
        let destination = dir.path().join("libdrasi_source_http.dylib");
        let backup = dir.path().join(".backup");
        std::fs::write(&staged, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();

        replace_staged_file(&staged, &destination, &backup).unwrap();

        assert_eq!(std::fs::read(destination).unwrap(), b"new");
        assert!(!staged.exists());
        assert!(!backup.exists());
    }

    #[tokio::test]
    async fn missing_local_plugin_is_fatal() {
        let registry = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let config = DrasiServerConfig {
            plugin_registry: Some(registry.path().to_string_lossy().into_owned()),
            auto_install_plugins: true,
            plugins: vec![PluginDependency {
                reference: "reaction/sse".to_string(),
            }],
            ..Default::default()
        };

        let error = auto_install_plugins(&config, destination.path(), false)
            .await
            .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("failed to install required plugin(s) from local directory"));
        assert!(message.contains("reaction/sse"));
    }
}
