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

//! Identity provider configuration DTO.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Built-in identity provider kind that does not require a plugin.
///
/// The `password` identity provider is implemented directly in
/// `drasi_lib::identity::PasswordIdentityProvider` and never goes through the
/// plugin registry. All other kinds must be provided by an `identity/*` plugin.
pub const BUILTIN_PASSWORD_KIND: &str = "password";

/// Identity provider configuration with kind discriminator.
///
/// Mirrors the shape of [`SourceConfig`](super::source::SourceConfig) and
/// [`ReactionConfig`](super::reaction::ReactionConfig): the deserializer
/// pulls the common `kind` and `id` fields out of the YAML/JSON map and
/// retains the remaining fields as a `serde_json::Value` to be forwarded
/// to the plugin's `create_identity_provider` factory.
///
/// # Example YAML
///
/// ```yaml
/// identityProviders:
///   - kind: azure
///     id: azure-developer
///     identityName: "user@example.com"
///     authMethod: developer_tools
///
///   - kind: password
///     id: pg-password
///     username: drasi
///     password: ${PG_PASSWORD}
/// ```
#[derive(Debug, Clone)]
pub struct IdentityProviderConfig {
    pub kind: String,
    pub id: String,
    pub config: serde_json::Value,
}

impl IdentityProviderConfig {
    /// Get the identity provider ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the identity provider kind (e.g. `azure`, `password`).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns `true` when this kind is built-in to drasi-lib and does not
    /// require a registered identity provider plugin.
    pub fn is_builtin(&self) -> bool {
        self.kind == BUILTIN_PASSWORD_KIND
    }
}

impl Serialize for IdentityProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        map.serialize_entry("id", &self.id)?;
        if let serde_json::Value::Object(config_map) = &self.config {
            for (k, v) in config_map {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for IdentityProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IdentityProviderConfigVisitor;

        impl<'de> Visitor<'de> for IdentityProviderConfigVisitor {
            type Value = IdentityProviderConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("an identity provider configuration with 'kind' and 'id' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut kind: Option<String> = None;
                let mut id: Option<String> = None;
                let mut remaining = serde_json::Map::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        "id" => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        other => {
                            let value: serde_json::Value = map.next_value()?;
                            remaining.insert(other.to_string(), value);
                        }
                    }
                }

                let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;

                Ok(IdentityProviderConfig {
                    kind,
                    id,
                    config: serde_json::Value::Object(remaining),
                })
            }
        }

        deserializer.deserialize_map(IdentityProviderConfigVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_azure() {
        let yaml = r#"
            kind: azure
            id: azure-dev
            identityName: "user@example.com"
            authMethod: developer_tools
        "#;
        let cfg: IdentityProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.kind(), "azure");
        assert_eq!(cfg.id(), "azure-dev");
        assert!(!cfg.is_builtin());
        assert_eq!(
            cfg.config.get("identityName").and_then(|v| v.as_str()),
            Some("user@example.com")
        );
        assert_eq!(
            cfg.config.get("authMethod").and_then(|v| v.as_str()),
            Some("developer_tools")
        );
    }

    #[test]
    fn deserialize_password_is_builtin() {
        let yaml = r#"
            kind: password
            id: pg
            username: drasi
            password: secret
        "#;
        let cfg: IdentityProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.is_builtin());
        assert_eq!(
            cfg.config.get("username").and_then(|v| v.as_str()),
            Some("drasi")
        );
    }

    #[test]
    fn missing_id_errors() {
        let yaml = "kind: azure\n";
        let err = serde_yaml::from_str::<IdentityProviderConfig>(yaml).unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn missing_kind_errors() {
        let yaml = "id: x\n";
        let err = serde_yaml::from_str::<IdentityProviderConfig>(yaml).unwrap_err();
        assert!(err.to_string().contains("kind"));
    }
}
