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

//! Unit tests for HTTP webhook configuration DTOs and serialization.

use drasi_server::models::{
    AuthConfigDto, BearerConfigDto, ConfigValue, CorsConfigDto, EffectiveFromConfigDto,
    ElementTemplateDto, ElementTypeDto, ErrorBehaviorDto, HttpMethodDto, HttpSourceConfigDto,
    MappingConditionDto, OperationTypeDto, SignatureAlgorithmDto, SignatureConfigDto,
    SignatureEncodingDto, TimestampFormatDto, WebhookConfigDto, WebhookMappingDto, WebhookRouteDto,
};
use std::collections::HashMap;

// ============================================================================
// WebhookConfigDto Tests
// ============================================================================

#[test]
fn test_webhook_config_serializes_to_yaml() {
    let config = WebhookConfigDto {
        error_behavior: ErrorBehaviorDto::Reject,
        cors: Some(CorsConfigDto::default()),
        routes: vec![WebhookRouteDto {
            path: "/github/events".to_string(),
            methods: vec![HttpMethodDto::Post],
            auth: None,
            error_behavior: None,
            mappings: vec![simple_mapping()],
        }],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert!(yaml.contains("errorBehavior: reject"));
    assert!(yaml.contains("path: /github/events"));
}

#[test]
fn test_webhook_config_deserializes_from_yaml() {
    let yaml = r#"
errorBehavior: accept_and_log
routes:
  - path: /webhook
    methods: [POST]
    mappings:
      - elementType: node
        operation: insert
        template:
          id: "test-id"
          labels: ["TestLabel"]
"#;

    let config: WebhookConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(config.error_behavior, ErrorBehaviorDto::AcceptAndLog);
    assert_eq!(config.routes.len(), 1);
    assert_eq!(config.routes[0].path, "/webhook");
}

#[test]
fn test_webhook_config_with_cors() {
    let yaml = r#"
errorBehavior: reject
cors:
  enabled: true
  allowOrigins: ["https://example.com", "https://other.com"]
  allowMethods: [GET, POST]
  allowHeaders: [Content-Type, Authorization]
  allowCredentials: true
  maxAge: 7200
routes:
  - path: /api
    mappings:
      - elementType: node
        template:
          id: "id"
          labels: ["Label"]
"#;

    let config: WebhookConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    let cors = config.cors.expect("Should have CORS config");
    assert!(cors.enabled);
    assert_eq!(
        cors.allow_origins,
        vec!["https://example.com", "https://other.com"]
    );
    assert_eq!(cors.allow_methods, vec!["GET", "POST"]);
    assert!(cors.allow_credentials);
    assert_eq!(cors.max_age, 7200);
}

// ============================================================================
// ErrorBehaviorDto Tests
// ============================================================================

#[test]
fn test_error_behavior_serialization() {
    assert_eq!(
        serde_json::to_string(&ErrorBehaviorDto::AcceptAndLog).unwrap(),
        "\"accept_and_log\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorBehaviorDto::AcceptAndSkip).unwrap(),
        "\"accept_and_skip\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorBehaviorDto::Reject).unwrap(),
        "\"reject\""
    );
}

#[test]
fn test_error_behavior_deserialization() {
    assert_eq!(
        serde_json::from_str::<ErrorBehaviorDto>("\"accept_and_log\"").unwrap(),
        ErrorBehaviorDto::AcceptAndLog
    );
    assert_eq!(
        serde_json::from_str::<ErrorBehaviorDto>("\"accept_and_skip\"").unwrap(),
        ErrorBehaviorDto::AcceptAndSkip
    );
    assert_eq!(
        serde_json::from_str::<ErrorBehaviorDto>("\"reject\"").unwrap(),
        ErrorBehaviorDto::Reject
    );
}

#[test]
fn test_error_behavior_default() {
    let default = ErrorBehaviorDto::default();
    assert_eq!(default, ErrorBehaviorDto::AcceptAndLog);
}

// ============================================================================
// HttpMethodDto Tests
// ============================================================================

#[test]
fn test_http_method_serialization() {
    assert_eq!(
        serde_json::to_string(&HttpMethodDto::Get).unwrap(),
        "\"GET\""
    );
    assert_eq!(
        serde_json::to_string(&HttpMethodDto::Post).unwrap(),
        "\"POST\""
    );
    assert_eq!(
        serde_json::to_string(&HttpMethodDto::Put).unwrap(),
        "\"PUT\""
    );
    assert_eq!(
        serde_json::to_string(&HttpMethodDto::Patch).unwrap(),
        "\"PATCH\""
    );
    assert_eq!(
        serde_json::to_string(&HttpMethodDto::Delete).unwrap(),
        "\"DELETE\""
    );
}

#[test]
fn test_http_method_deserialization() {
    assert_eq!(
        serde_json::from_str::<HttpMethodDto>("\"GET\"").unwrap(),
        HttpMethodDto::Get
    );
    assert_eq!(
        serde_json::from_str::<HttpMethodDto>("\"POST\"").unwrap(),
        HttpMethodDto::Post
    );
    assert_eq!(
        serde_json::from_str::<HttpMethodDto>("\"PUT\"").unwrap(),
        HttpMethodDto::Put
    );
    assert_eq!(
        serde_json::from_str::<HttpMethodDto>("\"PATCH\"").unwrap(),
        HttpMethodDto::Patch
    );
    assert_eq!(
        serde_json::from_str::<HttpMethodDto>("\"DELETE\"").unwrap(),
        HttpMethodDto::Delete
    );
}

// ============================================================================
// WebhookRouteDto Tests
// ============================================================================

#[test]
fn test_webhook_route_with_path_parameters() {
    let yaml = r#"
path: /users/:user_id/webhooks/:webhook_id
methods: [POST, PUT]
mappings:
  - elementType: node
    template:
      id: "{{path.user_id}}-{{path.webhook_id}}"
      labels: ["UserWebhook"]
"#;

    let route: WebhookRouteDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(route.path, "/users/:user_id/webhooks/:webhook_id");
    assert_eq!(route.methods, vec![HttpMethodDto::Post, HttpMethodDto::Put]);
}

#[test]
fn test_webhook_route_default_methods() {
    let yaml = r#"
path: /webhook
mappings:
  - elementType: node
    template:
      id: "id"
      labels: ["Label"]
"#;

    let route: WebhookRouteDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(route.methods, vec![HttpMethodDto::Post]); // Default is POST
}

#[test]
fn test_webhook_route_with_error_behavior_override() {
    let yaml = r#"
path: /critical
errorBehavior: reject
mappings:
  - elementType: node
    template:
      id: "id"
      labels: ["Label"]
"#;

    let route: WebhookRouteDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(route.error_behavior, Some(ErrorBehaviorDto::Reject));
}

// ============================================================================
// AuthConfigDto Tests
// ============================================================================

#[test]
fn test_auth_config_with_hmac_signature() {
    let yaml = r#"
signature:
  type: hmac-sha256
  secretEnv: GITHUB_WEBHOOK_SECRET
  header: X-Hub-Signature-256
  prefix: "sha256="
  encoding: hex
"#;

    let auth: AuthConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    let sig = auth.signature.expect("Should have signature config");
    assert_eq!(sig.algorithm, SignatureAlgorithmDto::HmacSha256);
    assert_eq!(sig.secret_env, "GITHUB_WEBHOOK_SECRET");
    assert_eq!(sig.header, "X-Hub-Signature-256");
    assert_eq!(sig.prefix, Some("sha256=".to_string()));
    assert_eq!(sig.encoding, SignatureEncodingDto::Hex);
}

#[test]
fn test_auth_config_with_hmac_sha1() {
    let yaml = r#"
signature:
  type: hmac-sha1
  secretEnv: WEBHOOK_SECRET
  header: X-Signature
"#;

    let auth: AuthConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    let sig = auth.signature.expect("Should have signature config");
    assert_eq!(sig.algorithm, SignatureAlgorithmDto::HmacSha1);
}

#[test]
fn test_auth_config_with_bearer_token() {
    let yaml = r#"
bearer:
  tokenEnv: API_TOKEN
"#;

    let auth: AuthConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    let bearer = auth.bearer.expect("Should have bearer config");
    assert_eq!(bearer.token_env, "API_TOKEN");
}

#[test]
fn test_auth_config_with_both_signature_and_bearer() {
    let yaml = r#"
signature:
  type: hmac-sha256
  secretEnv: HMAC_SECRET
  header: X-Signature
bearer:
  tokenEnv: API_TOKEN
"#;

    let auth: AuthConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert!(auth.signature.is_some());
    assert!(auth.bearer.is_some());
}

// ============================================================================
// SignatureEncodingDto Tests
// ============================================================================

#[test]
fn test_signature_encoding_serialization() {
    assert_eq!(
        serde_json::to_string(&SignatureEncodingDto::Hex).unwrap(),
        "\"hex\""
    );
    assert_eq!(
        serde_json::to_string(&SignatureEncodingDto::Base64).unwrap(),
        "\"base64\""
    );
}

#[test]
fn test_signature_encoding_default() {
    let default = SignatureEncodingDto::default();
    assert_eq!(default, SignatureEncodingDto::Hex);
}

// ============================================================================
// WebhookMappingDto Tests
// ============================================================================

#[test]
fn test_webhook_mapping_with_static_operation() {
    let yaml = r#"
elementType: node
operation: insert
template:
  id: "{{payload.id}}"
  labels: ["Event"]
  properties:
    name: "{{payload.name}}"
"#;

    let mapping: WebhookMappingDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(mapping.element_type, ElementTypeDto::Node);
    assert_eq!(mapping.operation, Some(OperationTypeDto::Insert));
    assert_eq!(mapping.template.id, "{{payload.id}}");
    assert_eq!(mapping.template.labels, vec!["Event"]);
}

#[test]
fn test_webhook_mapping_with_operation_from_path() {
    let yaml = r#"
elementType: node
operationFrom: "$.action"
operationMap:
  created: insert
  updated: update
  deleted: delete
template:
  id: "{{payload.id}}"
  labels: ["Resource"]
"#;

    let mapping: WebhookMappingDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(mapping.operation_from, Some("$.action".to_string()));

    let op_map = mapping.operation_map.expect("Should have operation map");
    assert_eq!(op_map.get("created"), Some(&OperationTypeDto::Insert));
    assert_eq!(op_map.get("updated"), Some(&OperationTypeDto::Update));
    assert_eq!(op_map.get("deleted"), Some(&OperationTypeDto::Delete));
}

#[test]
fn test_webhook_mapping_for_relation() {
    let yaml = r#"
elementType: relation
operation: insert
template:
  id: "{{payload.relation_id}}"
  labels: ["CONNECTS_TO"]
  from: "{{payload.source_id}}"
  to: "{{payload.target_id}}"
"#;

    let mapping: WebhookMappingDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(mapping.element_type, ElementTypeDto::Relation);
    assert_eq!(
        mapping.template.from,
        Some("{{payload.source_id}}".to_string())
    );
    assert_eq!(
        mapping.template.to,
        Some("{{payload.target_id}}".to_string())
    );
}

// ============================================================================
// MappingConditionDto Tests
// ============================================================================

#[test]
fn test_mapping_condition_with_header_equals() {
    let yaml = r#"
header: X-Event-Type
equals: push
"#;

    let condition: MappingConditionDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(condition.header, Some("X-Event-Type".to_string()));
    assert_eq!(condition.equals, Some("push".to_string()));
}

#[test]
fn test_mapping_condition_with_field_contains() {
    let yaml = r#"
field: "$.event.type"
contains: "user"
"#;

    let condition: MappingConditionDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(condition.field, Some("$.event.type".to_string()));
    assert_eq!(condition.contains, Some("user".to_string()));
}

#[test]
fn test_mapping_condition_with_regex() {
    let yaml = r#"
field: "$.action"
regex: "^(created|updated)$"
"#;

    let condition: MappingConditionDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert_eq!(condition.regex, Some("^(created|updated)$".to_string()));
}

#[test]
fn test_webhook_mapping_with_condition() {
    let yaml = r#"
when:
  header: X-GitHub-Event
  equals: push
elementType: node
operation: insert
template:
  id: "{{payload.head_commit.id}}"
  labels: ["Commit"]
"#;

    let mapping: WebhookMappingDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    let condition = mapping.when.expect("Should have condition");
    assert_eq!(condition.header, Some("X-GitHub-Event".to_string()));
    assert_eq!(condition.equals, Some("push".to_string()));
}

// ============================================================================
// EffectiveFromConfigDto Tests
// ============================================================================

#[test]
fn test_effective_from_simple_string() {
    let yaml = "\"{{payload.timestamp}}\"";
    let config: EffectiveFromConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");

    match config {
        EffectiveFromConfigDto::Simple(s) => assert_eq!(s, "{{payload.timestamp}}"),
        _ => panic!("Expected Simple variant"),
    }
}

#[test]
fn test_effective_from_explicit_config() {
    let yaml = r#"
value: "{{payload.created_at}}"
format: iso8601
"#;

    let config: EffectiveFromConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");

    match config {
        EffectiveFromConfigDto::Explicit { value, format } => {
            assert_eq!(value, "{{payload.created_at}}");
            assert_eq!(format, TimestampFormatDto::Iso8601);
        }
        _ => panic!("Expected Explicit variant"),
    }
}

#[test]
fn test_timestamp_format_variants() {
    assert_eq!(
        serde_json::from_str::<TimestampFormatDto>("\"iso8601\"").unwrap(),
        TimestampFormatDto::Iso8601
    );
    assert_eq!(
        serde_json::from_str::<TimestampFormatDto>("\"unix_seconds\"").unwrap(),
        TimestampFormatDto::UnixSeconds
    );
    assert_eq!(
        serde_json::from_str::<TimestampFormatDto>("\"unix_millis\"").unwrap(),
        TimestampFormatDto::UnixMillis
    );
    assert_eq!(
        serde_json::from_str::<TimestampFormatDto>("\"unix_nanos\"").unwrap(),
        TimestampFormatDto::UnixNanos
    );
}

// ============================================================================
// OperationTypeDto Tests
// ============================================================================

#[test]
fn test_operation_type_serialization() {
    assert_eq!(
        serde_json::to_string(&OperationTypeDto::Insert).unwrap(),
        "\"insert\""
    );
    assert_eq!(
        serde_json::to_string(&OperationTypeDto::Update).unwrap(),
        "\"update\""
    );
    assert_eq!(
        serde_json::to_string(&OperationTypeDto::Delete).unwrap(),
        "\"delete\""
    );
}

// ============================================================================
// ElementTypeDto Tests
// ============================================================================

#[test]
fn test_element_type_serialization() {
    assert_eq!(
        serde_json::to_string(&ElementTypeDto::Node).unwrap(),
        "\"node\""
    );
    assert_eq!(
        serde_json::to_string(&ElementTypeDto::Relation).unwrap(),
        "\"relation\""
    );
}

// ============================================================================
// CorsConfigDto Tests
// ============================================================================

#[test]
fn test_cors_config_default() {
    let config = CorsConfigDto::default();
    assert!(config.enabled);
    assert_eq!(config.allow_origins, vec!["*"]);
    assert!(config.allow_methods.contains(&"GET".to_string()));
    assert!(config.allow_methods.contains(&"POST".to_string()));
    assert!(config.allow_headers.contains(&"Content-Type".to_string()));
    assert!(!config.allow_credentials);
    assert_eq!(config.max_age, 3600);
}

#[test]
fn test_cors_config_custom() {
    let yaml = r#"
enabled: true
allowOrigins: ["https://app.example.com"]
allowMethods: [GET, POST, PUT]
allowHeaders: [Content-Type, X-Custom-Header]
exposeHeaders: [X-Response-Id]
allowCredentials: true
maxAge: 86400
"#;

    let config: CorsConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");
    assert!(config.enabled);
    assert_eq!(config.allow_origins, vec!["https://app.example.com"]);
    assert_eq!(config.allow_methods, vec!["GET", "POST", "PUT"]);
    assert_eq!(
        config.allow_headers,
        vec!["Content-Type", "X-Custom-Header"]
    );
    assert_eq!(config.expose_headers, vec!["X-Response-Id"]);
    assert!(config.allow_credentials);
    assert_eq!(config.max_age, 86400);
}

// ============================================================================
// HttpSourceConfigDto with Webhooks Tests
// ============================================================================

#[test]
fn test_http_source_config_with_webhooks() {
    let config = HttpSourceConfigDto {
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

    let yaml = serde_yaml::to_string(&config).expect("Should serialize");
    assert!(yaml.contains("webhooks:"));
    assert!(yaml.contains("path: /events"));
}

#[test]
fn test_http_source_config_without_webhooks() {
    let config = HttpSourceConfigDto {
        host: ConfigValue::Static("localhost".to_string()),
        port: ConfigValue::Static(9000),
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

    let yaml = serde_yaml::to_string(&config).expect("Should serialize");
    assert!(!yaml.contains("webhooks:")); // Should skip None
}

#[test]
fn test_full_github_webhook_config() {
    let yaml = r#"
host: "0.0.0.0"
port: 8080
timeoutMs: 30000
webhooks:
  errorBehavior: reject
  cors:
    allowOrigins: ["*"]
  routes:
    - path: /github/events
      methods: [POST]
      auth:
        signature:
          type: hmac-sha256
          secretEnv: GITHUB_WEBHOOK_SECRET
          header: X-Hub-Signature-256
          prefix: "sha256="
      mappings:
        - when:
            header: X-GitHub-Event
            equals: push
          elementType: node
          operation: insert
          template:
            id: "commit-{{payload.head_commit.id}}"
            labels: ["Commit"]
            properties:
              message: "{{payload.head_commit.message}}"
              author: "{{payload.head_commit.author.name}}"
        - when:
            header: X-GitHub-Event
            equals: pull_request
          elementType: node
          operationFrom: "$.action"
          operationMap:
            opened: insert
            closed: delete
            synchronize: update
          template:
            id: "pr-{{payload.pull_request.id}}"
            labels: ["PullRequest"]
"#;

    let config: HttpSourceConfigDto = serde_yaml::from_str(yaml).expect("Should deserialize");

    assert_eq!(config.host, ConfigValue::Static("0.0.0.0".to_string()));
    assert_eq!(config.port, ConfigValue::Static(8080));

    let webhooks = config.webhooks.expect("Should have webhooks");
    assert_eq!(webhooks.error_behavior, ErrorBehaviorDto::Reject);
    assert_eq!(webhooks.routes.len(), 1);

    let route = &webhooks.routes[0];
    assert_eq!(route.path, "/github/events");
    assert_eq!(route.mappings.len(), 2);

    // First mapping - push events
    let push_mapping = &route.mappings[0];
    assert_eq!(push_mapping.operation, Some(OperationTypeDto::Insert));

    // Second mapping - PR events with operation map
    let pr_mapping = &route.mappings[1];
    assert!(pr_mapping.operation_map.is_some());
}

// ============================================================================
// Round-trip Tests
// ============================================================================

#[test]
fn test_webhook_config_yaml_roundtrip() {
    let original = WebhookConfigDto {
        error_behavior: ErrorBehaviorDto::Reject,
        cors: Some(CorsConfigDto {
            enabled: true,
            allow_origins: vec!["https://example.com".to_string()],
            allow_methods: vec!["GET".to_string(), "POST".to_string()],
            allow_headers: vec!["Content-Type".to_string()],
            expose_headers: vec!["X-Custom".to_string()],
            allow_credentials: true,
            max_age: 7200,
        }),
        routes: vec![WebhookRouteDto {
            path: "/api/webhook".to_string(),
            methods: vec![HttpMethodDto::Post, HttpMethodDto::Put],
            auth: Some(AuthConfigDto {
                signature: Some(SignatureConfigDto {
                    algorithm: SignatureAlgorithmDto::HmacSha256,
                    secret_env: "SECRET".to_string(),
                    header: "X-Sig".to_string(),
                    prefix: Some("sha256=".to_string()),
                    encoding: SignatureEncodingDto::Hex,
                }),
                bearer: None,
            }),
            error_behavior: Some(ErrorBehaviorDto::AcceptAndSkip),
            mappings: vec![WebhookMappingDto {
                when: Some(MappingConditionDto {
                    header: Some("X-Type".to_string()),
                    field: None,
                    equals: Some("event".to_string()),
                    contains: None,
                    regex: None,
                }),
                operation: Some(OperationTypeDto::Insert),
                operation_from: None,
                operation_map: None,
                element_type: ElementTypeDto::Node,
                effective_from: Some(EffectiveFromConfigDto::Explicit {
                    value: "{{payload.ts}}".to_string(),
                    format: TimestampFormatDto::UnixMillis,
                }),
                template: ElementTemplateDto {
                    id: "{{payload.id}}".to_string(),
                    labels: vec!["Event".to_string()],
                    properties: Some(serde_json::json!({"name": "{{payload.name}}"})),
                    from: None,
                    to: None,
                },
            }],
        }],
    };

    let yaml = serde_yaml::to_string(&original).expect("Should serialize");
    let parsed: WebhookConfigDto = serde_yaml::from_str(&yaml).expect("Should deserialize");

    assert_eq!(original, parsed);
}

#[test]
fn test_webhook_config_json_roundtrip() {
    let original = WebhookConfigDto {
        error_behavior: ErrorBehaviorDto::AcceptAndLog,
        cors: None,
        routes: vec![WebhookRouteDto {
            path: "/hook".to_string(),
            methods: vec![HttpMethodDto::Post],
            auth: None,
            error_behavior: None,
            mappings: vec![simple_mapping()],
        }],
    };

    let json = serde_json::to_string(&original).expect("Should serialize");
    let parsed: WebhookConfigDto = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(original, parsed);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_webhook_route_requires_path() {
    let yaml = r#"
methods: [POST]
mappings:
  - elementType: node
    template:
      id: "id"
      labels: ["Label"]
"#;

    let result: Result<WebhookRouteDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_webhook_route_requires_mappings() {
    let yaml = r#"
path: /webhook
methods: [POST]
"#;

    let result: Result<WebhookRouteDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_webhook_mapping_requires_element_type() {
    let yaml = r#"
operation: insert
template:
  id: "id"
  labels: ["Label"]
"#;

    let result: Result<WebhookMappingDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_element_template_requires_id() {
    let yaml = r#"
labels: ["Label"]
"#;

    let result: Result<ElementTemplateDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_element_template_requires_labels() {
    let yaml = r#"
id: "test-id"
"#;

    let result: Result<ElementTemplateDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_signature_config_requires_all_fields() {
    let yaml = r#"
type: hmac-sha256
secretEnv: SECRET
"#;
    // Missing 'header' field
    let result: Result<SignatureConfigDto, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
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
        template: ElementTemplateDto {
            id: "test-id".to_string(),
            labels: vec!["TestLabel".to_string()],
            properties: None,
            from: None,
            to: None,
        },
    }
}
