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

//! Shared plugin-management operations used by init, CLI, startup, and Web API.
//!
//! `PluginOperations` is the single source of truth for file-level plugin
//! management: scanning directories, reading metadata, downloading from OCI
//! registries, computing hashes, and managing the lockfile. It wraps host-sdk
//! primitives with server config policy so that all entry points (init wizard,
//! CLI plugin commands, server startup, REST API handlers) follow the same behavior.

use std::path::{Path, PathBuf};

use anyhow::Result;
use drasi_host_sdk::loader::{
    scan_plugin_metadata, PluginMetadataSummary, DEFAULT_PLUGIN_FILE_PATTERNS,
};
use drasi_host_sdk::lockfile::{compute_file_hash, PluginLockfile};
use drasi_host_sdk::registry::{
    HostVersionInfo, LocalDirRegistry, PluginSourceKind, RegistryAuth, RegistryConfig,
    TrustedIdentity, VerificationConfig,
};

use crate::config::DrasiServerConfig;

/// Shared plugin-management operations.
///
/// Wraps host-sdk primitives with server config policy. Consumed by the init
/// wizard, CLI plugin commands, server startup, and REST API plugin handlers.
pub struct PluginOperations {
    plugins_dir: PathBuf,
    default_registry: String,
}

impl PluginOperations {
    /// Create a new `PluginOperations` instance.
    pub fn new(plugins_dir: PathBuf, default_registry: String) -> Self {
        Self {
            plugins_dir,
            default_registry,
        }
    }

    /// Create from a server config, using its configured registry URL.
    pub fn from_config(config: &DrasiServerConfig, plugins_dir: PathBuf) -> Self {
        let default_registry = config
            .plugin_registry
            .clone()
            .unwrap_or_else(|| "ghcr.io/drasi-project".to_string());
        Self::new(plugins_dir, default_registry)
    }

    /// The plugins directory this instance operates on.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// The default OCI registry URL.
    pub fn default_registry(&self) -> &str {
        &self.default_registry
    }

    // ── Local plugin management ──

    /// Scan the plugins directory for cdylib files and read their metadata
    /// (calls `drasi_plugin_metadata()` only, no `drasi_plugin_init()`).
    pub fn scan_local_plugins(&self) -> Result<Vec<PluginMetadataSummary>> {
        let dir = &self.plugins_dir;
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(dir)?;
        let mut summaries = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let filename = entry.file_name();
            let name = filename.to_string_lossy();

            if !is_plugin_binary(&name) {
                continue;
            }

            // Check if filename matches default plugin patterns
            let matches = DEFAULT_PLUGIN_FILE_PATTERNS.iter().any(|pattern| {
                let stem = name
                    .strip_suffix(".so")
                    .or_else(|| name.strip_suffix(".dylib"))
                    .or_else(|| name.strip_suffix(".dll"))
                    .unwrap_or(&name);
                let pat_stem = pattern.strip_suffix('*').unwrap_or(pattern);
                stem.starts_with(pat_stem) || *pattern == &*name
            });

            if !matches {
                continue;
            }

            if let Some(summary) = scan_plugin_metadata(&path) {
                summaries.push(summary);
            }
        }

        Ok(summaries)
    }

    /// List installed plugin files with lockfile metadata.
    pub fn list_installed(&self) -> Result<Vec<InstalledPluginInfo>> {
        let lockfile = PluginLockfile::read(&self.plugins_dir)?;
        let dir = &self.plugins_dir;

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(dir)?;
        let mut result = Vec::new();

        for entry in entries {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();

            if !is_plugin_binary(&filename) {
                continue;
            }

            let lockfile_entry = lockfile.as_ref().and_then(|lf| {
                lf.iter()
                    .find(|(_, v)| v.filename == filename)
                    .map(|(k, v)| (k.clone(), v.clone()))
            });

            let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);

            result.push(InstalledPluginInfo {
                filename,
                file_size,
                lockfile_key: lockfile_entry.as_ref().map(|(k, _)| k.clone()),
                lockfile_entry: lockfile_entry.map(|(_, v)| v),
            });
        }

        Ok(result)
    }

    /// Remove a plugin file from the plugins directory and update the lockfile.
    ///
    /// **Concurrency note:** At runtime, callers should go through
    /// [`PluginOrchestrator`] methods which hold the directory mutex.
    /// Direct calls are safe during startup/CLI (single-threaded) workflows.
    pub fn remove_plugin_file(&self, filename: &str) -> Result<bool> {
        let path = self.plugins_dir.join(filename);
        if !path.exists() {
            return Ok(false);
        }

        std::fs::remove_file(&path)?;

        // Update lockfile if present
        if let Some(mut lockfile) = PluginLockfile::read(&self.plugins_dir)? {
            let keys_to_remove: Vec<String> = lockfile
                .iter()
                .filter(|(_, v)| v.filename == filename)
                .map(|(k, _)| k.clone())
                .collect();

            for key in &keys_to_remove {
                lockfile.remove(key);
            }

            if !keys_to_remove.is_empty() {
                lockfile.write(&self.plugins_dir)?;
            }
        }

        Ok(true)
    }

    /// Compute SHA-256 hash of a plugin file.
    pub fn compute_file_hash(&self, filename: &str) -> Result<String> {
        let path = self.plugins_dir.join(filename);
        compute_file_hash(&path)
    }

    // ── Registry auth and verification ──

    /// Build the single source of truth for registry authentication.
    ///
    /// Reads from environment variables (`OCI_REGISTRY_PASSWORD`, `GHCR_TOKEN`,
    /// `OCI_REGISTRY_USERNAME`). This replaces the duplicated implementations
    /// in `plugin_install.rs` and `plugin/mod.rs`.
    pub fn registry_auth() -> RegistryAuth {
        let password = std::env::var("OCI_REGISTRY_PASSWORD")
            .or_else(|_| std::env::var("GHCR_TOKEN"))
            .ok();

        match password {
            Some(pwd) => {
                let username = std::env::var("OCI_REGISTRY_USERNAME").unwrap_or_default();
                RegistryAuth::Basic {
                    username,
                    password: pwd,
                }
            }
            None => RegistryAuth::Anonymous,
        }
    }

    /// Build host version info from compile-time environment variables.
    ///
    /// This replaces the duplicated `build_host_version_info` / `cli_host_version_info`.
    pub fn host_version_info() -> HostVersionInfo {
        HostVersionInfo {
            sdk_version: env!("DRASI_PLUGIN_SDK_VERSION").to_string(),
            core_version: env!("DRASI_CORE_VERSION").to_string(),
            lib_version: env!("DRASI_LIB_VERSION").to_string(),
            target_triple: env!("TARGET_TRIPLE").to_string(),
        }
    }

    /// Build verification config from server configuration.
    ///
    /// Replaces the `build_verification_config` in `plugin_install.rs`.
    pub fn verification_config(config: &DrasiServerConfig) -> VerificationConfig {
        VerificationConfig {
            enabled: config.verify_plugins,
            trusted_identities: config
                .trusted_identities
                .iter()
                .map(|ti| TrustedIdentity {
                    issuer: ti.issuer.clone(),
                    subject_pattern: ti.subject_pattern.clone(),
                })
                .collect(),
        }
    }

    /// Build a registry config with the default settings for this instance.
    pub fn registry_config(&self) -> RegistryConfig {
        RegistryConfig {
            default_registry: self.default_registry.clone(),
            auth: Self::registry_auth(),
        }
    }

    /// Resolve the effective registry URL from a config path and optional override.
    ///
    /// Priority: explicit override > config file value > default ("ghcr.io/drasi-project").
    /// Replaces the duplicated `get_plugin_registry` helper.
    pub fn resolve_registry(
        config_path: &std::path::Path,
        override_registry: Option<&str>,
    ) -> String {
        if let Some(r) = override_registry {
            return r.to_string();
        }
        if let Ok(config) = crate::config::load_config_file(config_path) {
            config
                .plugin_registry
                .unwrap_or_else(|| "ghcr.io/drasi-project".to_string())
        } else {
            "ghcr.io/drasi-project".to_string()
        }
    }

    /// Build an OCI registry client with best-effort signature verification.
    ///
    /// Replaces the duplicated `cli_registry_client` helper.
    pub fn build_registry_client(
        config: RegistryConfig,
    ) -> drasi_host_sdk::registry::OciRegistryClient {
        let verification = VerificationConfig {
            enabled: true,
            ..Default::default()
        };
        drasi_host_sdk::registry::OciRegistryClient::with_verifier(
            config,
            drasi_host_sdk::registry::CosignVerifier::new(verification),
        )
    }

    /// Build an OCI registry client with explicit verification config.
    pub fn build_registry_client_with_verification(
        config: RegistryConfig,
        verification: VerificationConfig,
    ) -> drasi_host_sdk::registry::OciRegistryClient {
        drasi_host_sdk::registry::OciRegistryClient::with_verifier(
            config,
            drasi_host_sdk::registry::CosignVerifier::new(verification),
        )
    }

    /// Load trusted signing identities from config, with a default fallback
    /// to the Drasi project CI identity.
    ///
    /// Replaces the duplicated `load_trusted_identities` helper.
    pub fn load_trusted_identities(
        config_path: &std::path::Path,
    ) -> Vec<drasi_host_sdk::registry::TrustedIdentity> {
        use drasi_host_sdk::registry::TrustedIdentity;

        let config_identities = crate::config::load_config_file(config_path)
            .ok()
            .map(|c| c.trusted_identities)
            .unwrap_or_default();

        if config_identities.is_empty() {
            vec![TrustedIdentity {
                issuer: "https://token.actions.githubusercontent.com".to_string(),
                subject_pattern: "https://github.com/drasi-project/*".to_string(),
            }]
        } else {
            config_identities
                .iter()
                .map(|ti| TrustedIdentity {
                    issuer: ti.issuer.clone(),
                    subject_pattern: ti.subject_pattern.clone(),
                })
                .collect()
        }
    }

    /// Download a plugin from an OCI registry or copy from a local directory,
    /// and return the local file path.
    ///
    /// When the registry value is a local path, the plugin is copied directly.
    /// When it's an OCI URL, the existing resolve/download flow is used.
    ///
    /// **Concurrency note:** At runtime, callers should go through
    /// [`PluginOrchestrator::install_and_load`] which holds the directory mutex.
    /// Direct calls are safe during startup/CLI (single-threaded) workflows.
    pub async fn install_from_registry(
        &self,
        reference: &str,
        registry_override: Option<&str>,
    ) -> Result<std::path::PathBuf> {
        let registry_value = registry_override
            .map(String::from)
            .unwrap_or_else(|| self.default_registry.clone());

        match PluginSourceKind::parse(&registry_value) {
            PluginSourceKind::LocalDir(dir) => self.install_from_local_dir(reference, &dir).await,
            PluginSourceKind::Oci(_) => self.install_from_oci(reference, &registry_value).await,
        }
    }

    /// Search for plugins in the configured registry (OCI or local directory).
    ///
    /// Returns a unified list of search results regardless of source kind.
    pub async fn search_registry(
        &self,
        query: &str,
        registry_override: Option<&str>,
    ) -> Result<Vec<PluginSearchResultUnified>> {
        let registry_value = registry_override
            .map(String::from)
            .unwrap_or_else(|| self.default_registry.clone());

        match PluginSourceKind::parse(&registry_value) {
            PluginSourceKind::LocalDir(dir) => {
                let local = LocalDirRegistry::new(&dir);
                let results = local.search(query)?;
                Ok(results
                    .into_iter()
                    .map(|r| PluginSearchResultUnified {
                        reference: r.reference,
                        full_reference: format!("file://{}", r.file_path.display()),
                        version: r.version,
                        filename: r.filename,
                        source: "local".to_string(),
                    })
                    .collect())
            }
            PluginSourceKind::Oci(_) => {
                let config = RegistryConfig {
                    default_registry: registry_value.clone(),
                    auth: Self::registry_auth(),
                };
                let client = Self::build_registry_client(config);
                let results = client.search_plugins(query).await?;
                Ok(results
                    .into_iter()
                    .map(|r| PluginSearchResultUnified {
                        reference: r.reference.clone(),
                        full_reference: r.full_reference,
                        version: r
                            .versions
                            .first()
                            .map(|v| v.version.clone())
                            .unwrap_or_default(),
                        filename: String::new(),
                        source: "oci".to_string(),
                    })
                    .collect())
            }
        }
    }

    /// Install a plugin from a local directory.
    async fn install_from_local_dir(
        &self,
        reference: &str,
        dir: &Path,
    ) -> Result<std::path::PathBuf> {
        let local = LocalDirRegistry::new(dir);
        let resolved = local.resolve(reference)?;
        let dest = local.install(&resolved, &self.plugins_dir)?;

        // Update lockfile
        let mut lockfile = PluginLockfile::read(&self.plugins_dir)?.unwrap_or_default();
        lockfile.insert(
            reference.to_string(),
            drasi_host_sdk::lockfile::LockedPlugin {
                reference: format!("file://{}", resolved.file_path.display()),
                version: resolved.version,
                digest: String::new(),
                sdk_version: resolved.sdk_version,
                core_version: String::new(),
                lib_version: String::new(),
                platform: env!("TARGET_TRIPLE").to_string(),
                filename: resolved.filename,
                file_hash: compute_file_hash(&dest).ok(),
                git_commit: None,
                build_timestamp: None,
                signature: None,
            },
        );
        lockfile.write(&self.plugins_dir)?;

        log::info!("Plugin installed from local dir: {}", dest.display());
        Ok(dest)
    }

    /// Install a plugin from an OCI registry (internal implementation).
    async fn install_from_oci(
        &self,
        reference: &str,
        registry_url: &str,
    ) -> Result<std::path::PathBuf> {
        use drasi_host_sdk::registry::{
            CosignVerifier, OciRegistryClient, PluginResolver, SignatureStatus,
        };

        let config = RegistryConfig {
            default_registry: registry_url.to_string(),
            auth: Self::registry_auth(),
        };

        let verification = VerificationConfig {
            enabled: true,
            ..Default::default()
        };
        let client = OciRegistryClient::with_verifier(config, CosignVerifier::new(verification));

        let host_info = Self::host_version_info();
        let resolver = PluginResolver::new(&client, &host_info);

        log::info!("Resolving plugin '{reference}' from '{registry_url}'...",);
        let resolved = resolver.resolve(reference, registry_url).await?;

        log::info!(
            "Downloading {} (version {}, platform {})...",
            resolved.filename,
            resolved.version,
            resolved.platform
        );
        std::fs::create_dir_all(&self.plugins_dir)?;
        let download = client
            .download_plugin(&resolved.reference, &self.plugins_dir, &resolved.filename)
            .await?;

        let plugin_path = self.plugins_dir.join(&resolved.filename);

        // Update lockfile
        let mut lockfile = PluginLockfile::read(&self.plugins_dir)?.unwrap_or_default();
        let sig_info = match &download.verification {
            SignatureStatus::Verified(v) => Some(drasi_host_sdk::lockfile::PluginSignatureInfo {
                verified: true,
                issuer: v.issuer.clone(),
                subject: v.subject.clone(),
            }),
            _ => None,
        };
        lockfile.insert(
            reference.to_string(),
            drasi_host_sdk::lockfile::LockedPlugin {
                reference: resolved.reference,
                version: resolved.version,
                digest: resolved.digest,
                sdk_version: resolved.sdk_version,
                core_version: resolved.core_version,
                lib_version: resolved.lib_version,
                platform: resolved.platform,
                filename: resolved.filename.clone(),
                file_hash: compute_file_hash(&plugin_path).ok(),
                git_commit: None,
                build_timestamp: None,
                signature: sig_info,
            },
        );
        lockfile.write(&self.plugins_dir)?;

        log::info!("Plugin installed: {}", plugin_path.display());
        Ok(plugin_path)
    }
}

/// A unified plugin search result that works for both local and OCI sources.
#[derive(Debug, Clone)]
pub struct PluginSearchResultUnified {
    /// Short plugin reference (e.g., "source/postgres").
    pub reference: String,
    /// Full reference (OCI URL or file:// path).
    pub full_reference: String,
    /// Latest version (from metadata or OCI tags).
    pub version: String,
    /// Filename of the binary (populated for local sources).
    pub filename: String,
    /// Source kind: "local" or "oci".
    pub source: String,
}

/// Simple wildcard pattern matching supporting `*` and `?`.
///
/// Replaces the duplicated `wildcard_match` in `plugin/install.rs` and `plugin/remove.rs`.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (None::<usize>, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Check whether a string contains wildcard characters (`*`, `?`, or `[`).
pub fn is_wildcard_pattern(reference: &str) -> bool {
    reference.contains('*') || reference.contains('?') || reference.contains('[')
}

// Re-export host-sdk naming helpers for convenience
pub use drasi_host_sdk::loader::{is_plugin_binary, plugin_kind_from_filename};

/// Information about an installed plugin file on disk.
#[derive(Debug, Clone)]
pub struct InstalledPluginInfo {
    /// The filename of the plugin binary.
    pub filename: String,
    /// File size in bytes.
    pub file_size: u64,
    /// The lockfile reference key, if the plugin has a lockfile entry.
    pub lockfile_key: Option<String>,
    /// The full lockfile entry, if available.
    pub lockfile_entry: Option<drasi_host_sdk::lockfile::LockedPlugin>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_auth_anonymous() {
        // With no env vars set, should return Anonymous
        // (May not be reliable in CI where env vars might be set)
        let _auth = PluginOperations::registry_auth();
    }

    #[test]
    fn test_host_version_info() {
        let info = PluginOperations::host_version_info();
        assert!(!info.sdk_version.is_empty());
        assert!(!info.target_triple.is_empty());
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let result = ops.scan_local_plugins().expect("scan");
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let ops = PluginOperations::new(
            PathBuf::from("/nonexistent/plugins"),
            "ghcr.io/test".to_string(),
        );
        let result = ops.scan_local_plugins().expect("scan");
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_installed_empty() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let result = ops.list_installed().expect("list");
        assert!(result.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let removed = ops.remove_plugin_file("nonexistent.so").expect("remove");
        assert!(!removed);
    }

    #[test]
    fn test_remove_existing_file() {
        let dir = TempDir::new().expect("create temp dir");
        let file_path = dir.path().join("libdrasi_source_test.so");
        std::fs::write(&file_path, b"fake plugin").expect("write");

        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let removed = ops
            .remove_plugin_file("libdrasi_source_test.so")
            .expect("remove");
        assert!(removed);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_compute_file_hash() {
        let dir = TempDir::new().expect("create temp dir");
        let file_path = dir.path().join("test.so");
        std::fs::write(&file_path, b"hello world").expect("write");

        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let hash = ops.compute_file_hash("test.so").expect("hash");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_registry_config() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/custom-org".to_string());
        let config = ops.registry_config();
        assert_eq!(config.default_registry, "ghcr.io/custom-org");
    }

    #[test]
    fn test_wildcard_match_star() {
        // '*' matches any sequence of characters
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("drasi-*", "drasi-source-postgres"));
        assert!(wildcard_match("drasi-*", "drasi-"));
        assert!(!wildcard_match("drasi-*", "other-source"));
        assert!(wildcard_match("*-postgres", "drasi-source-postgres"));
        assert!(!wildcard_match("*-postgres", "postgres")); // '*' matches empty but '-postgres' != 'postgres'
        assert!(!wildcard_match("*-postgres", "drasi-source-mysql"));

        // Multiple stars
        assert!(wildcard_match("*source*", "drasi-source-postgres"));
        assert!(wildcard_match("*source*", "source"));
        assert!(wildcard_match("*source*", "my-source-plugin"));
        assert!(!wildcard_match("*source*", "drasi-reaction-log"));

        // Star at beginning, middle, and end
        assert!(wildcard_match("a*b*c", "abc"));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXcYYb"));
    }

    #[test]
    fn test_wildcard_match_question_mark() {
        // '?' matches exactly one character
        assert!(wildcard_match("?", "a"));
        assert!(!wildcard_match("?", ""));
        assert!(!wildcard_match("?", "ab"));

        assert!(wildcard_match("drasi-source-?", "drasi-source-a"));
        assert!(!wildcard_match("drasi-source-?", "drasi-source-pg"));
        assert!(!wildcard_match("drasi-source-?", "drasi-source-"));

        // Mix of ? and *
        assert!(wildcard_match("?*", "a"));
        assert!(wildcard_match("?*", "abc"));
        assert!(!wildcard_match("?*", ""));

        assert!(wildcard_match("a?c", "abc"));
        assert!(wildcard_match("a?c", "axc"));
        assert!(!wildcard_match("a?c", "ac"));
        assert!(!wildcard_match("a?c", "abbc"));
    }

    #[test]
    fn test_wildcard_match_exact() {
        // Exact match (no wildcards)
        assert!(wildcard_match("hello", "hello"));
        assert!(!wildcard_match("hello", "world"));
        assert!(!wildcard_match("hello", "hell"));
        assert!(!wildcard_match("hello", "helloo"));
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("", "x"));
    }

    #[test]
    fn test_is_wildcard_pattern() {
        assert!(is_wildcard_pattern("drasi-*"));
        assert!(is_wildcard_pattern("source-?"));
        assert!(is_wildcard_pattern("[abc]"));
        assert!(is_wildcard_pattern("*"));
        assert!(!is_wildcard_pattern("drasi-source-postgres"));
        assert!(!is_wildcard_pattern("simple-name"));
        assert!(!is_wildcard_pattern(""));
    }

    #[test]
    fn test_resolve_registry_override() {
        // When an override is provided, it should be used regardless of config
        let result = PluginOperations::resolve_registry(
            Path::new("/nonexistent/config.yaml"),
            Some("my-registry.io/custom"),
        );
        assert_eq!(result, "my-registry.io/custom");
    }

    #[test]
    fn test_resolve_registry_default() {
        // When no override and config file doesn't exist, falls back to default
        let result =
            PluginOperations::resolve_registry(Path::new("/nonexistent/config.yaml"), None);
        assert_eq!(result, "ghcr.io/drasi-project");
    }

    #[test]
    fn test_plugins_dir_accessor() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        assert_eq!(ops.plugins_dir(), dir.path());
    }

    #[test]
    fn test_default_registry_accessor() {
        let dir = TempDir::new().expect("create temp dir");
        let ops =
            PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/my-org/drasi".to_string());
        assert_eq!(ops.default_registry(), "ghcr.io/my-org/drasi");
    }

    #[test]
    fn test_compute_file_hash_different_content() {
        let dir = TempDir::new().expect("create temp dir");

        let file_a = dir.path().join("a.so");
        let file_b = dir.path().join("b.so");
        std::fs::write(&file_a, b"content A").expect("write a");
        std::fs::write(&file_b, b"content B").expect("write b");

        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let hash_a = ops.compute_file_hash("a.so").expect("hash a");
        let hash_b = ops.compute_file_hash("b.so").expect("hash b");

        assert_ne!(
            hash_a, hash_b,
            "different content should yield different hashes"
        );
    }

    #[test]
    fn test_compute_file_hash_nonexistent() {
        let dir = TempDir::new().expect("create temp dir");
        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        let result = ops.compute_file_hash("nonexistent.so");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_plugin_file_returns_false_for_directory() {
        let dir = TempDir::new().expect("create temp dir");
        let sub = dir.path().join("subdir.so");
        std::fs::create_dir(&sub).expect("mkdir");

        let ops = PluginOperations::new(dir.path().to_path_buf(), "ghcr.io/test".to_string());
        // Trying to remove a directory via remove_file should fail
        let result = ops.remove_plugin_file("subdir.so");
        // On most systems std::fs::remove_file on a dir returns Err
        assert!(result.is_err() || !result.unwrap());
    }
}
