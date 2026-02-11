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
use crate::api::models::sources::http_source::*;
use crate::api::models::HttpSourceConfigDto;
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
            webhooks: dto
                .webhooks
                .as_ref()
                .map(|w| map_webhook_config(w, resolver))
                .transpose()?,
        })
    }
}

fn map_webhook_config(
    dto: &WebhookConfigDto,
    resolver: &DtoMapper,
) -> Result<WebhookConfig, MappingError> {
    Ok(WebhookConfig {
        error_behavior: map_error_behavior(&dto.error_behavior),
        cors: dto.cors.as_ref().map(|c| map_cors_config(c, resolver)).transpose()?,
        routes: dto
            .routes
            .iter()
            .map(|r| map_webhook_route(r, resolver))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn map_cors_config(
    dto: &CorsConfigDto,
    resolver: &DtoMapper,
) -> Result<CorsConfig, MappingError> {
    Ok(CorsConfig {
        enabled: dto.enabled,
        allow_origins: resolver.resolve_string_vec(&dto.allow_origins)?,
        allow_methods: resolver.resolve_string_vec(&dto.allow_methods)?,
        allow_headers: resolver.resolve_string_vec(&dto.allow_headers)?,
        expose_headers: resolver.resolve_string_vec(&dto.expose_headers)?,
        allow_credentials: dto.allow_credentials,
        max_age: dto.max_age,
    })
}

fn map_error_behavior(dto: &ErrorBehaviorDto) -> ErrorBehavior {
    match dto {
        ErrorBehaviorDto::AcceptAndLog => ErrorBehavior::AcceptAndLog,
        ErrorBehaviorDto::AcceptAndSkip => ErrorBehavior::AcceptAndSkip,
        ErrorBehaviorDto::Reject => ErrorBehavior::Reject,
    }
}

fn map_webhook_route(
    dto: &WebhookRouteDto,
    resolver: &DtoMapper,
) -> Result<WebhookRoute, MappingError> {
    Ok(WebhookRoute {
        path: resolver.resolve_string(&dto.path)?,
        methods: dto.methods.iter().map(map_http_method).collect(),
        auth: dto.auth.as_ref().map(|a| map_auth_config(a, resolver)).transpose()?,
        error_behavior: dto.error_behavior.as_ref().map(map_error_behavior),
        mappings: dto
            .mappings
            .iter()
            .map(|m| map_webhook_mapping(m, resolver))
            .collect::<Result<Vec<_>, _>>()?,
    })
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

fn map_auth_config(
    dto: &AuthConfigDto,
    resolver: &DtoMapper,
) -> Result<AuthConfig, MappingError> {
    Ok(AuthConfig {
        signature: dto
            .signature
            .as_ref()
            .map(|s| map_signature_config(s, resolver))
            .transpose()?,
        bearer: dto
            .bearer
            .as_ref()
            .map(|b| map_bearer_config(b, resolver))
            .transpose()?,
    })
}

fn map_signature_config(
    dto: &SignatureConfigDto,
    resolver: &DtoMapper,
) -> Result<SignatureConfig, MappingError> {
    Ok(SignatureConfig {
        algorithm: map_signature_algorithm(&dto.algorithm),
        secret_env: resolver.resolve_string(&dto.secret_env)?,
        header: resolver.resolve_string(&dto.header)?,
        prefix: resolver.resolve_optional_string(&dto.prefix)?,
        encoding: map_signature_encoding(&dto.encoding),
    })
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

fn map_bearer_config(
    dto: &BearerConfigDto,
    resolver: &DtoMapper,
) -> Result<BearerConfig, MappingError> {
    Ok(BearerConfig {
        token_env: resolver.resolve_string(&dto.token_env)?,
    })
}

fn map_webhook_mapping(
    dto: &WebhookMappingDto,
    resolver: &DtoMapper,
) -> Result<WebhookMapping, MappingError> {
    Ok(WebhookMapping {
        when: dto
            .when
            .as_ref()
            .map(|c| map_mapping_condition(c, resolver))
            .transpose()?,
        operation: dto.operation.as_ref().map(map_operation_type),
        operation_from: resolver.resolve_optional_string(&dto.operation_from)?,
        operation_map: dto.operation_map.as_ref().map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), map_operation_type(v)))
                .collect()
        }),
        element_type: map_element_type(&dto.element_type),
        effective_from: dto
            .effective_from
            .as_ref()
            .map(|e| map_effective_from(e, resolver))
            .transpose()?,
        template: map_element_template(&dto.template, resolver)?,
    })
}

fn map_mapping_condition(
    dto: &MappingConditionDto,
    resolver: &DtoMapper,
) -> Result<MappingCondition, MappingError> {
    Ok(MappingCondition {
        header: resolver.resolve_optional_string(&dto.header)?,
        field: resolver.resolve_optional_string(&dto.field)?,
        equals: resolver.resolve_optional_string(&dto.equals)?,
        contains: resolver.resolve_optional_string(&dto.contains)?,
        regex: resolver.resolve_optional_string(&dto.regex)?,
    })
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

fn map_effective_from(
    dto: &EffectiveFromConfigDto,
    resolver: &DtoMapper,
) -> Result<EffectiveFromConfig, MappingError> {
    match dto {
        EffectiveFromConfigDto::Simple(v) => {
            Ok(EffectiveFromConfig::Simple(resolver.resolve_string(v)?))
        }
        EffectiveFromConfigDto::Explicit { value, format } => {
            Ok(EffectiveFromConfig::Explicit {
                value: resolver.resolve_string(value)?,
                format: map_timestamp_format(format),
            })
        }
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

fn map_element_template(
    dto: &ElementTemplateDto,
    resolver: &DtoMapper,
) -> Result<ElementTemplate, MappingError> {
    Ok(ElementTemplate {
        id: resolver.resolve_string(&dto.id)?,
        labels: resolver.resolve_string_vec(&dto.labels)?,
        properties: dto.properties.clone(),
        from: resolver.resolve_optional_string(&dto.from)?,
        to: resolver.resolve_optional_string(&dto.to)?,
    })
}
