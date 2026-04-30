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

//! ABI compatibility validation for plugin upgrades.
//!
//! Validates that two plugin versions can coexist during a rolling upgrade
//! by checking SDK version compatibility and target platform matching.

use crate::upgrade::error::UpgradeError;

/// Plugin metadata used for ABI compatibility checks.
#[derive(Debug, Clone)]
pub struct PluginVersionInfo {
    /// SDK version string (e.g., "0.6.0").
    pub sdk_version: String,
    /// Plugin version string (e.g., "1.0.0").
    pub plugin_version: String,
    /// Target triple (e.g., "aarch64-apple-darwin").
    pub target_triple: String,
    /// Plugin kind identifier (e.g., "source/postgres").
    pub plugin_kind: String,
}

/// Validates that two plugin versions are ABI-compatible for dual-load upgrade.
///
/// Requirements for compatibility:
/// 1. SDK major.minor version must match (patch can differ)
/// 2. Target triple must match
/// 3. Plugin kind must be the same
pub fn validate_abi_compatibility(
    current: &PluginVersionInfo,
    candidate: &PluginVersionInfo,
) -> Result<(), UpgradeError> {
    // Parse SDK versions to compare major.minor
    let current_parts = parse_semver(&current.sdk_version);
    let candidate_parts = parse_semver(&candidate.sdk_version);

    match (current_parts, candidate_parts) {
        (Some((cur_major, cur_minor, _)), Some((cand_major, cand_minor, _))) => {
            if cur_major != cand_major || cur_minor != cand_minor {
                return Err(UpgradeError::AbiMismatch {
                    current_sdk: current.sdk_version.clone(),
                    candidate_sdk: candidate.sdk_version.clone(),
                });
            }
        }
        _ => {
            // If we can't parse either version, reject
            return Err(UpgradeError::AbiMismatch {
                current_sdk: current.sdk_version.clone(),
                candidate_sdk: candidate.sdk_version.clone(),
            });
        }
    }

    // Target triple must match
    if !current.target_triple.is_empty()
        && !candidate.target_triple.is_empty()
        && current.target_triple != candidate.target_triple
    {
        return Err(UpgradeError::TargetMismatch {
            current: current.target_triple.clone(),
            candidate: candidate.target_triple.clone(),
        });
    }

    Ok(())
}

/// Parse a semver-like string "major.minor.patch" into components.
fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts.get(2).and_then(|p| p.parse::<u64>().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_info(sdk: &str, target: &str, kind: &str) -> PluginVersionInfo {
        PluginVersionInfo {
            sdk_version: sdk.to_string(),
            plugin_version: "1.0.0".to_string(),
            target_triple: target.to_string(),
            plugin_kind: kind.to_string(),
        }
    }

    #[test]
    fn test_compatible_same_major_minor() {
        let current = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("0.6.1", "aarch64-apple-darwin", "source/postgres");
        assert!(validate_abi_compatibility(&current, &candidate).is_ok());
    }

    #[test]
    fn test_compatible_same_version() {
        let current = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        assert!(validate_abi_compatibility(&current, &candidate).is_ok());
    }

    #[test]
    fn test_incompatible_major_mismatch() {
        let current = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("1.0.0", "aarch64-apple-darwin", "source/postgres");
        let err = validate_abi_compatibility(&current, &candidate).unwrap_err();
        assert!(matches!(err, UpgradeError::AbiMismatch { .. }));
    }

    #[test]
    fn test_incompatible_minor_mismatch() {
        let current = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("0.7.0", "aarch64-apple-darwin", "source/postgres");
        let err = validate_abi_compatibility(&current, &candidate).unwrap_err();
        assert!(matches!(err, UpgradeError::AbiMismatch { .. }));
    }

    #[test]
    fn test_incompatible_target_mismatch() {
        let current = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("0.6.0", "x86_64-unknown-linux-gnu", "source/postgres");
        let err = validate_abi_compatibility(&current, &candidate).unwrap_err();
        assert!(matches!(err, UpgradeError::TargetMismatch { .. }));
    }

    #[test]
    fn test_empty_targets_are_compatible() {
        let current = make_info("0.6.0", "", "source/postgres");
        let candidate = make_info("0.6.0", "", "source/postgres");
        assert!(validate_abi_compatibility(&current, &candidate).is_ok());
    }

    #[test]
    fn test_unparseable_version_rejected() {
        let current = make_info("abc", "aarch64-apple-darwin", "source/postgres");
        let candidate = make_info("0.6.0", "aarch64-apple-darwin", "source/postgres");
        let err = validate_abi_compatibility(&current, &candidate).unwrap_err();
        assert!(matches!(err, UpgradeError::AbiMismatch { .. }));
    }
}
