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

//! OpenAPI documentation for API v1.
//!
//! This module defines the OpenAPI specification for the v1 API.
//! The spec is available at `/api/v1/openapi.json` and the Swagger UI
//! is served at `/api/v1/docs/`.

use utoipa::OpenApi;

use utoipa::openapi::schema::{OneOf, Ref, Schema};
use utoipa::openapi::RefOr;
use crate::api::models::{
    ApplicationBootstrapConfigDto, BootstrapProviderConfig, CallSpecDto, ConfigValueBoolSchema,
    ConfigValueSslModeSchema, ConfigValueStringSchema, ConfigValueU16Schema, ConfigValueU32Schema,
    ConfigValueU64Schema, ConfigValueUsizeSchema, GrpcAdaptiveReactionConfigDto,
    GrpcReactionConfigDto, GrpcSourceConfigDto, HttpAdaptiveReactionConfigDto, HttpQueryConfigDto,
    HttpReactionConfigDto, HttpSourceConfigDto, LogReactionConfigDto, MockSourceConfigDto,
    PlatformBootstrapConfigDto, PlatformReactionConfigDto, PlatformSourceConfigDto,
    PostgresBootstrapConfigDto, PostgresSourceConfigDto, ProfilerReactionConfigDto, QueryConfigDto,
    RedbStateStoreConfigDto, ScriptFileBootstrapConfigDto, SourceSubscriptionConfigDto,
    StateStoreConfig, TableKeyConfigDto, ComponentEventDto, LogMessageDto, ComponentStatusDto,
    ComponentTypeDto, LogLevelDto,
};
use crate::api::shared::{
    ApiResponseSchema, ApiVersionsResponse, ComponentListItem, ErrorDetail, ErrorResponse,
    HealthResponse, InstanceListItem, StatusResponse,
};
use crate::config::{DrasiLibInstanceConfig, DrasiServerConfig};

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::list_api_versions,
        super::handlers::health_check,
        super::handlers::list_instances,
        super::handlers::list_sources,
        super::handlers::create_source_handler,
        super::handlers::get_source,
        super::handlers::get_source_events,
        super::handlers::stream_source_events,
        super::handlers::get_source_logs,
        super::handlers::stream_source_logs,
        super::handlers::delete_source,
        super::handlers::start_source,
        super::handlers::stop_source,
        super::handlers::list_queries,
        super::handlers::create_query,
        super::handlers::get_query,
        super::handlers::get_query_events,
        super::handlers::stream_query_events,
        super::handlers::get_query_logs,
        super::handlers::stream_query_logs,
        super::handlers::delete_query,
        super::handlers::start_query,
        super::handlers::stop_query,
        super::handlers::get_query_results,
        super::handlers::attach_query_stream,
        super::handlers::list_reactions,
        super::handlers::create_reaction_handler,
        super::handlers::get_reaction,
        super::handlers::get_reaction_events,
        super::handlers::stream_reaction_events,
        super::handlers::get_reaction_logs,
        super::handlers::stream_reaction_logs,
        super::handlers::delete_reaction,
        super::handlers::start_reaction,
        super::handlers::stop_reaction,
    ),
    components(
        schemas(
            HealthResponse,
            ComponentListItem,
            ApiResponseSchema,
            StatusResponse,
            InstanceListItem,
            ApiVersionsResponse,
            ErrorResponse,
            ErrorDetail,
            ComponentTypeDto,
            ComponentStatusDto,
            LogLevelDto,
            ComponentEventDto,
            LogMessageDto,
            DrasiServerConfig,
            DrasiLibInstanceConfig,
            ConfigValueStringSchema,
            ConfigValueU16Schema,
            ConfigValueU32Schema,
            ConfigValueU64Schema,
            ConfigValueUsizeSchema,
            ConfigValueBoolSchema,
            ConfigValueSslModeSchema,
            MockSourceConfigDto,
            HttpSourceConfigDto,
            GrpcSourceConfigDto,
            PostgresSourceConfigDto,
            PlatformSourceConfigDto,
            TableKeyConfigDto,
            PostgresBootstrapConfigDto,
            ApplicationBootstrapConfigDto,
            ScriptFileBootstrapConfigDto,
            PlatformBootstrapConfigDto,
            BootstrapProviderConfig,
            LogReactionConfigDto,
            HttpReactionConfigDto,
            HttpAdaptiveReactionConfigDto,
            HttpQueryConfigDto,
            CallSpecDto,
            GrpcReactionConfigDto,
            GrpcAdaptiveReactionConfigDto,
            crate::api::models::reactions::sse::SseReactionConfigDto,
            crate::api::models::reactions::sse::SseQueryConfigDto,
            crate::api::models::reactions::sse::SseTemplateSpecDto,
            PlatformReactionConfigDto,
            ProfilerReactionConfigDto,
            QueryConfigDto,
            SourceSubscriptionConfigDto,
            RedbStateStoreConfigDto,
        )
    ),
    modifiers(&SourceReactionConfigSchemas),
    tags(
        (name = "API", description = "API version information"),
        (name = "Health", description = "Health check endpoints"),
        (name = "Instances", description = "DrasiLib instance management"),
        (name = "Sources", description = "Data source management"),
        (name = "Queries", description = "Continuous query management"),
        (name = "Reactions", description = "Reaction management"),
    ),
    info(
        title = "Drasi Server API",
        version = "1.0.0",
        description = "Drasi Server REST API v1.\n\nDrasi Server provides a standalone server for data change processing using the Drasi library.\n\n## API Versioning\n\nThis API uses URL-based versioning. All endpoints are prefixed with `/api/v1/`.\n\n## Multi-Instance Support\n\nDrasi Server supports multiple concurrent DrasiLib instances. Each instance has its own sources, queries, and reactions.\n\n### Instance-Specific Routes\n\nAccess specific instances via:\n- `/api/v1/instances/{instanceId}/sources`\n- `/api/v1/instances/{instanceId}/queries`\n- `/api/v1/instances/{instanceId}/reactions`\n\n### Convenience Routes (First Instance)\n\nFor convenience, the first configured instance is also accessible via shortened routes:\n- `/api/v1/sources` - Sources of the first instance\n- `/api/v1/queries` - Queries of the first instance\n- `/api/v1/reactions` - Reactions of the first instance",
        contact(
            name = "Drasi Project",
            url = "https://github.com/drasi-project/drasi-server"
        ),
        license(
            name = "Apache-2.0",
            url = "https://www.apache.org/licenses/LICENSE-2.0"
        )
    )
)]
pub struct ApiDocV1;

struct SourceReactionConfigSchemas;

impl utoipa::Modify for SourceReactionConfigSchemas {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        let schemas = &mut components.schemas;

        let source_variants = vec![
            "MockSourceConfig",
            "HttpSourceConfig",
            "GrpcSourceConfig",
            "PostgresSourceConfig",
            "PlatformSourceConfig",
        ];
        let reaction_variants = vec![
            "LogReactionConfig",
            "HttpReactionConfig",
            "HttpAdaptiveReactionConfig",
            "GrpcReactionConfig",
            "GrpcAdaptiveReactionConfig",
            "SseReactionConfig",
            "PlatformReactionConfig",
            "ProfilerReactionConfig",
        ];

        if !schemas.contains_key("SourceConfig") {
            schemas.insert(
                "SourceConfig".to_string(),
                RefOr::T(Schema::OneOf(OneOf {
                    items: source_variants
                        .iter()
                        .map(|name| RefOr::Ref(Ref::from_schema_name(*name)))
                        .collect(),
                    ..Default::default()
                })),
            );
        }

        if !schemas.contains_key("ReactionConfig") {
            schemas.insert(
                "ReactionConfig".to_string(),
                RefOr::T(Schema::OneOf(OneOf {
                    items: reaction_variants
                        .iter()
                        .map(|name| RefOr::Ref(Ref::from_schema_name(*name)))
                        .collect(),
                    ..Default::default()
                })),
            );
        }
    }
}
