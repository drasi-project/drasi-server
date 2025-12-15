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

use serde::{Deserialize, Serialize};

/// Local copy of PostgreSQL source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostgresSourceConfigDto {
    #[serde(default = "default_postgres_host")]
    pub host: String,
    #[serde(default = "default_postgres_port")]
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default = "default_slot_name")]
    pub slot_name: String,
    #[serde(default = "default_publication_name")]
    pub publication_name: String,
    #[serde(default)]
    pub ssl_mode: SslModeDto,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableKeyConfigDto {
    pub table: String,
    pub key_columns: Vec<String>,
}

fn default_postgres_host() -> String {
    "localhost".to_string()
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_slot_name() -> String {
    "drasi_slot".to_string()
}

fn default_publication_name() -> String {
    "drasi_publication".to_string()
}
