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

//! Plugin lockfile management.
//!
//! The lockfile (`plugins.lock`) records the exact resolved versions, digests,
//! and filenames for each plugin dependency. This enables reproducible installs
//! and the `--locked` flag to enforce exact versions.

use anyhow::{Context, Result, bail};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const LOCKFILE_NAME: &str = "plugins.lock";
const LOCKFILE_VERSION: u32 = 1;

/// The top-level lockfile structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginLockfile {
    /// Lockfile format version.
    pub version: u32,
    /// Locked plugin entries keyed by the original reference string.
    #[serde(default)]
    pub plugins: BTreeMap<String, LockedPlugin>,
}

/// A single locked plugin entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedPlugin {
    /// Fully qualified OCI reference with digest (e.g., ghcr.io/drasi-project/source/postgres@sha256:abc...).
    pub reference: String,
    /// Resolved version tag.
    pub version: String,
    /// Content digest (sha256:...).
    pub digest: String,
    /// SDK version of the plugin.
    pub sdk_version: String,
    /// Core version of the plugin.
    pub core_version: String,
    /// Lib version of the plugin.
    pub lib_version: String,
    /// OCI platform string (e.g., linux/amd64).
    pub platform: String,
    /// Expected binary filename (e.g., libdrasi_source_postgres.so).
    pub filename: String,
}

impl PluginLockfile {
    /// Create a new empty lockfile.
    pub fn new() -> Self {
        Self {
            version: LOCKFILE_VERSION,
            plugins: BTreeMap::new(),
        }
    }

    /// Read a lockfile from disk. Returns None if the file doesn't exist.
    pub fn read(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(LOCKFILE_NAME);
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let lockfile: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        if lockfile.version != LOCKFILE_VERSION {
            bail!(
                "unsupported lockfile version {} (expected {})",
                lockfile.version,
                LOCKFILE_VERSION
            );
        }

        debug!("Read lockfile with {} entries", lockfile.plugins.len());
        Ok(Some(lockfile))
    }

    /// Write the lockfile to disk.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(LOCKFILE_NAME);
        let content = toml::to_string_pretty(self)
            .context("failed to serialize lockfile")?;

        // Atomic write: temp file + rename
        let tmp_path = dir.join(".plugins.lock.tmp");
        std::fs::write(&tmp_path, &content)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to rename lockfile to {}", path.display()))?;

        info!("Updated {}", path.display());
        Ok(())
    }

    /// Get a locked entry for a plugin reference.
    pub fn get(&self, reference: &str) -> Option<&LockedPlugin> {
        self.plugins.get(reference)
    }

    /// Insert or update a locked entry.
    pub fn insert(&mut self, reference: String, entry: LockedPlugin) {
        self.plugins.insert(reference, entry);
    }

    /// Remove a locked entry.
    pub fn remove(&mut self, reference: &str) -> Option<LockedPlugin> {
        self.plugins.remove(reference)
    }
}

impl Default for PluginLockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_entry() -> LockedPlugin {
        LockedPlugin {
            reference: "ghcr.io/drasi-project/source/postgres@sha256:abc123".to_string(),
            version: "0.1.8".to_string(),
            digest: "sha256:abc123".to_string(),
            sdk_version: "0.3.1".to_string(),
            core_version: "0.3.3".to_string(),
            lib_version: "0.3.8".to_string(),
            platform: "linux/amd64".to_string(),
            filename: "libdrasi_source_postgres.so".to_string(),
        }
    }

    #[test]
    fn test_lockfile_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut lockfile = PluginLockfile::new();
        lockfile.insert("source/postgres".to_string(), sample_entry());
        lockfile.insert(
            "reaction/log".to_string(),
            LockedPlugin {
                reference: "ghcr.io/drasi-project/reaction/log@sha256:def456".to_string(),
                version: "0.1.7".to_string(),
                digest: "sha256:def456".to_string(),
                sdk_version: "0.3.1".to_string(),
                core_version: "0.3.3".to_string(),
                lib_version: "0.3.8".to_string(),
                platform: "linux/amd64".to_string(),
                filename: "libdrasi_reaction_log.so".to_string(),
            },
        );

        lockfile.write(dir.path()).unwrap();

        let loaded = PluginLockfile::read(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.plugins.len(), 2);
        assert_eq!(
            loaded.get("source/postgres").unwrap(),
            &sample_entry()
        );
    }

    #[test]
    fn test_lockfile_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        assert!(PluginLockfile::read(dir.path()).unwrap().is_none());
    }

    #[test]
    fn test_lockfile_toml_format() {
        let mut lockfile = PluginLockfile::new();
        lockfile.insert("source/postgres".to_string(), sample_entry());

        let content = toml::to_string_pretty(&lockfile).unwrap();
        assert!(content.contains("version = 1"));
        assert!(content.contains("[plugins.\"source/postgres\"]"));
        assert!(content.contains("sha256:abc123"));
    }

    #[test]
    fn test_lockfile_remove() {
        let mut lockfile = PluginLockfile::new();
        lockfile.insert("source/postgres".to_string(), sample_entry());
        assert!(lockfile.get("source/postgres").is_some());

        lockfile.remove("source/postgres");
        assert!(lockfile.get("source/postgres").is_none());
    }
}
