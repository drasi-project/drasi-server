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

//! Identity provider configuration DTOs.
//!
//! This module contains configuration types for authentication identity providers
//! that can be used by sources and reactions to authenticate with external systems.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::ConfigValue;

/// Identity provider configuration with kind discriminator.
///
/// Uses a custom deserializer to handle the `kind` field and validate unknown fields.
/// The inner config DTOs use `#[serde(deny_unknown_fields)]` to catch typos.
///
/// # Example YAML
///
/// ```yaml
/// identityProviders:
///   - kind: password
///     id: local-db
///     username: myuser
///     password: ${DB_PASSWORD}
///
///   - kind: azure
///     id: azure-ad
///     username: myapp@tenant.onmicrosoft.com
///     authenticationMode: workloadIdentity
///     scope: https://ossrdbms-aad.database.windows.net/.default
///
///   - kind: aws
///     id: aws-iam
///     username: myuser
///     hostname: mydb.us-east-1.rds.amazonaws.com
///     port: 5432
///     region: us-east-1
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum IdentityProviderConfig {
    /// Password-based authentication
    #[serde(rename = "password")]
    Password {
        /// Unique identifier for this identity provider
        id: String,
        #[serde(flatten)]
        config: PasswordIdentityProviderConfigDto,
    },
    /// Azure AD authentication
    #[cfg(feature = "azure-identity")]
    #[serde(rename = "azure")]
    Azure {
        /// Unique identifier for this identity provider
        id: String,
        #[serde(flatten)]
        config: AzureIdentityProviderConfigDto,
    },
    /// AWS IAM authentication
    #[cfg(feature = "aws-identity")]
    #[serde(rename = "aws")]
    Aws {
        /// Unique identifier for this identity provider
        id: String,
        #[serde(flatten)]
        config: AwsIdentityProviderConfigDto,
    },
}

// Known identity provider kinds for error messages
const IDENTITY_PROVIDER_KINDS: &[&str] = &[
    "password",
    #[cfg(feature = "azure-identity")]
    "azure",
    #[cfg(feature = "aws-identity")]
    "aws",
];

impl<'de> Deserialize<'de> for IdentityProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IdentityProviderConfigVisitor;

        impl<'de> Visitor<'de> for IdentityProviderConfigVisitor {
            type Value = IdentityProviderConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an identity provider configuration with 'kind' and 'id' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Storage for common fields
                let mut kind: Option<String> = None;
                let mut id: Option<String> = None;

                // Collect remaining fields for the inner config
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
                        // Collect all other fields for the inner config
                        other => {
                            let value: serde_json::Value = map.next_value()?;
                            remaining.insert(other.to_string(), value);
                        }
                    }
                }

                // Validate required fields
                let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;

                let remaining_value = serde_json::Value::Object(remaining);

                match kind.as_str() {
                    "password" => {
                        let config: PasswordIdentityProviderConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in identityProvider '{id}' (kind=password): {e}"
                                ))
                            })?;
                        Ok(IdentityProviderConfig::Password { id, config })
                    }
                    #[cfg(feature = "azure-identity")]
                    "azure" => {
                        let config: AzureIdentityProviderConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in identityProvider '{id}' (kind=azure): {e}"
                                ))
                            })?;
                        Ok(IdentityProviderConfig::Azure { id, config })
                    }
                    #[cfg(feature = "aws-identity")]
                    "aws" => {
                        let config: AwsIdentityProviderConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in identityProvider '{id}' (kind=aws): {e}"
                                ))
                            })?;
                        Ok(IdentityProviderConfig::Aws { id, config })
                    }
                    unknown => Err(de::Error::unknown_variant(unknown, IDENTITY_PROVIDER_KINDS)),
                }
            }
        }

        deserializer.deserialize_map(IdentityProviderConfigVisitor)
    }
}

impl IdentityProviderConfig {
    /// Get the identity provider ID
    pub fn id(&self) -> &str {
        match self {
            IdentityProviderConfig::Password { id, .. } => id,
            #[cfg(feature = "azure-identity")]
            IdentityProviderConfig::Azure { id, .. } => id,
            #[cfg(feature = "aws-identity")]
            IdentityProviderConfig::Aws { id, .. } => id,
        }
    }

    /// Get a display name for this identity provider type
    pub fn kind(&self) -> &str {
        match self {
            IdentityProviderConfig::Password { .. } => "password",
            #[cfg(feature = "azure-identity")]
            IdentityProviderConfig::Azure { .. } => "azure",
            #[cfg(feature = "aws-identity")]
            IdentityProviderConfig::Aws { .. } => "aws",
        }
    }
}

// =============================================================================
// Password Identity Provider DTO
// =============================================================================

/// Password-based identity provider configuration DTO.
///
/// This is the simplest form of authentication using a username and password.
///
/// # Example YAML
///
/// ```yaml
/// kind: password
/// id: local-db
/// username: myuser
/// password: ${DB_PASSWORD}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordIdentityProviderConfigDto {
    /// Username for authentication
    pub username: ConfigValue<String>,
    /// Password for authentication
    /// Supports environment variables: ${DB_PASSWORD}
    pub password: ConfigValue<String>,
}

// =============================================================================
// Azure Identity Provider DTO
// =============================================================================

/// Azure AD authentication mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AzureAuthenticationMode {
    /// System-assigned managed identity
    ManagedIdentity,
    /// AKS Workload Identity (recommended for Kubernetes)
    WorkloadIdentity,
    /// Default credentials chain (Azure CLI, Developer CLI, PowerShell)
    DefaultCredentials,
}

impl Default for AzureAuthenticationMode {
    fn default() -> Self {
        Self::WorkloadIdentity
    }
}

/// Azure AD identity provider configuration DTO.
///
/// Supports multiple authentication modes including Managed Identity,
/// Workload Identity (recommended for AKS), and default credential chain.
///
/// # Example YAML
///
/// ```yaml
/// kind: azure
/// id: azure-ad
/// username: myapp@tenant.onmicrosoft.com
/// authenticationMode: workloadIdentity
/// scope: https://ossrdbms-aad.database.windows.net/.default
/// clientId: ${AZURE_CLIENT_ID}  # Optional: for user-assigned managed identity
/// ```
#[cfg(feature = "azure-identity")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AzureIdentityProviderConfigDto {
    /// Username for database authentication (Azure AD user principal name)
    pub username: ConfigValue<String>,
    /// Authentication mode (default: workloadIdentity)
    #[serde(default)]
    pub authentication_mode: AzureAuthenticationMode,
    /// Optional client ID for user-assigned managed identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<ConfigValue<String>>,
    /// Optional token scope (default: Azure Database for PostgreSQL/MySQL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ConfigValue<String>>,
}

// =============================================================================
// AWS Identity Provider DTO
// =============================================================================

/// AWS IAM authentication mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AwsAuthenticationMode {
    /// Use AWS credential chain from environment
    DefaultCredentials,
    /// Assume an IAM role
    AssumeRole,
}

impl Default for AwsAuthenticationMode {
    fn default() -> Self {
        Self::DefaultCredentials
    }
}

/// AWS IAM identity provider configuration DTO.
///
/// Supports authentication using AWS credential chain or assuming an IAM role.
///
/// # Example YAML
///
/// ```yaml
/// kind: aws
/// id: aws-iam
/// username: myuser
/// hostname: mydb.us-east-1.rds.amazonaws.com
/// port: 5432
/// region: us-east-1
/// authenticationMode: defaultCredentials
///
/// # Or with role assumption:
/// kind: aws
/// id: aws-iam-role
/// username: myuser
/// hostname: mydb.us-east-1.rds.amazonaws.com
/// port: 5432
/// region: us-east-1
/// authenticationMode: assumeRole
/// roleArn: arn:aws:iam::123456789012:role/MyDatabaseRole
/// sessionName: drasi-session
/// ```
#[cfg(feature = "aws-identity")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AwsIdentityProviderConfigDto {
    /// Username for database authentication
    pub username: ConfigValue<String>,
    /// Database hostname (required for AWS IAM token generation)
    pub hostname: ConfigValue<String>,
    /// Database port (default: 5432 for PostgreSQL)
    #[serde(default = "default_port")]
    pub port: ConfigValue<u16>,
    /// AWS region (e.g., us-east-1)
    pub region: ConfigValue<String>,
    /// Authentication mode (default: defaultCredentials)
    #[serde(default)]
    pub authentication_mode: AwsAuthenticationMode,
    /// IAM role ARN to assume (required when authenticationMode is assumeRole)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_arn: Option<ConfigValue<String>>,
    /// Session name for role assumption (required when authenticationMode is assumeRole)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<ConfigValue<String>>,
}

fn default_port() -> ConfigValue<u16> {
    ConfigValue::Static(5432)
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // PasswordIdentityProvider Tests
    // =========================================================================

    #[test]
    fn test_password_provider_deserialize_valid() {
        let json = r#"{
            "kind": "password",
            "id": "local-db",
            "username": "myuser",
            "password": "mypassword"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(provider.id(), "local-db");
        assert_eq!(provider.kind(), "password");
        assert!(matches!(provider, IdentityProviderConfig::Password { .. }));
    }

    #[test]
    fn test_password_provider_with_env_var() {
        let json = r#"{
            "kind": "password",
            "id": "local-db",
            "username": "myuser",
            "password": "${DB_PASSWORD}"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Password { config, .. } = provider else {
            panic!("Expected Password variant");
        };
        assert!(matches!(
            config.password,
            ConfigValue::EnvironmentVariable { .. }
        ));
    }

    #[test]
    fn test_password_provider_yaml() {
        let yaml = r#"
kind: password
id: yaml-provider
username: testuser
password: testpass
"#;

        let provider: IdentityProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(provider.id(), "yaml-provider");
    }

    // =========================================================================
    // Azure Identity Provider Tests
    // =========================================================================

    #[cfg(feature = "azure-identity")]
    #[test]
    fn test_azure_provider_deserialize_minimal() {
        let json = r#"{
            "kind": "azure",
            "id": "azure-ad",
            "username": "myapp@tenant.onmicrosoft.com"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(provider.id(), "azure-ad");
        assert_eq!(provider.kind(), "azure");
        assert!(matches!(provider, IdentityProviderConfig::Azure { .. }));
    }

    #[cfg(feature = "azure-identity")]
    #[test]
    fn test_azure_provider_with_workload_identity() {
        let json = r#"{
            "kind": "azure",
            "id": "azure-workload",
            "username": "myapp@tenant.onmicrosoft.com",
            "authenticationMode": "workloadIdentity"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Azure { config, .. } = provider else {
            panic!("Expected Azure variant");
        };
        assert_eq!(
            config.authentication_mode,
            AzureAuthenticationMode::WorkloadIdentity
        );
    }

    #[cfg(feature = "azure-identity")]
    #[test]
    fn test_azure_provider_with_managed_identity_and_client_id() {
        let json = r#"{
            "kind": "azure",
            "id": "azure-managed",
            "username": "myapp@tenant.onmicrosoft.com",
            "authenticationMode": "managedIdentity",
            "clientId": "${AZURE_CLIENT_ID}"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Azure { config, .. } = provider else {
            panic!("Expected Azure variant");
        };
        assert_eq!(
            config.authentication_mode,
            AzureAuthenticationMode::ManagedIdentity
        );
        assert!(config.client_id.is_some());
    }

    #[cfg(feature = "azure-identity")]
    #[test]
    fn test_azure_provider_with_custom_scope() {
        let json = r#"{
            "kind": "azure",
            "id": "azure-custom",
            "username": "myapp@tenant.onmicrosoft.com",
            "scope": "https://custom.scope/.default"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Azure { config, .. } = provider else {
            panic!("Expected Azure variant");
        };
        assert!(config.scope.is_some());
    }

    // =========================================================================
    // AWS Identity Provider Tests
    // =========================================================================

    #[cfg(feature = "aws-identity")]
    #[test]
    fn test_aws_provider_deserialize_minimal() {
        let json = r#"{
            "kind": "aws",
            "id": "aws-iam",
            "username": "myuser",
            "hostname": "mydb.us-east-1.rds.amazonaws.com",
            "region": "us-east-1"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(provider.id(), "aws-iam");
        assert_eq!(provider.kind(), "aws");
        assert!(matches!(provider, IdentityProviderConfig::Aws { .. }));
    }

    #[cfg(feature = "aws-identity")]
    #[test]
    fn test_aws_provider_with_custom_port() {
        let json = r#"{
            "kind": "aws",
            "id": "aws-custom-port",
            "username": "myuser",
            "hostname": "mydb.us-east-1.rds.amazonaws.com",
            "port": 3306,
            "region": "us-east-1"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Aws { config, .. } = provider else {
            panic!("Expected Aws variant");
        };
        assert_eq!(config.port, ConfigValue::Static(3306));
    }

    #[cfg(feature = "aws-identity")]
    #[test]
    fn test_aws_provider_with_assume_role() {
        let json = r#"{
            "kind": "aws",
            "id": "aws-role",
            "username": "myuser",
            "hostname": "mydb.us-east-1.rds.amazonaws.com",
            "region": "us-east-1",
            "authenticationMode": "assumeRole",
            "roleArn": "arn:aws:iam::123456789012:role/MyRole",
            "sessionName": "drasi-session"
        }"#;

        let provider: IdentityProviderConfig = serde_json::from_str(json).unwrap();
        let IdentityProviderConfig::Aws { config, .. } = provider else {
            panic!("Expected Aws variant");
        };
        assert_eq!(
            config.authentication_mode,
            AwsAuthenticationMode::AssumeRole
        );
        assert!(config.role_arn.is_some());
        assert!(config.session_name.is_some());
    }

    // =========================================================================
    // Common Tests
    // =========================================================================

    #[test]
    fn test_identity_provider_missing_kind() {
        let json = r#"{
            "id": "test-provider",
            "username": "user"
        }"#;

        let result: Result<IdentityProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("kind"), "Error should mention 'kind': {err}");
    }

    #[test]
    fn test_identity_provider_missing_id() {
        let json = r#"{
            "kind": "password",
            "username": "user",
            "password": "pass"
        }"#;

        let result: Result<IdentityProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("id"), "Error should mention 'id': {err}");
    }

    #[test]
    fn test_identity_provider_unknown_kind() {
        let json = r#"{
            "kind": "unknown-provider",
            "id": "test",
            "username": "user"
        }"#;

        let result: Result<IdentityProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown-provider") || err.contains("unknown variant"),
            "Error should mention unknown kind: {err}"
        );
    }

    #[test]
    fn test_identity_provider_unknown_field_rejected() {
        let json = r#"{
            "kind": "password",
            "id": "test",
            "username": "user",
            "password": "pass",
            "unknownField": "value"
        }"#;

        let result: Result<IdentityProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknownField") || err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_identity_provider_serialize_deserialize_roundtrip() {
        let original = IdentityProviderConfig::Password {
            id: "roundtrip-test".to_string(),
            config: PasswordIdentityProviderConfigDto {
                username: ConfigValue::Static("testuser".to_string()),
                password: ConfigValue::Static("testpass".to_string()),
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: IdentityProviderConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id(), "roundtrip-test");
        assert_eq!(deserialized.kind(), "password");
    }
}
