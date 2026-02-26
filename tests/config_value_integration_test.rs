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
    use drasi_server::api::mappings::DtoMapper;
    use drasi_server::api::models::ConfigValue;
    use serde_json::json;

    #[test]
    fn test_config_value_static_string_resolution() {
        let value: ConfigValue<String> = serde_json::from_value(json!("db.example.com")).unwrap();
        let mapper = DtoMapper::new();
        let resolved = mapper.resolve_string(&value).unwrap();
        assert_eq!(resolved, "db.example.com");
    }

    #[test]
    fn test_config_value_static_typed_resolution() {
        let value: ConfigValue<u16> = serde_json::from_value(json!(5433)).unwrap();
        let mapper = DtoMapper::new();
        let resolved = mapper.resolve_typed(&value).unwrap();
        assert_eq!(resolved, 5433);
    }

    #[test]
    fn test_config_value_environment_variable_resolution() {
        std::env::set_var("TEST_CV_HOST", "env-host.com");
        std::env::set_var("TEST_CV_PORT", "5434");

        let host: ConfigValue<String> =
            serde_json::from_value(json!({"kind": "EnvironmentVariable", "name": "TEST_CV_HOST"}))
                .unwrap();
        let port: ConfigValue<u16> = serde_json::from_value(
            json!({"kind": "EnvironmentVariable", "name": "TEST_CV_PORT", "default": "5432"}),
        )
        .unwrap();

        let mapper = DtoMapper::new();
        assert_eq!(mapper.resolve_string(&host).unwrap(), "env-host.com");
        assert_eq!(mapper.resolve_typed(&port).unwrap(), 5434);

        std::env::remove_var("TEST_CV_HOST");
        std::env::remove_var("TEST_CV_PORT");
    }

    #[test]
    fn test_config_value_environment_variable_defaults() {
        let host: ConfigValue<String> = serde_json::from_value(
            json!({"kind": "EnvironmentVariable", "name": "NONEXISTENT_CV_HOST", "default": "default-host.com"}),
        )
        .unwrap();
        let port: ConfigValue<u16> = serde_json::from_value(
            json!({"kind": "EnvironmentVariable", "name": "NONEXISTENT_CV_PORT", "default": "9999"}),
        )
        .unwrap();

        let mapper = DtoMapper::new();
        assert_eq!(mapper.resolve_string(&host).unwrap(), "default-host.com");
        assert_eq!(mapper.resolve_typed(&port).unwrap(), 9999);
    }

    #[test]
    fn test_config_value_mixed_static_and_env() {
        std::env::set_var("TEST_CV_MIXED_PASSWORD", "secure_password");

        let host: ConfigValue<String> = serde_json::from_value(json!("localhost")).unwrap();
        let port: ConfigValue<u16> = serde_json::from_value(json!(5432)).unwrap();
        let password: ConfigValue<String> = serde_json::from_value(
            json!({"kind": "EnvironmentVariable", "name": "TEST_CV_MIXED_PASSWORD"}),
        )
        .unwrap();

        let mapper = DtoMapper::new();
        assert_eq!(mapper.resolve_string(&host).unwrap(), "localhost");
        assert_eq!(mapper.resolve_typed(&port).unwrap(), 5432);
        assert_eq!(mapper.resolve_string(&password).unwrap(), "secure_password");

        std::env::remove_var("TEST_CV_MIXED_PASSWORD");
    }

    #[test]
    fn test_config_value_deserialization_from_yaml() {
        let yaml = r#"
host: "yaml-host.com"
database: "${DB_NAME:-default_db}"
password:
  kind: EnvironmentVariable
  name: DB_PASSWORD
  default: "default_password"
sslMode:
  kind: EnvironmentVariable
  name: SSL_MODE
  default: "prefer"
        "#;

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TestConfig {
            host: ConfigValue<String>,
            database: ConfigValue<String>,
            password: ConfigValue<String>,
            ssl_mode: ConfigValue<String>,
        }

        let config: TestConfig = serde_yaml::from_str(yaml).unwrap();

        match config.host {
            ConfigValue::Static(ref s) => assert_eq!(s, "yaml-host.com"),
            _ => panic!("Expected static host"),
        }

        match config.database {
            ConfigValue::EnvironmentVariable {
                ref name,
                ref default,
            } => {
                assert_eq!(name, "DB_NAME");
                assert_eq!(default.as_deref(), Some("default_db"));
            }
            _ => panic!("Expected environment variable for database"),
        }

        match config.password {
            ConfigValue::EnvironmentVariable {
                ref name,
                ref default,
            } => {
                assert_eq!(name, "DB_PASSWORD");
                assert_eq!(default.as_deref(), Some("default_password"));
            }
            _ => panic!("Expected environment variable for password"),
        }

        match config.ssl_mode {
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
