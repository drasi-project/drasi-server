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

//! Integration tests for ConfigValue end-to-end functionality

#[cfg(test)]
mod tests {
    use drasi_server::api::mappings::{ConfigMapper, DtoMapper, PostgresConfigMapper};
    use drasi_server::api::models::{ConfigValue, PostgresSourceConfigDto, SslModeDto};

    #[test]
    fn test_postgres_with_static_values() {
        let dto = PostgresSourceConfigDto {
            host: ConfigValue::Static("db.example.com".to_string()),
            port: ConfigValue::Static(5433),
            database: ConfigValue::Static("production".to_string()),
            user: ConfigValue::Static("app_user".to_string()),
            password: ConfigValue::Static("secret123".to_string()),
            tables: vec!["users".to_string(), "orders".to_string()],
            slot_name: "my_slot".to_string(),
            publication_name: "my_pub".to_string(),
            ssl_mode: ConfigValue::Static(SslModeDto::Require),
            table_keys: vec![],
        };

        let mapper = DtoMapper::new();
        let postgres_mapper = PostgresConfigMapper;
        let config = postgres_mapper.map(&dto, &mapper).unwrap();

        assert_eq!(config.host, "db.example.com");
        assert_eq!(config.port, 5433);
        assert_eq!(config.database, "production");
        assert_eq!(config.user, "app_user");
        assert_eq!(config.password, "secret123");
    }

    #[test]
    fn test_postgres_with_environment_variables() {
        // Set up environment variables
        std::env::set_var("TEST_DB_HOST", "env-host.com");
        std::env::set_var("TEST_DB_PORT", "5434");
        std::env::set_var("TEST_DB_NAME", "env_database");
        std::env::set_var("TEST_DB_USER", "env_user");
        std::env::set_var("TEST_DB_PASSWORD", "env_secret");
        std::env::set_var("TEST_SSL_MODE", "require");

        let dto = PostgresSourceConfigDto {
            host: ConfigValue::EnvironmentVariable {
                name: "TEST_DB_HOST".to_string(),
                default: None,
            },
            port: ConfigValue::EnvironmentVariable {
                name: "TEST_DB_PORT".to_string(),
                default: Some("5432".to_string()),
            },
            database: ConfigValue::EnvironmentVariable {
                name: "TEST_DB_NAME".to_string(),
                default: None,
            },
            user: ConfigValue::EnvironmentVariable {
                name: "TEST_DB_USER".to_string(),
                default: None,
            },
            password: ConfigValue::EnvironmentVariable {
                name: "TEST_DB_PASSWORD".to_string(),
                default: None,
            },
            tables: vec![],
            slot_name: "slot".to_string(),
            publication_name: "pub".to_string(),
            ssl_mode: ConfigValue::EnvironmentVariable {
                name: "TEST_SSL_MODE".to_string(),
                default: Some("prefer".to_string()),
            },
            table_keys: vec![],
        };

        let mapper = DtoMapper::new();
        let postgres_mapper = PostgresConfigMapper;
        let config = postgres_mapper.map(&dto, &mapper).unwrap();

        assert_eq!(config.host, "env-host.com");
        assert_eq!(config.port, 5434);
        assert_eq!(config.database, "env_database");
        assert_eq!(config.user, "env_user");
        assert_eq!(config.password, "env_secret");

        // Clean up
        std::env::remove_var("TEST_DB_HOST");
        std::env::remove_var("TEST_DB_PORT");
        std::env::remove_var("TEST_DB_NAME");
        std::env::remove_var("TEST_DB_USER");
        std::env::remove_var("TEST_DB_PASSWORD");
        std::env::remove_var("TEST_SSL_MODE");
    }

    #[test]
    fn test_postgres_with_defaults() {
        // Don't set environment variables, rely on defaults
        let dto = PostgresSourceConfigDto {
            host: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_HOST".to_string(),
                default: Some("default-host.com".to_string()),
            },
            port: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_PORT".to_string(),
                default: Some("9999".to_string()),
            },
            database: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_DB".to_string(),
                default: Some("default_db".to_string()),
            },
            user: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_USER".to_string(),
                default: Some("default_user".to_string()),
            },
            password: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_PASSWORD".to_string(),
                default: Some("default_pass".to_string()),
            },
            tables: vec![],
            slot_name: "slot".to_string(),
            publication_name: "pub".to_string(),
            ssl_mode: ConfigValue::EnvironmentVariable {
                name: "NONEXISTENT_SSL".to_string(),
                default: Some("disable".to_string()),
            },
            table_keys: vec![],
        };

        let mapper = DtoMapper::new();
        let postgres_mapper = PostgresConfigMapper;
        let config = postgres_mapper.map(&dto, &mapper).unwrap();

        assert_eq!(config.host, "default-host.com");
        assert_eq!(config.port, 9999);
        assert_eq!(config.database, "default_db");
        assert_eq!(config.user, "default_user");
        assert_eq!(config.password, "default_pass");
    }

    #[test]
    fn test_postgres_mixed_static_and_env() {
        std::env::set_var("TEST_MIXED_PASSWORD", "secure_password");

        let dto = PostgresSourceConfigDto {
            host: ConfigValue::Static("localhost".to_string()),
            port: ConfigValue::Static(5432),
            database: ConfigValue::Static("testdb".to_string()),
            user: ConfigValue::Static("testuser".to_string()),
            password: ConfigValue::EnvironmentVariable {
                name: "TEST_MIXED_PASSWORD".to_string(),
                default: None,
            },
            tables: vec!["table1".to_string()],
            slot_name: "slot".to_string(),
            publication_name: "pub".to_string(),
            ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
            table_keys: vec![],
        };

        let mapper = DtoMapper::new();
        let postgres_mapper = PostgresConfigMapper;
        let config = postgres_mapper.map(&dto, &mapper).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.password, "secure_password");

        std::env::remove_var("TEST_MIXED_PASSWORD");
    }

    #[test]
    fn test_deserialization_from_yaml() {
        let yaml = r#"
host: "yaml-host.com"
port: 5432
database: "${DB_NAME:-default_db}"
user: "yaml_user"
password:
  kind: EnvironmentVariable
  name: DB_PASSWORD
  default: "default_password"
tables:
  - users
  - orders
slotName: "yaml_slot"
publicationName: "yaml_pub"
sslMode:
  kind: EnvironmentVariable
  name: SSL_MODE
  default: "prefer"
tableKeys: []
        "#;

        let dto: PostgresSourceConfigDto = serde_yaml::from_str(yaml).unwrap();

        // Check deserialization worked correctly
        match dto.host {
            ConfigValue::Static(ref s) => assert_eq!(s, "yaml-host.com"),
            _ => panic!("Expected static host"),
        }

        match dto.database {
            ConfigValue::EnvironmentVariable {
                ref name,
                ref default,
            } => {
                assert_eq!(name, "DB_NAME");
                assert_eq!(default.as_deref(), Some("default_db"));
            }
            _ => panic!("Expected environment variable for database"),
        }

        match dto.password {
            ConfigValue::EnvironmentVariable {
                ref name,
                ref default,
            } => {
                assert_eq!(name, "DB_PASSWORD");
                assert_eq!(default.as_deref(), Some("default_password"));
            }
            _ => panic!("Expected environment variable for password"),
        }

        match dto.ssl_mode {
            ConfigValue::EnvironmentVariable {
                ref name,
                ref default,
            } => {
                assert_eq!(name, "SSL_MODE");
                assert_eq!(default.as_deref(), Some("prefer"));
            }
            _ => panic!("Expected environment variable for ssl_mode"),
        }
    }
}
