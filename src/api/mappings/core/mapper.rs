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

//! DTO to domain model mapping service with value resolution.

use super::resolver::{EnvironmentVariableResolver, ResolverError, SecretResolver, ValueResolver};
use crate::api::models::ConfigValue;
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;

/// Errors that can occur during mapping
#[derive(Debug, Error)]
pub enum MappingError {
    #[error("Failed to resolve config value: {0}")]
    ResolutionError(#[from] ResolverError),

    #[error("No mapper found for config type: {0}")]
    NoMapperFound(String),

    #[error("Mapper type mismatch")]
    MapperTypeMismatch,

    #[error("Failed to create source: {0}")]
    SourceCreationError(String),

    #[error("Failed to create reaction: {0}")]
    ReactionCreationError(String),
}

/// Trait for converting a specific DTO config to its domain model
pub trait ConfigMapper<TDto, TDomain>: Send + Sync {
    fn map(&self, dto: &TDto, resolver: &DtoMapper) -> Result<TDomain, MappingError>;
}

/// Main mapping service that converts API DTOs to domain models
pub struct DtoMapper {
    resolvers: HashMap<&'static str, Box<dyn ValueResolver>>,
}

impl DtoMapper {
    /// Create a new mapper with default resolvers
    pub fn new() -> Self {
        let mut resolvers: HashMap<&'static str, Box<dyn ValueResolver>> = HashMap::new();
        resolvers.insert("EnvironmentVariable", Box::new(EnvironmentVariableResolver));
        resolvers.insert("Secret", Box::new(SecretResolver));

        Self { resolvers }
    }

    /// Resolve a ConfigValue<String> to its actual string value
    pub fn resolve_string(&self, value: &ConfigValue<String>) -> Result<String, ResolverError> {
        match value {
            ConfigValue::Static(s) => Ok(s.clone()),

            ConfigValue::Secret { .. } => {
                let resolver = self
                    .resolvers
                    .get("Secret")
                    .ok_or_else(|| ResolverError::NoResolverFound("Secret".to_string()))?;
                resolver.resolve_to_string(value)
            }

            ConfigValue::EnvironmentVariable { .. } => {
                let resolver = self.resolvers.get("EnvironmentVariable").ok_or_else(|| {
                    ResolverError::NoResolverFound("EnvironmentVariable".to_string())
                })?;
                resolver.resolve_to_string(value)
            }
        }
    }

    /// Resolve a ConfigValue<T> to its typed value (parses from string representation)
    pub fn resolve_typed<T>(&self, value: &ConfigValue<T>) -> Result<T, ResolverError>
    where
        T: FromStr + Clone + serde::Serialize + serde::de::DeserializeOwned,
        T::Err: std::fmt::Display,
    {
        match value {
            ConfigValue::Static(v) => Ok(v.clone()),

            ConfigValue::Secret { name } => {
                // Resolve to string first, then parse
                let string_val = self.resolve_secret_to_string(name)?;
                string_val.parse::<T>().map_err(|e| {
                    ResolverError::ParseError(format!("Failed to parse secret '{}': {}", name, e))
                })
            }

            ConfigValue::EnvironmentVariable { name, default } => {
                // Get string value from env var or default
                let string_val = std::env::var(name).or_else(|_| {
                    default
                        .clone()
                        .ok_or_else(|| ResolverError::EnvVarNotFound(name.clone()))
                })?;

                // Parse to target type
                string_val.parse::<T>().map_err(|e| {
                    ResolverError::ParseError(format!("Failed to parse env var '{}': {}", name, e))
                })
            }
        }
    }

    /// Resolve an optional ConfigValue
    pub fn resolve_optional<T>(
        &self,
        value: &Option<ConfigValue<T>>,
    ) -> Result<Option<T>, ResolverError>
    where
        T: FromStr + Clone + serde::Serialize + serde::de::DeserializeOwned,
        T::Err: std::fmt::Display,
    {
        value.as_ref().map(|v| self.resolve_typed(v)).transpose()
    }

    /// Helper to resolve secret name to string (used by resolve_typed)
    fn resolve_secret_to_string(&self, name: &str) -> Result<String, ResolverError> {
        Err(ResolverError::NotImplemented(format!(
            "Secret resolution not yet implemented for '{}'",
            name
        )))
    }

    /// Map using a config mapper implementation
    pub fn map_with<TDto, TDomain>(
        &self,
        dto: &TDto,
        mapper: &impl ConfigMapper<TDto, TDomain>,
    ) -> Result<TDomain, MappingError> {
        mapper.map(dto, self)
    }
}

impl Default for DtoMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_string_static() {
        let mapper = DtoMapper::new();
        let value = ConfigValue::Static("hello".to_string());

        let result = mapper.resolve_string(&value).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_resolve_string_env_var() {
        std::env::set_var("TEST_MAPPER_VAR", "mapped_value");

        let mapper = DtoMapper::new();
        let value = ConfigValue::EnvironmentVariable {
            name: "TEST_MAPPER_VAR".to_string(),
            default: None,
        };

        let result = mapper.resolve_string(&value).unwrap();
        assert_eq!(result, "mapped_value");

        std::env::remove_var("TEST_MAPPER_VAR");
    }

    #[test]
    fn test_resolve_typed_u16() {
        let mapper = DtoMapper::new();
        let value = ConfigValue::Static(5432u16);

        let result = mapper.resolve_typed(&value).unwrap();
        assert_eq!(result, 5432u16);
    }

    #[test]
    fn test_resolve_typed_u16_from_env() {
        std::env::set_var("TEST_PORT", "8080");

        let mapper = DtoMapper::new();
        let value: ConfigValue<u16> = ConfigValue::EnvironmentVariable {
            name: "TEST_PORT".to_string(),
            default: None,
        };

        let result = mapper.resolve_typed(&value).unwrap();
        assert_eq!(result, 8080u16);

        std::env::remove_var("TEST_PORT");
    }

    #[test]
    fn test_resolve_typed_parse_error() {
        std::env::set_var("TEST_INVALID_PORT", "not_a_number");

        let mapper = DtoMapper::new();
        let value: ConfigValue<u16> = ConfigValue::EnvironmentVariable {
            name: "TEST_INVALID_PORT".to_string(),
            default: None,
        };

        let result = mapper.resolve_typed(&value);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ResolverError::ParseError(_)));

        std::env::remove_var("TEST_INVALID_PORT");
    }

    #[test]
    fn test_resolve_optional_some() {
        let mapper = DtoMapper::new();
        let value = Some(ConfigValue::Static("test".to_string()));

        let result = mapper.resolve_optional(&value).unwrap();
        assert_eq!(result, Some("test".to_string()));
    }

    #[test]
    fn test_resolve_optional_none() {
        let mapper = DtoMapper::new();
        let value: Option<ConfigValue<String>> = None;

        let result = mapper.resolve_optional(&value).unwrap();
        assert_eq!(result, None);
    }
}
