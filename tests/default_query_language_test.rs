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

//! Tests for default query language behavior.
//!
//! This test module verifies that:
//! 1. The default query language is GQL when not specified
//! 2. The default can be overridden by explicitly setting queryLanguage
//! 3. Both GQL and Cypher are supported

use drasi_server::api::models::{ConfigValue, QueryConfigDto, SourceSubscriptionConfigDto};
use drasi_server::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};

#[test]
fn test_default_query_language_is_gql() {
    // Test YAML without queryLanguage field deserializes with GQL default
    // This tests the Serde default mechanism
    let yaml = r#"
id: test-query
query: "MATCH (n) RETURN n"
sources:
  - sourceId: test-source
"#;

    let dto: QueryConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    
    // Map to verify the default is applied correctly
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    assert_eq!(
        format!("{:?}", config.query_language),
        "GQL",
        "Default query language should be GQL when not specified in YAML"
    );
}

#[test]
fn test_explicit_cypher_language() {
    // Create a query config with explicit Cypher language
    let dto = QueryConfigDto {
        id: "test-query".to_string(),
        auto_start: false,
        query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
        query_language: ConfigValue::Static("Cypher".to_string()),
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: ConfigValue::Static("test-source".to_string()),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    // Map the DTO to a QueryConfig
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    // Verify the language is Cypher
    assert_eq!(
        format!("{:?}", config.query_language),
        "Cypher",
        "Query language should be Cypher when explicitly set"
    );
}

#[test]
fn test_explicit_gql_language() {
    // Create a query config with explicit GQL language
    let dto = QueryConfigDto {
        id: "test-query".to_string(),
        auto_start: false,
        query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
        query_language: ConfigValue::Static("GQL".to_string()),
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: ConfigValue::Static("test-source".to_string()),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    // Map the DTO to a QueryConfig
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    // Verify the language is GQL
    assert_eq!(
        format!("{:?}", config.query_language),
        "GQL",
        "Query language should be GQL when explicitly set"
    );
}

#[test]
fn test_invalid_query_language_rejected() {
    // Create a query config with invalid language
    let dto = QueryConfigDto {
        id: "test-query".to_string(),
        auto_start: false,
        query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
        query_language: ConfigValue::Static("SQL".to_string()), // Invalid!
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: ConfigValue::Static("test-source".to_string()),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    // Map the DTO to a QueryConfig - should fail
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let result = mapper.map(&dto, &resolver);

    assert!(
        result.is_err(),
        "Invalid query language should be rejected"
    );
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("Invalid query language"),
        "Error should mention invalid query language, got: {err_msg}"
    );
}

#[test]
fn test_yaml_deserialization_default_language() {
    // Test that YAML without queryLanguage field deserializes with GQL default
    let yaml = r#"
id: test-query
query: "MATCH (n) RETURN n"
sources:
  - sourceId: test-source
"#;

    let dto: QueryConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    
    // Map to verify the default is applied correctly
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    assert_eq!(
        format!("{:?}", config.query_language),
        "GQL",
        "Default query language should be GQL when not specified in YAML"
    );
}

#[test]
fn test_yaml_deserialization_explicit_cypher() {
    // Test that YAML with queryLanguage: Cypher works correctly
    let yaml = r#"
id: test-query
query: "MATCH (n) RETURN n"
queryLanguage: Cypher
sources:
  - sourceId: test-source
"#;

    let dto: QueryConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    
    // Map to verify Cypher is used
    let mapper = QueryConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    assert_eq!(
        format!("{:?}", config.query_language),
        "Cypher",
        "Query language should be Cypher when specified in YAML"
    );
}
