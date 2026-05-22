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

//! End-to-end tests for config persistence roundtrip.
//!
//! These tests verify that ConfigValue envelopes (secrets, environment variables)
//! and camelCase property keys survive the full save → load cycle. This was
//! broken by PR #84 which introduced `snapshot_configuration()` → `properties()`
//! roundtripping that flattened ConfigValue wrappers to resolved plaintext.

mod test_support;

use drasi_server::models::ConfigValue;
use drasi_server::{load_config_file, DrasiServerConfig, ReactionConfig, SourceConfig};
use tempfile::TempDir;

/// Helper: create a DrasiServerConfig, save to file, reload, return reloaded config
fn roundtrip_config(config: &DrasiServerConfig) -> DrasiServerConfig {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("roundtrip-test.yaml");

    config
        .save_to_file(&config_path)
        .expect("Failed to save config");

    load_config_file(&config_path).expect("Failed to load config")
}

// =============================================================================
// Secret Reference Roundtrip Tests
// =============================================================================

#[test]
fn test_secret_reference_survives_source_config_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": "db.example.com",
                "port": 5432,
                "database": "mydb",
                "user": "admin",
                "password": { "kind": "secret", "name": "DB_PASSWORD" }
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    assert_eq!(loaded.sources.len(), 1);
    let source = &loaded.sources[0];
    assert_eq!(source.id, "pg-source");
    assert_eq!(source.kind, "postgres");

    // Verify secret reference is preserved (not resolved to plaintext)
    let password = source
        .config
        .get("password")
        .expect("password should exist");
    assert!(
        password.is_object(),
        "password should be an object (secret envelope), got: {password}"
    );
    assert_eq!(password.get("kind").unwrap(), "secret");
    assert_eq!(password.get("name").unwrap(), "DB_PASSWORD");

    // Verify static values are preserved
    assert_eq!(source.config.get("host").unwrap(), "db.example.com");
    assert_eq!(source.config.get("port").unwrap(), 5432);
}

#[test]
fn test_secret_reference_survives_reaction_config_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        reactions: vec![ReactionConfig {
            kind: "http".to_string(),
            id: "webhook".to_string(),
            queries: vec!["q1".to_string()],
            auto_start: true,
            config: serde_json::json!({
                "endpoint": "https://api.example.com",
                "authToken": { "kind": "secret", "name": "API_TOKEN" }
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    assert_eq!(loaded.reactions.len(), 1);
    let reaction = &loaded.reactions[0];

    let auth = reaction
        .config
        .get("authToken")
        .expect("authToken should exist");
    assert!(auth.is_object(), "authToken should be a secret envelope");
    assert_eq!(auth.get("kind").unwrap(), "secret");
    assert_eq!(auth.get("name").unwrap(), "API_TOKEN");
}

// =============================================================================
// Environment Variable Reference Roundtrip Tests
// =============================================================================

#[test]
fn test_env_var_reference_survives_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": { "kind": "EnvironmentVariable", "name": "DB_HOST", "default": "localhost" },
                "port": 5432,
                "database": "mydb",
                "user": "admin",
                "password": "static-pass"
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    let source = &loaded.sources[0];
    let host = source.config.get("host").expect("host should exist");
    assert!(
        host.is_object(),
        "host should be an env var envelope, got: {host}"
    );
    assert_eq!(host.get("kind").unwrap(), "EnvironmentVariable");
    assert_eq!(host.get("name").unwrap(), "DB_HOST");
    assert_eq!(host.get("default").unwrap(), "localhost");
}

#[test]
fn test_env_var_without_default_survives_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": { "kind": "EnvironmentVariable", "name": "DB_HOST" },
                "port": 5432
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    let host = loaded.sources[0].config.get("host").unwrap();
    assert_eq!(host.get("kind").unwrap(), "EnvironmentVariable");
    assert_eq!(host.get("name").unwrap(), "DB_HOST");
    // "default" key should not be present (or be null)
    assert!(
        host.get("default").is_none() || host.get("default").unwrap().is_null(),
        "default should not be present or be null"
    );
}

// =============================================================================
// camelCase Preservation Tests
// =============================================================================

#[test]
fn test_camelcase_keys_preserved_in_source_config_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": "localhost",
                "port": 5432,
                "slotName": "my_slot",
                "publicationName": "my_pub",
                "sslMode": "prefer",
                "tables": [{ "name": "users", "keys": ["id"] }]
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);
    let source = &loaded.sources[0];

    // Verify camelCase keys are preserved
    assert!(
        source.config.get("slotName").is_some(),
        "slotName should be preserved"
    );
    assert!(
        source.config.get("publicationName").is_some(),
        "publicationName should be preserved"
    );
    assert!(
        source.config.get("sslMode").is_some(),
        "sslMode should be preserved"
    );

    // Verify they were NOT converted to snake_case
    assert!(
        source.config.get("slot_name").is_none(),
        "Should NOT have snake_case slot_name"
    );
    assert!(
        source.config.get("publication_name").is_none(),
        "Should NOT have snake_case publication_name"
    );
    assert!(
        source.config.get("ssl_mode").is_none(),
        "Should NOT have snake_case ssl_mode"
    );
}

#[test]
fn test_camelcase_keys_preserved_in_reaction_config_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        reactions: vec![ReactionConfig {
            kind: "sse".to_string(),
            id: "sse-reaction".to_string(),
            queries: vec!["q1".to_string()],
            auto_start: true,
            config: serde_json::json!({
                "ssePath": "/events",
                "queryConfig": {
                    "q1": { "format": "json" }
                }
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);
    let reaction = &loaded.reactions[0];

    assert!(
        reaction.config.get("ssePath").is_some(),
        "ssePath should be preserved"
    );
    assert!(
        reaction.config.get("queryConfig").is_some(),
        "queryConfig should be preserved"
    );
    assert!(
        reaction.config.get("sse_path").is_none(),
        "Should NOT have snake_case sse_path"
    );
}

// =============================================================================
// Mixed Config Values Tests
// =============================================================================

#[test]
fn test_mixed_static_secret_and_env_var_values_roundtrip() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": { "kind": "EnvironmentVariable", "name": "DB_HOST", "default": "localhost" },
                "port": 5432,
                "database": "mydb",
                "user": { "kind": "EnvironmentVariable", "name": "DB_USER" },
                "password": { "kind": "secret", "name": "DB_PASSWORD" },
                "sslMode": "prefer",
                "slotName": "replication_slot",
                "tables": [
                    { "name": "users", "keys": ["id"] },
                    { "name": "orders", "keys": ["order_id"] }
                ]
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);
    let source = &loaded.sources[0];

    // Env var with default
    let host = source.config.get("host").unwrap();
    assert_eq!(host.get("kind").unwrap(), "EnvironmentVariable");
    assert_eq!(host.get("default").unwrap(), "localhost");

    // Static integer
    assert_eq!(source.config.get("port").unwrap(), 5432);

    // Static string
    assert_eq!(source.config.get("database").unwrap(), "mydb");

    // Env var without default
    let user = source.config.get("user").unwrap();
    assert_eq!(user.get("kind").unwrap(), "EnvironmentVariable");
    assert_eq!(user.get("name").unwrap(), "DB_USER");

    // Secret reference
    let password = source.config.get("password").unwrap();
    assert_eq!(password.get("kind").unwrap(), "secret");
    assert_eq!(password.get("name").unwrap(), "DB_PASSWORD");

    // Static string (camelCase key)
    assert_eq!(source.config.get("sslMode").unwrap(), "prefer");

    // Nested structure
    let tables = source.config.get("tables").unwrap();
    assert!(tables.is_array());
    assert_eq!(tables.as_array().unwrap().len(), 2);
}

// =============================================================================
// Password Preservation Tests
// =============================================================================

#[test]
fn test_password_field_preserved_not_stripped() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": "localhost",
                "port": 5432,
                "password": "my-secret-password"
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);
    let source = &loaded.sources[0];

    // Password should be preserved, not stripped
    assert_eq!(
        source.config.get("password").unwrap(),
        "my-secret-password",
        "Password should survive roundtrip"
    );
}

#[test]
fn test_password_as_secret_preserved() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "pg-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "host": "localhost",
                "password": { "kind": "secret", "name": "PG_PASSWORD" }
            }),
            identity_provider: None,
        }],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);
    let password = loaded.sources[0].config.get("password").unwrap();

    assert!(
        password.is_object(),
        "Secret password should remain as object"
    );
    assert_eq!(password.get("kind").unwrap(), "secret");
    assert_eq!(password.get("name").unwrap(), "PG_PASSWORD");
}

// =============================================================================
// Multiple Components Roundtrip Test
// =============================================================================

#[test]
fn test_multiple_sources_and_reactions_all_preserve_config_values() {
    let config = DrasiServerConfig {
        persist_config: true,
        sources: vec![
            SourceConfig {
                kind: "postgres".to_string(),
                id: "source-1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "host": { "kind": "EnvironmentVariable", "name": "S1_HOST" },
                    "password": { "kind": "secret", "name": "S1_PASS" }
                }),
                identity_provider: None,
            },
            SourceConfig {
                kind: "http".to_string(),
                id: "source-2".to_string(),
                auto_start: false,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "endpoint": "https://api.example.com",
                    "authHeader": { "kind": "secret", "name": "API_KEY" }
                }),
                identity_provider: None,
            },
        ],
        reactions: vec![
            ReactionConfig {
                kind: "log".to_string(),
                id: "reaction-1".to_string(),
                queries: vec!["q1".to_string()],
                auto_start: true,
                config: serde_json::json!({"routes": {}}),
                identity_provider: None,
            },
            ReactionConfig {
                kind: "http".to_string(),
                id: "reaction-2".to_string(),
                queries: vec!["q1".to_string()],
                auto_start: true,
                config: serde_json::json!({
                    "endpoint": { "kind": "EnvironmentVariable", "name": "WEBHOOK_URL" },
                    "bearerToken": { "kind": "secret", "name": "WEBHOOK_TOKEN" }
                }),
                identity_provider: None,
            },
        ],
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    // Source 1
    let s1 = &loaded.sources[0];
    assert_eq!(
        s1.config.get("host").unwrap().get("kind").unwrap(),
        "EnvironmentVariable"
    );
    assert_eq!(
        s1.config.get("password").unwrap().get("kind").unwrap(),
        "secret"
    );

    // Source 2
    let s2 = &loaded.sources[1];
    assert_eq!(
        s2.config.get("endpoint").unwrap(),
        "https://api.example.com"
    );
    assert_eq!(
        s2.config.get("authHeader").unwrap().get("kind").unwrap(),
        "secret"
    );

    // Reaction 2
    let r2 = &loaded.reactions[1];
    assert_eq!(
        r2.config.get("endpoint").unwrap().get("kind").unwrap(),
        "EnvironmentVariable"
    );
    assert_eq!(
        r2.config.get("bearerToken").unwrap().get("kind").unwrap(),
        "secret"
    );
}

// =============================================================================
// Server-Level ConfigValue Roundtrip Tests
// =============================================================================

#[test]
fn test_server_config_value_env_vars_roundtrip() {
    let config = DrasiServerConfig {
        host: ConfigValue::EnvironmentVariable {
            name: "SERVER_HOST".to_string(),
            default: Some("0.0.0.0".to_string()),
        },
        port: ConfigValue::EnvironmentVariable {
            name: "SERVER_PORT".to_string(),
            default: Some("8080".to_string()),
        },
        persist_config: true,
        ..DrasiServerConfig::default()
    };

    let loaded = roundtrip_config(&config);

    match &loaded.host {
        ConfigValue::EnvironmentVariable { name, default } => {
            assert_eq!(name, "SERVER_HOST");
            assert_eq!(default.as_deref(), Some("0.0.0.0"));
        }
        other => panic!("Expected EnvironmentVariable for host, got: {other:?}"),
    }

    match &loaded.port {
        ConfigValue::EnvironmentVariable { name, default } => {
            assert_eq!(name, "SERVER_PORT");
            assert_eq!(default.as_deref(), Some("8080"));
        }
        other => panic!("Expected EnvironmentVariable for port, got: {other:?}"),
    }
}
