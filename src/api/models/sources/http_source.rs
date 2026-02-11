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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub webhooks: Option<WebhookConfigDto>,
}

fn default_http_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(10000)
}

/// Webhook configuration for custom route handling
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebhookConfigDto {
    /// Global error behavior for unmatched/failed requests
    #[serde(default)]
    pub error_behavior: ErrorBehaviorDto,

    /// CORS configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cors: Option<CorsConfigDto>,

    /// List of webhook route configurations
    pub routes: Vec<WebhookRouteDto>,
}

/// CORS configuration for webhook endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorsConfigDto {
    /// Whether CORS is enabled
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,

    /// Allowed origins
    #[serde(default = "default_cors_origins")]
    pub allow_origins: Vec<String>,

    /// Allowed HTTP methods
    #[serde(default = "default_cors_methods")]
    pub allow_methods: Vec<String>,

    /// Allowed headers
    #[serde(default = "default_cors_headers")]
    pub allow_headers: Vec<String>,

    /// Headers to expose
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose_headers: Vec<String>,

    /// Whether to allow credentials
    #[serde(default)]
    pub allow_credentials: bool,

    /// Max age in seconds for preflight caching
    #[serde(default = "default_cors_max_age")]
    pub max_age: u64,
}

fn default_cors_enabled() -> bool {
    true
}

fn default_cors_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_cors_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "PATCH".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_cors_headers() -> Vec<String> {
    vec![
        "Content-Type".to_string(),
        "Authorization".to_string(),
        "X-Requested-With".to_string(),
    ]
}

fn default_cors_max_age() -> u64 {
    3600
}

impl Default for CorsConfigDto {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_origins: default_cors_origins(),
            allow_methods: default_cors_methods(),
            allow_headers: default_cors_headers(),
            expose_headers: Vec::new(),
            allow_credentials: false,
            max_age: default_cors_max_age(),
        }
    }
}

/// Error handling behavior for webhook requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ErrorBehaviorDto {
    /// Accept the request and log the issue
    #[default]
    AcceptAndLog,
    /// Accept the request but silently discard
    AcceptAndSkip,
    /// Reject the request with an HTTP error
    Reject,
}

/// Configuration for a single webhook route
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebhookRouteDto {
    /// Route path pattern (supports `:param` for path parameters)
    pub path: String,

    /// Allowed HTTP methods
    #[serde(default = "default_methods")]
    pub methods: Vec<HttpMethodDto>,

    /// Authentication configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfigDto>,

    /// Error behavior override for this route
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_behavior: Option<ErrorBehaviorDto>,

    /// Mappings from payload to source change events
    pub mappings: Vec<WebhookMappingDto>,
}

fn default_methods() -> Vec<HttpMethodDto> {
    vec![HttpMethodDto::Post]
}

/// HTTP methods supported for webhook routes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethodDto {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Authentication configuration for a webhook route
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthConfigDto {
    /// HMAC signature verification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureConfigDto>,

    /// Bearer token verification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<BearerConfigDto>,
}

/// HMAC signature verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignatureConfigDto {
    /// Signature algorithm type
    #[serde(rename = "type")]
    pub algorithm: SignatureAlgorithmDto,

    /// Environment variable containing the secret
    pub secret_env: String,

    /// Header containing the signature
    pub header: String,

    /// Prefix to strip from signature
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    /// Encoding of the signature
    #[serde(default)]
    pub encoding: SignatureEncodingDto,
}

/// Supported HMAC signature algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureAlgorithmDto {
    HmacSha1,
    HmacSha256,
}

/// Signature encoding format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SignatureEncodingDto {
    #[default]
    Hex,
    Base64,
}

/// Bearer token verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BearerConfigDto {
    /// Environment variable containing the expected token
    pub token_env: String,
}

/// Mapping configuration from webhook payload to source change event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebhookMappingDto {
    /// Condition for when this mapping applies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<MappingConditionDto>,

    /// Static operation type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationTypeDto>,

    /// Path to extract operation from payload
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_from: Option<String>,

    /// Mapping from payload values to operation types
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_map: Option<HashMap<String, OperationTypeDto>>,

    /// Element type to create
    pub element_type: ElementTypeDto,

    /// Timestamp configuration for effective_from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_from: Option<EffectiveFromConfigDto>,

    /// Template for element creation
    pub template: ElementTemplateDto,
}

/// Condition for when a mapping applies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MappingConditionDto {
    /// Header to check
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,

    /// Payload field path to check
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,

    /// Value must equal this
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,

    /// Value must contain this
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,

    /// Value must match this regex
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
}

/// Operation type for source changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OperationTypeDto {
    Insert,
    Update,
    Delete,
}

/// Element type for source changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ElementTypeDto {
    Node,
    Relation,
}

/// Configuration for effective_from timestamp
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EffectiveFromConfigDto {
    /// Simple template string
    Simple(String),
    /// Explicit configuration with format
    Explicit {
        /// Template for the timestamp value
        value: String,
        /// Format of the timestamp
        format: TimestampFormatDto,
    },
}

/// Timestamp format for effective_from
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimestampFormatDto {
    /// ISO 8601 datetime string
    Iso8601,
    /// Unix timestamp in seconds
    UnixSeconds,
    /// Unix timestamp in milliseconds
    UnixMillis,
    /// Unix timestamp in nanoseconds
    UnixNanos,
}

/// Template for element creation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementTemplateDto {
    /// Template for element ID
    pub id: String,

    /// Templates for element labels
    pub labels: Vec<String>,

    /// Templates for element properties
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,

    /// Template for relation source node ID (relations only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,

    /// Template for relation target node ID (relations only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}
