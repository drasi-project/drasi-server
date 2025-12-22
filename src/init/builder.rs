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

//! Configuration building logic for init command.

use anyhow::Result;

use drasi_server::api::models::{ConfigValue, ReactionConfig, SourceConfig};
use drasi_server::DrasiServerConfig;

use super::prompts::ServerSettings;

/// Build a complete DrasiServerConfig from user selections.
pub fn build_config(
    server_settings: ServerSettings,
    sources: Vec<SourceConfig>,
    reactions: Vec<ReactionConfig>,
) -> DrasiServerConfig {
    // Generate a unique server ID
    let server_id = uuid::Uuid::new_v4().to_string();

    // Create a sample query if we have sources
    let queries = if !sources.is_empty() {
        let source_id = sources
            .first()
            .map(|s| s.id().to_string())
            .unwrap_or_default();
        vec![drasi_lib::config::QueryConfig {
            id: "my-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            query_language: drasi_lib::config::QueryLanguage::Cypher,
            auto_start: true,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            middleware: vec![],
            sources: vec![drasi_lib::config::SourceSubscriptionConfig {
                source_id,
                nodes: vec![],
                relations: vec![],
                pipeline: vec![],
            }],
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        }]
    } else {
        vec![]
    };

    DrasiServerConfig {
        host: ConfigValue::Static(server_settings.host),
        port: ConfigValue::Static(server_settings.port),
        log_level: ConfigValue::Static(server_settings.log_level),
        disable_persistence: false,
        sources,
        reactions,
        core_config: drasi_lib::config::DrasiLibConfig {
            id: server_id,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            queries,
            storage_backends: vec![],
        },
    }
}

/// Generate YAML string from configuration.
pub fn generate_yaml(config: &DrasiServerConfig) -> Result<String> {
    // Add a header comment
    let mut yaml = String::new();
    yaml.push_str("# Drasi Server Configuration\n");
    yaml.push_str("# Generated with: drasi-server init\n");
    yaml.push_str("#\n");
    yaml.push_str("# Edit this file to customize your configuration.\n");
    yaml.push_str("# See documentation at: https://drasi.io/docs\n");
    yaml.push('\n');

    // Serialize the config
    let config_yaml = serde_yaml::to_string(config)?;
    yaml.push_str(&config_yaml);

    // Add helpful comments at the end
    yaml.push_str("\n# Tips:\n");
    yaml.push_str("# - Use environment variables: ${VAR_NAME:-default}\n");
    yaml.push_str("# - Update 'my-query' with your actual Cypher query\n");
    yaml.push_str("# - Connect reactions to your queries by updating the 'queries' field\n");

    Ok(yaml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_server::api::models::{
        HttpSourceConfigDto, LogReactionConfigDto, MockSourceConfigDto, SseReactionConfigDto,
    };

    /// Helper to create test server settings
    fn test_server_settings() -> ServerSettings {
        ServerSettings {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
        }
    }

    /// Helper to create a mock source config for testing
    fn mock_source_config(id: &str) -> SourceConfig {
        SourceConfig::Mock {
            id: id.to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                interval_ms: ConfigValue::Static(5000),
                data_type: ConfigValue::Static("generic".to_string()),
            },
        }
    }

    /// Helper to create an HTTP source config for testing
    fn http_source_config(id: &str) -> SourceConfig {
        SourceConfig::Http {
            id: id.to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: HttpSourceConfigDto {
                host: ConfigValue::Static("0.0.0.0".to_string()),
                port: ConfigValue::Static(9000),
                endpoint: None,
                timeout_ms: ConfigValue::Static(10000),
                adaptive_max_batch_size: None,
                adaptive_min_batch_size: None,
                adaptive_max_wait_ms: None,
                adaptive_min_wait_ms: None,
                adaptive_window_secs: None,
                adaptive_enabled: None,
            },
        }
    }

    /// Helper to create a log reaction config for testing
    fn log_reaction_config(id: &str) -> ReactionConfig {
        ReactionConfig::Log {
            id: id.to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: LogReactionConfigDto::default(),
        }
    }

    /// Helper to create an SSE reaction config for testing
    fn sse_reaction_config(id: &str) -> ReactionConfig {
        ReactionConfig::Sse {
            id: id.to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: SseReactionConfigDto {
                host: ConfigValue::Static("0.0.0.0".to_string()),
                port: ConfigValue::Static(8081),
                sse_path: ConfigValue::Static("/events".to_string()),
                heartbeat_interval_ms: ConfigValue::Static(30000),
                routes: std::collections::HashMap::new(),
                default_template: None,
            },
        }
    }

    // ==================== build_config tests ====================

    #[test]
    fn test_build_config_empty_sources_and_reactions() {
        let settings = test_server_settings();
        let config = build_config(settings, vec![], vec![]);

        // Check server settings are applied
        assert_eq!(config.host, ConfigValue::Static("0.0.0.0".to_string()));
        assert_eq!(config.port, ConfigValue::Static(8080));
        assert_eq!(config.log_level, ConfigValue::Static("info".to_string()));
        assert!(!config.disable_persistence);

        // Check sources and reactions are empty
        assert!(config.sources.is_empty());
        assert!(config.reactions.is_empty());

        // Check no query is generated when no sources
        assert!(config.core_config.queries.is_empty());

        // Check server ID is generated (UUID format)
        assert!(!config.core_config.id.is_empty());
        assert!(uuid::Uuid::parse_str(&config.core_config.id).is_ok());
    }

    #[test]
    fn test_build_config_with_single_source() {
        let settings = test_server_settings();
        let sources = vec![mock_source_config("my-mock")];
        let config = build_config(settings, sources, vec![]);

        // Check source is included
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].id(), "my-mock");

        // Check a sample query is generated
        assert_eq!(config.core_config.queries.len(), 1);
        let query = &config.core_config.queries[0];
        assert_eq!(query.id, "my-query");
        assert_eq!(query.query, "MATCH (n) RETURN n");
        assert!(query.auto_start);
        assert!(query.enable_bootstrap);
        assert_eq!(query.bootstrap_buffer_size, 10000);

        // Check query subscribes to the source
        assert_eq!(query.sources.len(), 1);
        assert_eq!(query.sources[0].source_id, "my-mock");
    }

    #[test]
    fn test_build_config_with_multiple_sources() {
        let settings = test_server_settings();
        let sources = vec![
            mock_source_config("source-1"),
            http_source_config("source-2"),
        ];
        let config = build_config(settings, sources, vec![]);

        // Check all sources are included
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.sources[0].id(), "source-1");
        assert_eq!(config.sources[1].id(), "source-2");

        // Query should subscribe to the first source
        assert_eq!(config.core_config.queries.len(), 1);
        assert_eq!(
            config.core_config.queries[0].sources[0].source_id,
            "source-1"
        );
    }

    #[test]
    fn test_build_config_with_reactions() {
        let settings = test_server_settings();
        let reactions = vec![log_reaction_config("log-1"), sse_reaction_config("sse-1")];
        let config = build_config(settings, vec![], reactions);

        // Check reactions are included
        assert_eq!(config.reactions.len(), 2);
        assert_eq!(config.reactions[0].id(), "log-1");
        assert_eq!(config.reactions[1].id(), "sse-1");
    }

    #[test]
    fn test_build_config_with_sources_and_reactions() {
        let settings = ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 9090,
            log_level: "debug".to_string(),
        };
        let sources = vec![mock_source_config("data-source")];
        let reactions = vec![log_reaction_config("my-log")];

        let config = build_config(settings, sources, reactions);

        // Check custom server settings
        assert_eq!(config.host, ConfigValue::Static("127.0.0.1".to_string()));
        assert_eq!(config.port, ConfigValue::Static(9090));
        assert_eq!(config.log_level, ConfigValue::Static("debug".to_string()));

        // Check components
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.reactions.len(), 1);
        assert_eq!(config.core_config.queries.len(), 1);
    }

    #[test]
    fn test_build_config_generates_unique_server_ids() {
        let settings1 = test_server_settings();
        let settings2 = test_server_settings();

        let config1 = build_config(settings1, vec![], vec![]);
        let config2 = build_config(settings2, vec![], vec![]);

        // Each call should generate a unique ID
        assert_ne!(config1.core_config.id, config2.core_config.id);
    }

    // ==================== generate_yaml tests ====================

    #[test]
    fn test_generate_yaml_includes_header() {
        let settings = test_server_settings();
        let config = build_config(settings, vec![], vec![]);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.starts_with("# Drasi Server Configuration"));
        assert!(yaml.contains("# Generated with: drasi-server init"));
        assert!(yaml.contains("# See documentation at: https://drasi.io/docs"));
    }

    #[test]
    fn test_generate_yaml_includes_tips() {
        let settings = test_server_settings();
        let config = build_config(settings, vec![], vec![]);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.contains("# Tips:"));
        assert!(yaml.contains("# - Use environment variables: ${VAR_NAME:-default}"));
        assert!(yaml.contains("# - Update 'my-query' with your actual Cypher query"));
    }

    #[test]
    fn test_generate_yaml_contains_server_settings() {
        let settings = ServerSettings {
            host: "192.168.1.1".to_string(),
            port: 3000,
            log_level: "warn".to_string(),
        };
        let config = build_config(settings, vec![], vec![]);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.contains("host: 192.168.1.1"));
        assert!(yaml.contains("port: 3000"));
        assert!(yaml.contains("log_level: warn"));
    }

    #[test]
    fn test_generate_yaml_contains_sources() {
        let settings = test_server_settings();
        let sources = vec![mock_source_config("test-source")];
        let config = build_config(settings, sources, vec![]);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.contains("sources:"));
        assert!(yaml.contains("id: test-source"));
        assert!(yaml.contains("kind: mock") || yaml.contains("Mock"));
    }

    #[test]
    fn test_generate_yaml_contains_queries() {
        let settings = test_server_settings();
        let sources = vec![mock_source_config("src")];
        let config = build_config(settings, sources, vec![]);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.contains("queries:"));
        assert!(yaml.contains("id: my-query"));
        assert!(yaml.contains("MATCH (n) RETURN n"));
    }

    #[test]
    fn test_generate_yaml_contains_reactions() {
        let settings = test_server_settings();
        let reactions = vec![log_reaction_config("my-log-reaction")];
        let config = build_config(settings, vec![], reactions);

        let yaml = generate_yaml(&config).unwrap();

        assert!(yaml.contains("reactions:"));
        assert!(yaml.contains("id: my-log-reaction"));
    }

    #[test]
    fn test_generate_yaml_is_valid_yaml() {
        let settings = test_server_settings();
        let sources = vec![mock_source_config("src")];
        let reactions = vec![log_reaction_config("react")];
        let config = build_config(settings, sources, reactions);

        let yaml = generate_yaml(&config).unwrap();

        // Extract just the YAML content (skip comments at start and end)
        let yaml_content: String = yaml
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");

        // Should be parseable as YAML
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml_content).unwrap();
        assert!(parsed.is_mapping());
    }

    #[test]
    fn test_generate_yaml_roundtrip() {
        let settings = test_server_settings();
        let sources = vec![mock_source_config("roundtrip-source")];
        let reactions = vec![log_reaction_config("roundtrip-reaction")];
        let original_config = build_config(settings, sources, reactions);

        let yaml = generate_yaml(&original_config).unwrap();

        // Extract just the YAML content (skip comments)
        let yaml_content: String = yaml
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");

        // Parse back to config
        let parsed_config: DrasiServerConfig = serde_yaml::from_str(&yaml_content).unwrap();

        // Verify key fields match
        assert_eq!(parsed_config.host, original_config.host);
        assert_eq!(parsed_config.port, original_config.port);
        assert_eq!(parsed_config.log_level, original_config.log_level);
        assert_eq!(parsed_config.sources.len(), original_config.sources.len());
        assert_eq!(
            parsed_config.reactions.len(),
            original_config.reactions.len()
        );
        assert_eq!(
            parsed_config.core_config.queries.len(),
            original_config.core_config.queries.len()
        );
    }

    #[test]
    fn test_generate_yaml_empty_config() {
        let settings = test_server_settings();
        let config = build_config(settings, vec![], vec![]);

        let yaml = generate_yaml(&config).unwrap();

        // Should still be valid and contain basic structure
        assert!(yaml.contains("host:"));
        assert!(yaml.contains("port:"));
        assert!(yaml.contains("sources:"));
        assert!(yaml.contains("reactions:"));
    }
}
