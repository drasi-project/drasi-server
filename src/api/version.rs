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

//! API version definitions and utilities.
//!
//! This module provides version constants and helpers for managing
//! multiple API versions.

use std::fmt;

/// The current/latest API version.
pub const API_CURRENT_VERSION: ApiVersion = ApiVersion::V1;

/// Available API versions.
///
/// When adding a new API version:
/// 1. Add a new variant to this enum (e.g., V2)
/// 2. Create a new module under `src/api/v2/`
/// 3. Implement handlers, routes, and openapi for the new version
/// 4. Update `API_CURRENT_VERSION` if the new version becomes the default
/// 5. Register the new version in the version router
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiVersion {
    /// API Version 1
    V1,
    // Future versions:
    // V2,
    // V3,
}

impl ApiVersion {
    /// Get the URL path prefix for this version.
    pub fn path_prefix(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "/api/v1",
            // ApiVersion::V2 => "/api/v2",
        }
    }

    /// Get the version string (e.g., "v1").
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
            // ApiVersion::V2 => "v2",
        }
    }

    /// Get all available API versions.
    pub fn all() -> &'static [ApiVersion] {
        &[ApiVersion::V1]
    }

    /// Get all version strings.
    pub fn all_strings() -> Vec<String> {
        Self::all().iter().map(|v| v.as_str().to_string()).collect()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ApiVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "v1" | "1" => Ok(ApiVersion::V1),
            // "v2" | "2" => Ok(ApiVersion::V2),
            _ => Err(format!("Unknown API version: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_path_prefix() {
        assert_eq!(ApiVersion::V1.path_prefix(), "/api/v1");
    }

    #[test]
    fn test_version_as_str() {
        assert_eq!(ApiVersion::V1.as_str(), "v1");
    }

    #[test]
    fn test_version_from_str() {
        assert_eq!("v1".parse::<ApiVersion>().unwrap(), ApiVersion::V1);
        assert_eq!("V1".parse::<ApiVersion>().unwrap(), ApiVersion::V1);
        assert_eq!("1".parse::<ApiVersion>().unwrap(), ApiVersion::V1);
        assert!("v99".parse::<ApiVersion>().is_err());
    }

    #[test]
    fn test_all_versions() {
        let versions = ApiVersion::all();
        assert!(versions.contains(&ApiVersion::V1));
    }
}
