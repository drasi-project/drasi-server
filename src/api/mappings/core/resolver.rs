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

//! Value resolvers for different ConfigValue reference types.

use crate::api::models::ConfigValue;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during value resolution
#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("Environment variable '{0}' not found and no default provided")]
    EnvVarNotFound(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("No resolver found for reference type: {0}")]
    NoResolverFound(String),

    #[error("Wrong resolver type used for this reference")]
    WrongResolverType,

    #[error("Failed to parse value: {0}")]
    ParseError(String),
}

/// Trait for resolving a specific type of ConfigValue variant
pub trait ValueResolver: Send + Sync {
    /// Resolve a ConfigValue variant to its actual string value (always resolves to string first)
    fn resolve_to_string(&self, value: &ConfigValue<String>) -> Result<String, ResolverError>;
}

/// Environment variable resolver
pub struct EnvironmentVariableResolver;

impl ValueResolver for EnvironmentVariableResolver {
    fn resolve_to_string(&self, value: &ConfigValue<String>) -> Result<String, ResolverError> {
        match value {
            ConfigValue::EnvironmentVariable { name, default } => {
                std::env::var(name).or_else(|_| {
                    default
                        .clone()
                        .ok_or_else(|| ResolverError::EnvVarNotFound(name.clone()))
                })
            }
            _ => Err(ResolverError::WrongResolverType),
        }
    }
}

/// Secret resolver (unimplemented)
pub struct SecretResolver;

impl ValueResolver for SecretResolver {
    fn resolve_to_string(&self, value: &ConfigValue<String>) -> Result<String, ResolverError> {
        match value {
            ConfigValue::Secret { name } => Err(ResolverError::NotImplemented(format!(
                "Secret resolution not yet implemented for '{name}'"
            ))),
            _ => Err(ResolverError::WrongResolverType),
        }
    }
}

/// Environment variable resolver with override support.
///
/// Resolution order:
/// 1. Check overrides map first
/// 2. Fall back to environment variable
/// 3. Fall back to default value (if provided)
/// 4. Return error if no value found
///
/// This enables Solution Templates to provide user-supplied variable values
/// that take precedence over environment variables.
pub struct OverridingEnvResolver {
    overrides: HashMap<String, String>,
}

impl OverridingEnvResolver {
    /// Create a new resolver with the given override values.
    pub fn new(overrides: HashMap<String, String>) -> Self {
        Self { overrides }
    }
}

impl ValueResolver for OverridingEnvResolver {
    fn resolve_to_string(&self, value: &ConfigValue<String>) -> Result<String, ResolverError> {
        match value {
            ConfigValue::EnvironmentVariable { name, default } => {
                // 1. Check overrides first
                if let Some(override_val) = self.overrides.get(name) {
                    return Ok(override_val.clone());
                }
                // 2. Fall back to env var
                if let Ok(env_val) = std::env::var(name) {
                    return Ok(env_val);
                }
                // 3. Fall back to default
                default
                    .clone()
                    .ok_or_else(|| ResolverError::EnvVarNotFound(name.clone()))
            }
            _ => Err(ResolverError::WrongResolverType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_resolver_with_set_var() {
        std::env::set_var("TEST_VAR_1", "test_value");

        let resolver = EnvironmentVariableResolver;
        let value = ConfigValue::EnvironmentVariable {
            name: "TEST_VAR_1".to_string(),
            default: None,
        };

        let result = resolver.resolve_to_string(&value).unwrap();
        assert_eq!(result, "test_value");

        std::env::remove_var("TEST_VAR_1");
    }

    #[test]
    fn test_env_resolver_with_default() {
        let resolver = EnvironmentVariableResolver;
        let value = ConfigValue::EnvironmentVariable {
            name: "NONEXISTENT_VAR_12345".to_string(),
            default: Some("default_value".to_string()),
        };

        let result = resolver.resolve_to_string(&value).unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_env_resolver_missing_var_no_default() {
        let resolver = EnvironmentVariableResolver;
        let value = ConfigValue::EnvironmentVariable {
            name: "NONEXISTENT_VAR_67890".to_string(),
            default: None,
        };

        let result = resolver.resolve_to_string(&value);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolverError::EnvVarNotFound(_)
        ));
    }

    #[test]
    fn test_secret_resolver_not_implemented() {
        let resolver = SecretResolver;
        let value = ConfigValue::Secret {
            name: "my-secret".to_string(),
        };

        let result = resolver.resolve_to_string(&value);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolverError::NotImplemented(_)
        ));
    }

    // OverridingEnvResolver tests

    #[test]
    fn test_overriding_resolver_override_takes_precedence() {
        // Set env var that should be overridden
        std::env::set_var("TEST_OVERRIDE_VAR", "env_value");

        let mut overrides = HashMap::new();
        overrides.insert(
            "TEST_OVERRIDE_VAR".to_string(),
            "override_value".to_string(),
        );

        let resolver = OverridingEnvResolver::new(overrides);
        let value = ConfigValue::EnvironmentVariable {
            name: "TEST_OVERRIDE_VAR".to_string(),
            default: Some("default_value".to_string()),
        };

        let result = resolver.resolve_to_string(&value).unwrap();
        assert_eq!(result, "override_value");

        std::env::remove_var("TEST_OVERRIDE_VAR");
    }

    #[test]
    fn test_overriding_resolver_falls_back_to_env() {
        std::env::set_var("TEST_FALLBACK_ENV_VAR", "env_value");

        let overrides = HashMap::new(); // No overrides

        let resolver = OverridingEnvResolver::new(overrides);
        let value = ConfigValue::EnvironmentVariable {
            name: "TEST_FALLBACK_ENV_VAR".to_string(),
            default: Some("default_value".to_string()),
        };

        let result = resolver.resolve_to_string(&value).unwrap();
        assert_eq!(result, "env_value");

        std::env::remove_var("TEST_FALLBACK_ENV_VAR");
    }

    #[test]
    fn test_overriding_resolver_falls_back_to_default() {
        let overrides = HashMap::new(); // No overrides
                                        // No env var set

        let resolver = OverridingEnvResolver::new(overrides);
        let value = ConfigValue::EnvironmentVariable {
            name: "NONEXISTENT_OVERRIDE_VAR_123".to_string(),
            default: Some("default_value".to_string()),
        };

        let result = resolver.resolve_to_string(&value).unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_overriding_resolver_error_when_no_value() {
        let overrides = HashMap::new(); // No overrides
                                        // No env var set, no default

        let resolver = OverridingEnvResolver::new(overrides);
        let value = ConfigValue::EnvironmentVariable {
            name: "NONEXISTENT_REQUIRED_VAR_456".to_string(),
            default: None,
        };

        let result = resolver.resolve_to_string(&value);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolverError::EnvVarNotFound(_)
        ));
    }

    #[test]
    fn test_overriding_resolver_wrong_type() {
        let overrides = HashMap::new();
        let resolver = OverridingEnvResolver::new(overrides);
        let value = ConfigValue::Secret {
            name: "some-secret".to_string(),
        };

        let result = resolver.resolve_to_string(&value);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolverError::WrongResolverType
        ));
    }
}
