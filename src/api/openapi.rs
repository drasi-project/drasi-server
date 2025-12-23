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

use utoipa::OpenApi;

use crate::api::error::{ErrorDetail, ErrorResponse};
use crate::api::handlers::{
    ApiResponseSchema, ComponentListItem, HealthResponse, InstanceListItem, StatusResponse,
};
// Note: Config types from drasi_lib are imported but not used in schema
// as they don't implement ToSchema trait
#[allow(unused_imports)]
use drasi_lib::{
    channels::{ComponentStatus, ComponentType},
    config::{QueryJoinConfig, QueryJoinKeyConfig, QueryRuntime, ReactionRuntime, SourceRuntime},
    QueryConfig,
};
// SourceConfig and ReactionConfig are defined in crate::config, not drasi_lib
#[allow(unused_imports)]
use crate::config::{ReactionConfig, SourceConfig};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::handlers::list_instances,
        crate::api::handlers::health_check,
        crate::api::handlers::list_sources,
        crate::api::handlers::create_source_handler,
        crate::api::handlers::get_source,
        crate::api::handlers::delete_source,
        crate::api::handlers::start_source,
        crate::api::handlers::stop_source,
        crate::api::handlers::list_queries,
        crate::api::handlers::create_query,
        crate::api::handlers::get_query,
        crate::api::handlers::delete_query,
        crate::api::handlers::start_query,
        crate::api::handlers::stop_query,
        crate::api::handlers::get_query_results,
        crate::api::handlers::list_reactions,
        crate::api::handlers::create_reaction_handler,
        crate::api::handlers::get_reaction,
        crate::api::handlers::delete_reaction,
        crate::api::handlers::start_reaction,
        crate::api::handlers::stop_reaction,
    ),
    components(
        schemas(
            HealthResponse,
            ComponentListItem,
            ApiResponseSchema,
            StatusResponse,
            InstanceListItem,
            ErrorResponse,
            ErrorDetail,
            // Note: Config types from drasi_lib are not included
            // in the schema as they don't implement ToSchema trait
        )
    ),
    tags(
        (name = "Health", description = "Health check endpoints"),
        (name = "Instances", description = "DrasiLib instance management"),
        (name = "Sources", description = "Data source management"),
        (name = "Queries", description = "Continuous query management"),
        (name = "Reactions", description = "Reaction management"),
    ),
    info(
        title = "Drasi Server API",
        version = "0.1.0",
        description = "Standalone Drasi server for data change processing",
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
pub struct ApiDoc;
