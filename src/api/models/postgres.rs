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

//! PostgreSQL source configuration DTOs.

use crate::api::models::ConfigValue;
use drasi_source_postgres::SslMode;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Local copy of PostgreSQL source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PostgresSourceConfigDto {
    #[serde(default = "default_postgres_host")]
    pub host: ConfigValue<String>,
    #[serde(default = "default_postgres_port")]
    pub port: ConfigValue<u16>,
    pub database: ConfigValue<String>,
    pub user: ConfigValue<String>,
    #[serde(default = "default_password")]
    pub password: ConfigValue<String>,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default = "default_slot_name")]
    pub slot_name: String,
    #[serde(default = "default_publication_name")]
    pub publication_name: String,
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: ConfigValue<SslModeDto>,
    #[serde(default)]
    pub table_keys: Vec<TableKeyConfigDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SslModeDto {
    Disable,
    Prefer,
    Require,
}

impl Default for SslModeDto {
    fn default() -> Self {
        Self::Prefer
    }
}

impl FromStr for SslModeDto {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disable" => Ok(SslModeDto::Disable),
            "prefer" => Ok(SslModeDto::Prefer),
            "require" => Ok(SslModeDto::Require),
            _ => Err(format!("Invalid SSL mode: {}", s)),
        }
    }
}

impl From<SslModeDto> for SslMode {
    fn from(dto: SslModeDto) -> Self {
        match dto {
            SslModeDto::Disable => SslMode::Disable,
            SslModeDto::Prefer => SslMode::Prefer,
            SslModeDto::Require => SslMode::Require,
        }
    }
}

impl From<SslMode> for SslModeDto {
    fn from(mode: SslMode) -> Self {
        match mode {
            SslMode::Disable => SslModeDto::Disable,
            SslMode::Prefer => SslModeDto::Prefer,
            SslMode::Require => SslModeDto::Require,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableKeyConfigDto {
    pub table: String,
    pub key_columns: Vec<String>,
}

fn default_postgres_host() -> ConfigValue<String> {
    ConfigValue::Static("localhost".to_string())
}

fn default_postgres_port() -> ConfigValue<u16> {
    ConfigValue::Static(5432)
}

fn default_slot_name() -> String {
    "drasi_slot".to_string()
}

fn default_publication_name() -> String {
    "drasi_publication".to_string()
}

fn default_password() -> ConfigValue<String> {
    ConfigValue::Static(String::new())
}

fn default_ssl_mode() -> ConfigValue<SslModeDto> {
    ConfigValue::Static(SslModeDto::default())
}
