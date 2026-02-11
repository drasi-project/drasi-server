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

//! Unit tests for HTTP source configuration mapper including webhook support.

use drasi_server::api::mappings::{ConfigMapper, DtoMapper};
use drasi_server::api::mappings::sources::HttpSourceConfigMapper;
use drasi_server::models::{
    AuthConfigDto, BearerConfigDto, ConfigValue, CorsConfigDto, EffectiveFromConfigDto,
    ElementTemplateDto, ElementTypeDto, ErrorBehaviorDto, HttpMethodDto, HttpSourceConfigDto,
    MappingConditionDto, OperationTypeDto, SignatureAlgorithmDto, SignatureConfigDto,
    SignatureEncodingDto, TimestampFormatDto, WebhookConfigDto, WebhookMappingDto,
    WebhookRouteDto,
};
use drasi_source_http::config::{
    ElementType, ErrorBehavior, HttpMethod, OperationType, SignatureAlgorithm,
    SignatureEncoding, TimestampFormat,
};
use std::collections::HashMap;

// ============================================================================
// Basic Mapping Tests
// ============================================================================

#[test]
fn test_map_http_source_without_webhooks() {
    let dto = HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: Some(ConfigValue::Static("/api".to_string())),
        timeout_ms: ConfigValue::Static(5000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: None,
    };

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert_eq!(config.endpoint, Some("/api".to_string()));
    assert_eq!(config.timeout_ms, 5000);
    assert!(config.webhooks.is_none());
}

#[test]
fn test_map_http_source_with_simple_webhook() {
    let dto = HttpSourceConfigDto {
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
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::Reject,
            cors: None,
            routes: vec![WebhookRouteDto {
                path: "/events".to_string(),
                methods: vec![HttpMethodDto::Post],
                auth: None,
                error_behavior: None,
                mappings: vec![simple_mapping()],
            }],
        }),
    };

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehavior::Reject);
    assert_eq!(webhooks.routes.len(), 1);
    assert_eq!(webhooks.routes[0].path, "/events");
}

// ============================================================================
// Error Behavior Mapping Tests
// ============================================================================

#[test]
fn test_map_error_behavior_accept_and_log() {
    let config = create_config_with_error_behavior(ErrorBehaviorDto::AcceptAndLog);
    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehavior::AcceptAndLog);
}

#[test]
fn test_map_error_behavior_accept_and_skip() {
    let config = create_config_with_error_behavior(ErrorBehaviorDto::AcceptAndSkip);
    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehavior::AcceptAndSkip);
}

#[test]
fn test_map_error_behavior_reject() {
    let config = create_config_with_error_behavior(ErrorBehaviorDto::Reject);
    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehavior::Reject);
}

// ============================================================================
// HTTP Method Mapping Tests
// ============================================================================

#[test]
fn test_map_http_methods() {
    let dto = create_dto_with_methods(vec![
        HttpMethodDto::Get,
        HttpMethodDto::Post,
        HttpMethodDto::Put,
        HttpMethodDto::Patch,
        HttpMethodDto::Delete,
    ]);

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.expect("Should have webhooks");
    let methods = &webhooks.routes[0].methods;

    assert_eq!(methods.len(), 5);
    assert!(methods.contains(&HttpMethod::Get));
    assert!(methods.contains(&HttpMethod::Post));
    assert!(methods.contains(&HttpMethod::Put));
    assert!(methods.contains(&HttpMethod::Patch));
    assert!(methods.contains(&HttpMethod::Delete));
}

// ============================================================================
// CORS Mapping Tests
// ============================================================================

#[test]
fn test_map_cors_config() {
    let dto = HttpSourceConfigDto {
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(10000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::AcceptAndLog,
            cors: Some(CorsConfigDto {
                enabled: true,
                allow_origins: vec!["https://example.com".to_string()],
                allow_methods: vec!["GET".to_string(), "POST".to_string()],
                allow_headers: vec!["Content-Type".to_string()],
                expose_headers: vec!["X-Custom".to_string()],
                allow_credentials: true,
                max_age: 7200,
            }),
            routes: vec![simple_route()],
        }),
    };

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let cors = config.webhooks.unwrap().cors.expect("Should have CORS config");
    assert!(cors.enabled);
    assert_eq!(cors.allow_origins, vec!["https://example.com"]);
    assert_eq!(cors.allow_methods, vec!["GET", "POST"]);
    assert_eq!(cors.allow_headers, vec!["Content-Type"]);
    assert_eq!(cors.expose_headers, vec!["X-Custom"]);
    assert!(cors.allow_credentials);
    assert_eq!(cors.max_age, 7200);
}

// ============================================================================
// Authentication Mapping Tests
// ============================================================================

#[test]
fn test_map_signature_auth_hmac_sha256() {
    let dto = create_dto_with_auth(Some(AuthConfigDto {
        signature: Some(SignatureConfigDto {
            algorithm: SignatureAlgorithmDto::HmacSha256,
            secret_env: "WEBHOOK_SECRET".to_string(),
            header: "X-Signature".to_string(),
            prefix: Some("sha256=".to_string()),
            encoding: SignatureEncodingDto::Hex,
        }),
        bearer: None,
    }));

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.unwrap();
    let auth = webhooks.routes[0].auth.as_ref().expect("Should have auth");
    let sig = auth.signature.as_ref().expect("Should have signature");
    
    assert_eq!(sig.algorithm, SignatureAlgorithm::HmacSha256);
    assert_eq!(sig.secret_env, "WEBHOOK_SECRET");
    assert_eq!(sig.header, "X-Signature");
    assert_eq!(sig.prefix, Some("sha256=".to_string()));
    assert_eq!(sig.encoding, SignatureEncoding::Hex);
}

#[test]
fn test_map_signature_auth_hmac_sha1() {
    let dto = create_dto_with_auth(Some(AuthConfigDto {
        signature: Some(SignatureConfigDto {
            algorithm: SignatureAlgorithmDto::HmacSha1,
            secret_env: "SECRET".to_string(),
            header: "X-Hub-Signature".to_string(),
            prefix: None,
            encoding: SignatureEncodingDto::Base64,
        }),
        bearer: None,
    }));

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.unwrap();
    let sig = webhooks.routes[0]
        .auth.as_ref().unwrap()
        .signature.as_ref().expect("Should have signature");
    
    assert_eq!(sig.algorithm, SignatureAlgorithm::HmacSha1);
    assert_eq!(sig.encoding, SignatureEncoding::Base64);
    assert!(sig.prefix.is_none());
}

#[test]
fn test_map_bearer_auth() {
    let dto = create_dto_with_auth(Some(AuthConfigDto {
        signature: None,
        bearer: Some(BearerConfigDto {
            token_env: "API_TOKEN".to_string(),
        }),
    }));

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.unwrap();
    let bearer = webhooks.routes[0]
        .auth.as_ref().unwrap()
        .bearer.as_ref().expect("Should have bearer");
    
    assert_eq!(bearer.token_env, "API_TOKEN");
}

// ============================================================================
// Webhook Mapping Tests
// ============================================================================

#[test]
fn test_map_webhook_mapping_with_static_operation() {
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let mapping = &config.webhooks.unwrap().routes[0].mappings[0];
    assert_eq!(mapping.operation, Some(OperationType::Insert));
    assert_eq!(mapping.element_type, ElementType::Node);
}

#[test]
fn test_map_all_operation_types() {
    // Insert
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });
    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).unwrap();
    assert_eq!(config.webhooks.unwrap().routes[0].mappings[0].operation, Some(OperationType::Insert));

    // Update
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Update),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });
    let config = mapper.map(&dto, &resolver).unwrap();
    assert_eq!(config.webhooks.unwrap().routes[0].mappings[0].operation, Some(OperationType::Update));

    // Delete
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Delete),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });
    let config = mapper.map(&dto, &resolver).unwrap();
    assert_eq!(config.webhooks.unwrap().routes[0].mappings[0].operation, Some(OperationType::Delete));
}

#[test]
fn test_map_element_types() {
    // Node
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });
    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).unwrap();
    assert_eq!(config.webhooks.unwrap().routes[0].mappings[0].element_type, ElementType::Node);

    // Relation
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Relation,
        effective_from: None,
        template: ElementTemplateDto {
            id: "rel-id".to_string(),
            labels: vec!["RELATES_TO".to_string()],
            properties: None,
            from: Some("source-id".to_string()),
            to: Some("target-id".to_string()),
        },
    });
    let config = mapper.map(&dto, &resolver).unwrap();
    let mapping = &config.webhooks.unwrap().routes[0].mappings[0];
    assert_eq!(mapping.element_type, ElementType::Relation);
    assert_eq!(mapping.template.from, Some("source-id".to_string()));
    assert_eq!(mapping.template.to, Some("target-id".to_string()));
}

#[test]
fn test_map_operation_map() {
    let mut op_map = HashMap::new();
    op_map.insert("created".to_string(), OperationTypeDto::Insert);
    op_map.insert("updated".to_string(), OperationTypeDto::Update);
    op_map.insert("deleted".to_string(), OperationTypeDto::Delete);

    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: None,
        operation_from: Some("$.action".to_string()),
        operation_map: Some(op_map),
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let mapping = &config.webhooks.unwrap().routes[0].mappings[0];
    assert_eq!(mapping.operation_from, Some("$.action".to_string()));
    
    let mapped_ops = mapping.operation_map.as_ref().expect("Should have operation map");
    assert_eq!(mapped_ops.get("created"), Some(&OperationType::Insert));
    assert_eq!(mapped_ops.get("updated"), Some(&OperationType::Update));
    assert_eq!(mapped_ops.get("deleted"), Some(&OperationType::Delete));
}

// ============================================================================
// Mapping Condition Tests
// ============================================================================

#[test]
fn test_map_mapping_condition() {
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: Some(MappingConditionDto {
            header: Some("X-Event-Type".to_string()),
            field: Some("$.type".to_string()),
            equals: Some("push".to_string()),
            contains: Some("event".to_string()),
            regex: Some("^push".to_string()),
        }),
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    });

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.unwrap();
    let condition = webhooks.routes[0].mappings[0]
        .when.as_ref().expect("Should have condition");
    
    assert_eq!(condition.header, Some("X-Event-Type".to_string()));
    assert_eq!(condition.field, Some("$.type".to_string()));
    assert_eq!(condition.equals, Some("push".to_string()));
    assert_eq!(condition.contains, Some("event".to_string()));
    assert_eq!(condition.regex, Some("^push".to_string()));
}

// ============================================================================
// Effective From Mapping Tests
// ============================================================================

#[test]
fn test_map_effective_from_simple() {
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: Some(EffectiveFromConfigDto::Simple("{{payload.timestamp}}".to_string())),
        template: simple_template(),
    });

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let webhooks = config.webhooks.unwrap();
    let effective_from = webhooks.routes[0].mappings[0]
        .effective_from.as_ref().expect("Should have effective_from");
    
    match effective_from {
        drasi_source_http::config::EffectiveFromConfig::Simple(s) => {
            assert_eq!(s, "{{payload.timestamp}}");
        }
        _ => panic!("Expected Simple variant"),
    }
}

#[test]
fn test_map_effective_from_explicit_all_formats() {
    let formats = [
        (TimestampFormatDto::Iso8601, TimestampFormat::Iso8601),
        (TimestampFormatDto::UnixSeconds, TimestampFormat::UnixSeconds),
        (TimestampFormatDto::UnixMillis, TimestampFormat::UnixMillis),
        (TimestampFormatDto::UnixNanos, TimestampFormat::UnixNanos),
    ];

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();

    for (dto_format, expected_format) in formats {
        let dto = create_dto_with_mapping(WebhookMappingDto {
            when: None,
            operation: Some(OperationTypeDto::Insert),
            operation_from: None,
            operation_map: None,
            element_type: ElementTypeDto::Node,
            effective_from: Some(EffectiveFromConfigDto::Explicit {
                value: "{{ts}}".to_string(),
                format: dto_format,
            }),
            template: simple_template(),
        });

        let config = mapper.map(&dto, &resolver).expect("Should map successfully");
        let webhooks = config.webhooks.unwrap();
        let effective_from = webhooks.routes[0].mappings[0]
            .effective_from.as_ref().expect("Should have effective_from");

        match effective_from {
            drasi_source_http::config::EffectiveFromConfig::Explicit { value, format } => {
                assert_eq!(value, "{{ts}}");
                assert_eq!(*format, expected_format);
            }
            _ => panic!("Expected Explicit variant"),
        }
    }
}

// ============================================================================
// Element Template Mapping Tests
// ============================================================================

#[test]
fn test_map_element_template() {
    let dto = create_dto_with_mapping(WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: ElementTemplateDto {
            id: "node-{{payload.id}}".to_string(),
            labels: vec!["Event".to_string(), "Webhook".to_string()],
            properties: Some(serde_json::json!({
                "name": "{{payload.name}}",
                "count": "{{payload.count}}"
            })),
            from: None,
            to: None,
        },
    });

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    let template = &config.webhooks.unwrap().routes[0].mappings[0].template;
    assert_eq!(template.id, "node-{{payload.id}}");
    assert_eq!(template.labels, vec!["Event", "Webhook"]);
    assert!(template.properties.is_some());
}

// ============================================================================
// Complex Integration Tests
// ============================================================================

#[test]
fn test_map_full_github_webhook_config() {
    let mut operation_map = HashMap::new();
    operation_map.insert("opened".to_string(), OperationTypeDto::Insert);
    operation_map.insert("closed".to_string(), OperationTypeDto::Delete);
    operation_map.insert("synchronize".to_string(), OperationTypeDto::Update);

    let dto = HttpSourceConfigDto {
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(30000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::Reject,
            cors: Some(CorsConfigDto::default()),
            routes: vec![WebhookRouteDto {
                path: "/github/events".to_string(),
                methods: vec![HttpMethodDto::Post],
                auth: Some(AuthConfigDto {
                    signature: Some(SignatureConfigDto {
                        algorithm: SignatureAlgorithmDto::HmacSha256,
                        secret_env: "GITHUB_WEBHOOK_SECRET".to_string(),
                        header: "X-Hub-Signature-256".to_string(),
                        prefix: Some("sha256=".to_string()),
                        encoding: SignatureEncodingDto::Hex,
                    }),
                    bearer: None,
                }),
                error_behavior: None,
                mappings: vec![
                    // Push mapping
                    WebhookMappingDto {
                        when: Some(MappingConditionDto {
                            header: Some("X-GitHub-Event".to_string()),
                            field: None,
                            equals: Some("push".to_string()),
                            contains: None,
                            regex: None,
                        }),
                        operation: Some(OperationTypeDto::Insert),
                        operation_from: None,
                        operation_map: None,
                        element_type: ElementTypeDto::Node,
                        effective_from: None,
                        template: ElementTemplateDto {
                            id: "commit-{{payload.head_commit.id}}".to_string(),
                            labels: vec!["Commit".to_string()],
                            properties: Some(serde_json::json!({
                                "message": "{{payload.head_commit.message}}"
                            })),
                            from: None,
                            to: None,
                        },
                    },
                    // PR mapping
                    WebhookMappingDto {
                        when: Some(MappingConditionDto {
                            header: Some("X-GitHub-Event".to_string()),
                            field: None,
                            equals: Some("pull_request".to_string()),
                            contains: None,
                            regex: None,
                        }),
                        operation: None,
                        operation_from: Some("$.action".to_string()),
                        operation_map: Some(operation_map),
                        element_type: ElementTypeDto::Node,
                        effective_from: None,
                        template: ElementTemplateDto {
                            id: "pr-{{payload.pull_request.id}}".to_string(),
                            labels: vec!["PullRequest".to_string()],
                            properties: None,
                            from: None,
                            to: None,
                        },
                    },
                ],
            }],
        }),
    };

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    let config = mapper.map(&dto, &resolver).expect("Should map successfully");

    // Verify top-level config
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8080);
    assert_eq!(config.timeout_ms, 30000);

    // Verify webhook config
    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehavior::Reject);
    assert!(webhooks.cors.is_some());
    assert_eq!(webhooks.routes.len(), 1);

    // Verify route
    let route = &webhooks.routes[0];
    assert_eq!(route.path, "/github/events");
    assert_eq!(route.methods, vec![HttpMethod::Post]);
    assert!(route.auth.is_some());
    assert_eq!(route.mappings.len(), 2);

    // Verify push mapping
    let push_mapping = &route.mappings[0];
    assert!(push_mapping.when.is_some());
    assert_eq!(push_mapping.operation, Some(OperationType::Insert));

    // Verify PR mapping
    let pr_mapping = &route.mappings[1];
    assert!(pr_mapping.operation_map.is_some());
    assert_eq!(pr_mapping.operation_from, Some("$.action".to_string()));
}

// ============================================================================
// Helper Functions
// ============================================================================

fn simple_mapping() -> WebhookMappingDto {
    WebhookMappingDto {
        when: None,
        operation: Some(OperationTypeDto::Insert),
        operation_from: None,
        operation_map: None,
        element_type: ElementTypeDto::Node,
        effective_from: None,
        template: simple_template(),
    }
}

fn simple_template() -> ElementTemplateDto {
    ElementTemplateDto {
        id: "test-id".to_string(),
        labels: vec!["TestLabel".to_string()],
        properties: None,
        from: None,
        to: None,
    }
}

fn simple_route() -> WebhookRouteDto {
    WebhookRouteDto {
        path: "/webhook".to_string(),
        methods: vec![HttpMethodDto::Post],
        auth: None,
        error_behavior: None,
        mappings: vec![simple_mapping()],
    }
}

fn create_config_with_error_behavior(behavior: ErrorBehaviorDto) -> drasi_source_http::HttpSourceConfig {
    let dto = HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(10000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: behavior,
            cors: None,
            routes: vec![simple_route()],
        }),
    };

    let mapper = HttpSourceConfigMapper;
    let resolver = DtoMapper::new();
    mapper.map(&dto, &resolver).expect("Should map")
}

fn create_dto_with_methods(methods: Vec<HttpMethodDto>) -> HttpSourceConfigDto {
    HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(10000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::AcceptAndLog,
            cors: None,
            routes: vec![WebhookRouteDto {
                path: "/webhook".to_string(),
                methods,
                auth: None,
                error_behavior: None,
                mappings: vec![simple_mapping()],
            }],
        }),
    }
}

fn create_dto_with_auth(auth: Option<AuthConfigDto>) -> HttpSourceConfigDto {
    HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(10000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::AcceptAndLog,
            cors: None,
            routes: vec![WebhookRouteDto {
                path: "/webhook".to_string(),
                methods: vec![HttpMethodDto::Post],
                auth,
                error_behavior: None,
                mappings: vec![simple_mapping()],
            }],
        }),
    }
}

fn create_dto_with_mapping(mapping: WebhookMappingDto) -> HttpSourceConfigDto {
    HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(8080),
        endpoint: None,
        timeout_ms: ConfigValue::Static(10000),
        adaptive_max_batch_size: None,
        adaptive_min_batch_size: None,
        adaptive_max_wait_ms: None,
        adaptive_min_wait_ms: None,
        adaptive_window_secs: None,
        adaptive_enabled: None,
        webhooks: Some(WebhookConfigDto {
            error_behavior: ErrorBehaviorDto::AcceptAndLog,
            cors: None,
            routes: vec![WebhookRouteDto {
                path: "/webhook".to_string(),
                methods: vec![HttpMethodDto::Post],
                auth: None,
                error_behavior: None,
                mappings: vec![mapping],
            }],
        }),
    }
}
