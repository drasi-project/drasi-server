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

//! HTTP source configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Local copy of HTTP source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = HttpSourceConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpSourceConfigDto {
    pub host: ConfigValue<String>,
    pub port: ConfigValue<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<ConfigValue<String>>,
    #[serde(default = "default_http_timeout_ms")]
    pub timeout_ms: ConfigValue<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_max_batch_size: Option<ConfigValue<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_min_batch_size: Option<ConfigValue<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_max_wait_ms: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_min_wait_ms: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_window_secs: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_enabled: Option<ConfigValue<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<WebhookConfig>)]
    pub webhooks: Option<WebhookConfigDto>,
}

fn default_http_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(10000)
}

/// Webhook configuration for custom route handling
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = WebhookConfig)]
#[serde(rename_all = "camelCase")]
pub struct WebhookConfigDto {
    #[serde(default)]
    #[schema(value_type = ErrorBehavior)]
    pub error_behavior: ErrorBehaviorDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<CorsConfig>)]
    pub cors: Option<CorsConfigDto>,
    #[schema(value_type = Vec<WebhookRoute>)]
    pub routes: Vec<WebhookRouteDto>,
}

/// CORS configuration for webhook endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = CorsConfig)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfigDto {
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cors_origins")]
    pub allow_origins: Vec<ConfigValue<String>>,
    #[serde(default = "default_cors_methods")]
    pub allow_methods: Vec<ConfigValue<String>>,
    #[serde(default = "default_cors_headers")]
    pub allow_headers: Vec<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose_headers: Vec<ConfigValue<String>>,
    #[serde(default)]
    pub allow_credentials: bool,
    #[serde(default = "default_cors_max_age")]
    pub max_age: u64,
}

fn default_cors_enabled() -> bool {
    true
}

fn default_cors_origins() -> Vec<ConfigValue<String>> {
    vec![ConfigValue::Static("*".to_string())]
}

fn default_cors_methods() -> Vec<ConfigValue<String>> {
    vec![
        ConfigValue::Static("GET".to_string()),
        ConfigValue::Static("POST".to_string()),
        ConfigValue::Static("PUT".to_string()),
        ConfigValue::Static("PATCH".to_string()),
        ConfigValue::Static("DELETE".to_string()),
        ConfigValue::Static("OPTIONS".to_string()),
    ]
}

fn default_cors_headers() -> Vec<ConfigValue<String>> {
    vec![
        ConfigValue::Static("Content-Type".to_string()),
        ConfigValue::Static("Authorization".to_string()),
        ConfigValue::Static("X-Requested-With".to_string()),
    ]
}

fn default_cors_max_age() -> u64 {
    3600
}

/// Error handling behavior for webhook requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, utoipa::ToSchema)]
#[schema(as = ErrorBehavior)]
#[serde(rename_all = "snake_case")]
pub enum ErrorBehaviorDto {
    #[default]
    AcceptAndLog,
    AcceptAndSkip,
    Reject,
}

/// Configuration for a single webhook route
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = WebhookRoute)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRouteDto {
    pub path: ConfigValue<String>,
    #[serde(default = "default_methods")]
    #[schema(value_type = Vec<HttpMethod>)]
    pub methods: Vec<HttpMethodDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<AuthConfig>)]
    pub auth: Option<AuthConfigDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<ErrorBehavior>)]
    pub error_behavior: Option<ErrorBehaviorDto>,
    #[schema(value_type = Vec<WebhookMapping>)]
    pub mappings: Vec<WebhookMappingDto>,
}

fn default_methods() -> Vec<HttpMethodDto> {
    vec![HttpMethodDto::Post]
}

/// HTTP methods supported for webhook routes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, utoipa::ToSchema)]
#[schema(as = HttpMethod)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethodDto {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Authentication configuration for a webhook route
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = AuthConfig)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfigDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<SignatureConfig>)]
    pub signature: Option<SignatureConfigDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<BearerConfig>)]
    pub bearer: Option<BearerConfigDto>,
}

/// HMAC signature verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = SignatureConfig)]
#[serde(rename_all = "camelCase")]
pub struct SignatureConfigDto {
    #[serde(rename = "type")]
    #[schema(value_type = SignatureAlgorithm)]
    pub algorithm: SignatureAlgorithmDto,
    pub secret_env: ConfigValue<String>,
    pub header: ConfigValue<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<ConfigValue<String>>,
    #[serde(default)]
    #[schema(value_type = SignatureEncoding)]
    pub encoding: SignatureEncodingDto,
}

/// Supported HMAC signature algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = SignatureAlgorithm)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureAlgorithmDto {
    HmacSha1,
    HmacSha256,
}

/// Signature encoding format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, utoipa::ToSchema)]
#[schema(as = SignatureEncoding)]
#[serde(rename_all = "lowercase")]
pub enum SignatureEncodingDto {
    #[default]
    Hex,
    Base64,
}

/// Bearer token verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = BearerConfig)]
#[serde(rename_all = "camelCase")]
pub struct BearerConfigDto {
    pub token_env: ConfigValue<String>,
}

/// Mapping configuration from webhook payload to source change event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = WebhookMapping)]
#[serde(rename_all = "camelCase")]
pub struct WebhookMappingDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<MappingCondition>)]
    pub when: Option<MappingConditionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<OperationType>)]
    pub operation: Option<OperationTypeDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_from: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<HashMap<String, OperationType>>)]
    pub operation_map: Option<HashMap<String, OperationTypeDto>>,
    #[schema(value_type = ElementType)]
    pub element_type: ElementTypeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<EffectiveFromConfig>)]
    pub effective_from: Option<EffectiveFromConfigDto>,
    #[schema(value_type = ElementTemplate)]
    pub template: ElementTemplateDto,
}

/// Condition for when a mapping applies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = MappingCondition)]
#[serde(rename_all = "camelCase")]
pub struct MappingConditionDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<ConfigValue<String>>,
}

/// Operation type for source changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = OperationType)]
#[serde(rename_all = "lowercase")]
pub enum OperationTypeDto {
    Insert,
    Update,
    Delete,
}

/// Element type for source changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = ElementType)]
#[serde(rename_all = "lowercase")]
pub enum ElementTypeDto {
    Node,
    Relation,
}

/// Configuration for effective_from timestamp
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = EffectiveFromConfig)]
#[serde(untagged)]
pub enum EffectiveFromConfigDto {
    Simple(ConfigValue<String>),
    Explicit {
        value: ConfigValue<String>,
        format: TimestampFormatDto,
    },
}

/// Timestamp format for effective_from
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = TimestampFormat)]
#[serde(rename_all = "snake_case")]
pub enum TimestampFormatDto {
    Iso8601,
    UnixSeconds,
    UnixMillis,
    UnixNanos,
}

/// Template for element creation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = ElementTemplate)]
#[serde(rename_all = "camelCase")]
pub struct ElementTemplateDto {
    pub id: ConfigValue<String>,
    pub labels: Vec<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ConfigValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<ConfigValue<String>>,
}
