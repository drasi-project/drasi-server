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

//! HTTP source configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::sources::http_source::{
    AuthConfigDto, BearerConfigDto, CorsConfigDto, EffectiveFromConfigDto, ElementTemplateDto,
    ElementTypeDto, ErrorBehaviorDto, HttpMethodDto, HttpSourceConfigDto, MappingConditionDto,
    OperationTypeDto, SignatureAlgorithmDto, SignatureConfigDto, SignatureEncodingDto,
    TimestampFormatDto, WebhookConfigDto, WebhookMappingDto, WebhookRouteDto,
};
use drasi_source_http::config::{
    AuthConfig, BearerConfig, CorsConfig, EffectiveFromConfig, ElementTemplate, ElementType,
    ErrorBehavior, HttpMethod, MappingCondition, OperationType, SignatureAlgorithm,
    SignatureConfig, SignatureEncoding, TimestampFormat, WebhookConfig, WebhookMapping,
    WebhookRoute,
};
use drasi_source_http::HttpSourceConfig;

pub struct HttpSourceConfigMapper;

impl ConfigMapper<HttpSourceConfigDto, HttpSourceConfig> for HttpSourceConfigMapper {
    fn map(
        &self,
        dto: &HttpSourceConfigDto,
        resolver: &DtoMapper,
    ) -> Result<HttpSourceConfig, MappingError> {
        Ok(HttpSourceConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            endpoint: resolver.resolve_optional(&dto.endpoint)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
            adaptive_max_batch_size: resolver.resolve_optional(&dto.adaptive_max_batch_size)?,
            adaptive_min_batch_size: resolver.resolve_optional(&dto.adaptive_min_batch_size)?,
            adaptive_max_wait_ms: resolver.resolve_optional(&dto.adaptive_max_wait_ms)?,
            adaptive_min_wait_ms: resolver.resolve_optional(&dto.adaptive_min_wait_ms)?,
            adaptive_window_secs: resolver.resolve_optional(&dto.adaptive_window_secs)?,
            adaptive_enabled: resolver.resolve_optional(&dto.adaptive_enabled)?,
            webhooks: dto.webhooks.as_ref().map(map_webhook_config),
        })
    }
}

fn map_webhook_config(dto: &WebhookConfigDto) -> WebhookConfig {
    WebhookConfig {
        error_behavior: map_error_behavior(&dto.error_behavior),
        cors: dto.cors.as_ref().map(map_cors_config),
        routes: dto.routes.iter().map(map_webhook_route).collect(),
    }
}

fn map_cors_config(dto: &CorsConfigDto) -> CorsConfig {
    CorsConfig {
        enabled: dto.enabled,
        allow_origins: dto.allow_origins.clone(),
        allow_methods: dto.allow_methods.clone(),
        allow_headers: dto.allow_headers.clone(),
        expose_headers: dto.expose_headers.clone(),
        allow_credentials: dto.allow_credentials,
        max_age: dto.max_age,
    }
}

fn map_error_behavior(dto: &ErrorBehaviorDto) -> ErrorBehavior {
    match dto {
        ErrorBehaviorDto::AcceptAndLog => ErrorBehavior::AcceptAndLog,
        ErrorBehaviorDto::AcceptAndSkip => ErrorBehavior::AcceptAndSkip,
        ErrorBehaviorDto::Reject => ErrorBehavior::Reject,
    }
}

fn map_webhook_route(dto: &WebhookRouteDto) -> WebhookRoute {
    WebhookRoute {
        path: dto.path.clone(),
        methods: dto.methods.iter().map(map_http_method).collect(),
        auth: dto.auth.as_ref().map(map_auth_config),
        error_behavior: dto.error_behavior.as_ref().map(map_error_behavior),
        mappings: dto.mappings.iter().map(map_webhook_mapping).collect(),
    }
}

fn map_http_method(dto: &HttpMethodDto) -> HttpMethod {
    match dto {
        HttpMethodDto::Get => HttpMethod::Get,
        HttpMethodDto::Post => HttpMethod::Post,
        HttpMethodDto::Put => HttpMethod::Put,
        HttpMethodDto::Patch => HttpMethod::Patch,
        HttpMethodDto::Delete => HttpMethod::Delete,
    }
}

fn map_auth_config(dto: &AuthConfigDto) -> AuthConfig {
    AuthConfig {
        signature: dto.signature.as_ref().map(map_signature_config),
        bearer: dto.bearer.as_ref().map(map_bearer_config),
    }
}

fn map_signature_config(dto: &SignatureConfigDto) -> SignatureConfig {
    SignatureConfig {
        algorithm: map_signature_algorithm(&dto.algorithm),
        secret_env: dto.secret_env.clone(),
        header: dto.header.clone(),
        prefix: dto.prefix.clone(),
        encoding: map_signature_encoding(&dto.encoding),
    }
}

fn map_signature_algorithm(dto: &SignatureAlgorithmDto) -> SignatureAlgorithm {
    match dto {
        SignatureAlgorithmDto::HmacSha1 => SignatureAlgorithm::HmacSha1,
        SignatureAlgorithmDto::HmacSha256 => SignatureAlgorithm::HmacSha256,
    }
}

fn map_signature_encoding(dto: &SignatureEncodingDto) -> SignatureEncoding {
    match dto {
        SignatureEncodingDto::Hex => SignatureEncoding::Hex,
        SignatureEncodingDto::Base64 => SignatureEncoding::Base64,
    }
}

fn map_bearer_config(dto: &BearerConfigDto) -> BearerConfig {
    BearerConfig {
        token_env: dto.token_env.clone(),
    }
}

fn map_webhook_mapping(dto: &WebhookMappingDto) -> WebhookMapping {
    WebhookMapping {
        when: dto.when.as_ref().map(map_mapping_condition),
        operation: dto.operation.as_ref().map(map_operation_type),
        operation_from: dto.operation_from.clone(),
        operation_map: dto.operation_map.as_ref().map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), map_operation_type(v)))
                .collect()
        }),
        element_type: map_element_type(&dto.element_type),
        effective_from: dto.effective_from.as_ref().map(map_effective_from_config),
        template: map_element_template(&dto.template),
    }
}

fn map_mapping_condition(dto: &MappingConditionDto) -> MappingCondition {
    MappingCondition {
        header: dto.header.clone(),
        field: dto.field.clone(),
        equals: dto.equals.clone(),
        contains: dto.contains.clone(),
        regex: dto.regex.clone(),
    }
}

fn map_operation_type(dto: &OperationTypeDto) -> OperationType {
    match dto {
        OperationTypeDto::Insert => OperationType::Insert,
        OperationTypeDto::Update => OperationType::Update,
        OperationTypeDto::Delete => OperationType::Delete,
    }
}

fn map_element_type(dto: &ElementTypeDto) -> ElementType {
    match dto {
        ElementTypeDto::Node => ElementType::Node,
        ElementTypeDto::Relation => ElementType::Relation,
    }
}

fn map_effective_from_config(dto: &EffectiveFromConfigDto) -> EffectiveFromConfig {
    match dto {
        EffectiveFromConfigDto::Simple(s) => EffectiveFromConfig::Simple(s.clone()),
        EffectiveFromConfigDto::Explicit { value, format } => EffectiveFromConfig::Explicit {
            value: value.clone(),
            format: map_timestamp_format(format),
        },
    }
}

fn map_timestamp_format(dto: &TimestampFormatDto) -> TimestampFormat {
    match dto {
        TimestampFormatDto::Iso8601 => TimestampFormat::Iso8601,
        TimestampFormatDto::UnixSeconds => TimestampFormat::UnixSeconds,
        TimestampFormatDto::UnixMillis => TimestampFormat::UnixMillis,
        TimestampFormatDto::UnixNanos => TimestampFormat::UnixNanos,
    }
}

fn map_element_template(dto: &ElementTemplateDto) -> ElementTemplate {
    ElementTemplate {
        id: dto.id.clone(),
        labels: dto.labels.clone(),
        properties: dto.properties.clone(),
        from: dto.from.clone(),
        to: dto.to.clone(),
    }
}
