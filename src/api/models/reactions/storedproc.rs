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

//! Stored procedure reaction configuration DTOs for PostgreSQL, MySQL, and MSSQL.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Template Configuration Types
// =============================================================================

/// Template specification for a stored procedure command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredProcTemplateSpecDto {
    /// The stored procedure command template
    /// Supports @after.field and @before.field syntax
    /// Example: "CALL add_user(@after.id, @after.name, @after.email)"
    pub command: ConfigValue<String>,
}

/// Query-specific configuration for ADD/UPDATE/DELETE operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredProcQueryConfigDto {
    /// Template for ADD operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<StoredProcTemplateSpecDto>,

    /// Template for UPDATE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<StoredProcTemplateSpecDto>,

    /// Template for DELETE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<StoredProcTemplateSpecDto>,
}

// =============================================================================
// PostgreSQL Stored Procedure Reaction
// =============================================================================

/// PostgreSQL stored procedure reaction configuration DTO
///
/// Executes PostgreSQL stored procedures in response to query result changes.
///
/// # Example YAML
///
/// ```yaml
/// kind: storedprocPostgres
/// id: postgres-sync
/// queries:
///   - user-query
/// config:
///   hostname: localhost
///   port: 5432
///   database: mydb
///   ssl: false
///   identityProviderId: postgres-password  # Reference to identity provider
///   commandTimeoutMs: 5000
///   retryAttempts: 3
///   defaultTemplate:
///     added:
///       command: "CALL add_user(@after.id, @after.name)"
///     updated:
///       command: "CALL update_user(@after.id, @after.name)"
///     deleted:
///       command: "CALL delete_user(@before.id)"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostgresStoredProcReactionConfigDto {
    /// Database hostname or IP address
    #[serde(default = "default_hostname")]
    pub hostname: ConfigValue<String>,

    /// Database port (default: 5432)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<ConfigValue<u16>>,

    /// Database name
    pub database: ConfigValue<String>,

    /// Enable SSL/TLS
    #[serde(default)]
    pub ssl: bool,

    /// Identity provider ID for authentication
    /// References an identity provider defined in the identityProviders section
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_provider_id: Option<String>,

    /// Legacy: Database user (deprecated, use identity_provider_id instead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<ConfigValue<String>>,

    /// Legacy: Database password (deprecated, use identity_provider_id instead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<ConfigValue<String>>,

    /// Query-specific template configurations
    #[serde(default)]
    pub routes: HashMap<String, StoredProcQueryConfigDto>,

    /// Default template used when no query-specific route is defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_template: Option<StoredProcQueryConfigDto>,

    /// Command timeout in milliseconds (default: 5000)
    #[serde(default = "default_timeout_ms")]
    pub command_timeout_ms: ConfigValue<u64>,

    /// Number of retry attempts on failure (default: 3)
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: ConfigValue<u32>,
}

// =============================================================================
// MySQL Stored Procedure Reaction
// =============================================================================

/// MySQL stored procedure reaction configuration DTO
///
/// Executes MySQL stored procedures in response to query result changes.
///
/// # Example YAML
///
/// ```yaml
/// kind: storedprocMysql
/// id: mysql-sync
/// queries:
///   - user-query
/// config:
///   hostname: localhost
///   port: 3306
///   database: mydb
///   ssl: false
///   identityProviderId: mysql-password
///   commandTimeoutMs: 5000
///   retryAttempts: 3
///   defaultTemplate:
///     added:
///       command: "CALL add_user(@after.id, @after.name)"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MysqlStoredProcReactionConfigDto {
    /// Database hostname or IP address
    #[serde(default = "default_hostname")]
    pub hostname: ConfigValue<String>,

    /// Database port (default: 3306)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<ConfigValue<u16>>,

    /// Database name
    pub database: ConfigValue<String>,

    /// Enable SSL/TLS
    #[serde(default)]
    pub ssl: bool,

    /// Identity provider ID for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_provider_id: Option<String>,

    /// Legacy: Database user (deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<ConfigValue<String>>,

    /// Legacy: Database password (deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<ConfigValue<String>>,

    /// Query-specific template configurations
    #[serde(default)]
    pub routes: HashMap<String, StoredProcQueryConfigDto>,

    /// Default template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_template: Option<StoredProcQueryConfigDto>,

    /// Command timeout in milliseconds (default: 5000)
    #[serde(default = "default_timeout_ms")]
    pub command_timeout_ms: ConfigValue<u64>,

    /// Number of retry attempts on failure (default: 3)
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: ConfigValue<u32>,
}

// =============================================================================
// MSSQL Stored Procedure Reaction
// =============================================================================

/// MSSQL stored procedure reaction configuration DTO
///
/// Executes MSSQL stored procedures in response to query result changes.
///
/// # Example YAML
///
/// ```yaml
/// kind: storedprocMssql
/// id: mssql-sync
/// queries:
///   - user-query
/// config:
///   hostname: localhost
///   port: 1433
///   database: mydb
///   ssl: false
///   identityProviderId: mssql-password
///   commandTimeoutMs: 5000
///   retryAttempts: 3
///   defaultTemplate:
///     added:
///       command: "EXEC add_user @after.id, @after.name"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MssqlStoredProcReactionConfigDto {
    /// Database hostname or IP address
    #[serde(default = "default_hostname")]
    pub hostname: ConfigValue<String>,

    /// Database port (default: 1433)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<ConfigValue<u16>>,

    /// Database name
    pub database: ConfigValue<String>,

    /// Enable SSL/TLS
    #[serde(default)]
    pub ssl: bool,

    /// Identity provider ID for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_provider_id: Option<String>,

    /// Legacy: Database user (deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<ConfigValue<String>>,

    /// Legacy: Database password (deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<ConfigValue<String>>,

    /// Query-specific template configurations
    #[serde(default)]
    pub routes: HashMap<String, StoredProcQueryConfigDto>,

    /// Default template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_template: Option<StoredProcQueryConfigDto>,

    /// Command timeout in milliseconds (default: 5000)
    #[serde(default = "default_timeout_ms")]
    pub command_timeout_ms: ConfigValue<u64>,

    /// Number of retry attempts on failure (default: 3)
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: ConfigValue<u32>,
}

// =============================================================================
// Default Value Functions
// =============================================================================

fn default_hostname() -> ConfigValue<String> {
    ConfigValue::Static("localhost".to_string())
}

fn default_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(5000)
}

fn default_retry_attempts() -> ConfigValue<u32> {
    ConfigValue::Static(3)
}
