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

use crate::api::shared::{
    ApiResponseSchema, ApiVersionsResponse, ComponentListItem, ErrorDetail, ErrorResponse,
    HealthResponse, InstanceListItem, StatusResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::list_api_versions,
        super::handlers::health_check,
        super::handlers::list_instances,
        super::handlers::list_sources,
        super::handlers::create_source_handler,
        super::handlers::get_source,
        super::handlers::delete_source,
        super::handlers::start_source,
        super::handlers::stop_source,
        super::handlers::list_queries,
        super::handlers::create_query,
        super::handlers::get_query,
        super::handlers::delete_query,
        super::handlers::start_query,
        super::handlers::stop_query,
        super::handlers::get_query_results,
        super::handlers::list_reactions,
        super::handlers::create_reaction_handler,
        super::handlers::get_reaction,
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
        )
    ),
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
