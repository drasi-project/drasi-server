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

//! PostgreSQL source configuration mapper.

use crate::api::models::PostgresSourceConfigDto;
use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use drasi_source_postgres::{PostgresSourceConfig, TableKeyConfig};

pub struct PostgresConfigMapper;

impl ConfigMapper<PostgresSourceConfigDto, PostgresSourceConfig> for PostgresConfigMapper {
    fn map(&self, dto: &PostgresSourceConfigDto, resolver: &DtoMapper) -> Result<PostgresSourceConfig, MappingError> {
        Ok(PostgresSourceConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            database: resolver.resolve_string(&dto.database)?,
            user: resolver.resolve_string(&dto.user)?,
            password: resolver.resolve_string(&dto.password)?,
            tables: dto.tables.clone(),
            slot_name: dto.slot_name.clone(),
            publication_name: dto.publication_name.clone(),
            ssl_mode: resolver.resolve_typed::<crate::api::models::SslModeDto>(&dto.ssl_mode)?.into(),
            table_keys: dto.table_keys.iter().map(|tk| TableKeyConfig {
                table: tk.table.clone(),
                key_columns: tk.key_columns.clone(),
            }).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{ConfigValue, SslModeDto};
    
    #[test]
    fn test_postgres_mapper() {
        std::env::set_var("TEST_PG_PASSWORD", "secret123");
        
        let dto = PostgresSourceConfigDto {
            host: ConfigValue::Static("localhost".to_string()),
            port: ConfigValue::Static(5432),
            database: ConfigValue::Static("testdb".to_string()),
            user: ConfigValue::Static("testuser".to_string()),
            password: ConfigValue::EnvironmentVariable {
                name: "TEST_PG_PASSWORD".to_string(),
                default: None,
            },
            tables: vec!["users".to_string()],
            slot_name: "test_slot".to_string(),
            publication_name: "test_pub".to_string(),
            ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
            table_keys: vec![],
        };
        
        let mapper = DtoMapper::new();
        let postgres_mapper = PostgresConfigMapper;
        let result = postgres_mapper.map(&dto, &mapper).unwrap();
        
        assert_eq!(result.host, "localhost");
        assert_eq!(result.port, 5432);
        assert_eq!(result.database, "testdb");
        assert_eq!(result.user, "testuser");
        assert_eq!(result.password, "secret123");
        assert_eq!(result.tables, vec!["users".to_string()]);
        
        std::env::remove_var("TEST_PG_PASSWORD");
    }
}
