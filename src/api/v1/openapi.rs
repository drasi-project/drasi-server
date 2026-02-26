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

use crate::api::models::{
    ComponentEventDto, ComponentStatusDto, ComponentTypeDto, ConfigValueBoolSchema,
    ConfigValueStringSchema, ConfigValueU16Schema, ConfigValueU32Schema, ConfigValueU64Schema,
    ConfigValueUsizeSchema, LogLevelDto, LogMessageDto, QueryConfigDto, RedbStateStoreConfigDto,
    SourceSubscriptionConfigDto, StateStoreConfig,
};
use crate::api::shared::handlers::CreateInstanceRequest;
use crate::api::shared::{
    ApiResponseSchema, ApiVersionsResponse, ComponentListItem, ErrorDetail, ErrorResponse,
    HealthResponse, InstanceLinks, InstanceListItem, StatusResponse,
};
use crate::config::{DrasiLibInstanceConfig, DrasiServerConfig};
use crate::plugin_registry::PluginRegistry;
use utoipa::openapi::schema::{AllOf, ObjectBuilder, OneOf, Ref, Schema};
use utoipa::openapi::RefOr;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::list_api_versions,
        super::handlers::health_check,
        super::handlers::list_instances,
        super::handlers::create_instance,
        super::handlers::list_sources,
        super::handlers::create_source_handler,
        super::handlers::upsert_source_handler,
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
        super::handlers::upsert_reaction_handler,
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
            InstanceLinks,
            CreateInstanceRequest,
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
            QueryConfigDto,
            SourceSubscriptionConfigDto,
            RedbStateStoreConfigDto,
            ConfigValueStringSchema,
            ConfigValueU16Schema,
            ConfigValueU32Schema,
            ConfigValueU64Schema,
            ConfigValueUsizeSchema,
            ConfigValueBoolSchema,
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

/// Injects plugin schemas from the registry into an OpenAPI specification.
///
/// This function dynamically builds the `SourceConfig`, `ReactionConfig`, and
/// `BootstrapProviderConfig` schemas from the registered plugins, replacing
/// the previously hardcoded DTO list.
pub fn inject_plugin_schemas(openapi: &mut utoipa::openapi::OpenApi, registry: &PluginRegistry) {
    let components = openapi.components.get_or_insert_with(Default::default);
    let schemas = &mut components.schemas;

    let mut source_schema_names = Vec::new();
    let mut reaction_schema_names = Vec::new();
    let mut bootstrap_schema_names = Vec::new();

    // Inject source plugin schemas
    for info in registry.source_plugin_infos() {
        inject_schemas_from_json(&info.config_schema_json, schemas);
        source_schema_names.push(info.config_schema_name);
    }

    // Inject reaction plugin schemas
    for info in registry.reaction_plugin_infos() {
        inject_schemas_from_json(&info.config_schema_json, schemas);
        reaction_schema_names.push(info.config_schema_name);
    }

    // Inject bootstrap plugin schemas
    for info in registry.bootstrapper_plugin_infos() {
        inject_schemas_from_json(&info.config_schema_json, schemas);
        bootstrap_schema_names.push(info.config_schema_name);
    }

    // Build SourceConfig OneOf
    if !schemas.contains_key("SourceConfig") {
        schemas.insert(
            "SourceConfig".to_string(),
            RefOr::T(Schema::OneOf(OneOf {
                items: source_schema_names
                    .iter()
                    .map(|name| RefOr::Ref(Ref::from_schema_name(name)))
                    .collect(),
                ..Default::default()
            })),
        );
    }

    // Build ReactionConfig OneOf
    if !schemas.contains_key("ReactionConfig") {
        schemas.insert(
            "ReactionConfig".to_string(),
            RefOr::T(Schema::OneOf(OneOf {
                items: reaction_schema_names
                    .iter()
                    .map(|name| RefOr::Ref(Ref::from_schema_name(name)))
                    .collect(),
                ..Default::default()
            })),
        );
    }

    // Build BootstrapProviderConfig as allOf(base object with kind, oneOf config variants)
    if !schemas.contains_key("BootstrapProviderConfig") {
        let kind_property = ObjectBuilder::new()
            .schema_type(utoipa::openapi::SchemaType::String)
            .description(Some("The bootstrap provider type"))
            .build();

        let mut bootstrap_schema = utoipa::openapi::schema::Object::new();
        bootstrap_schema
            .properties
            .insert("kind".to_string(), RefOr::T(Schema::Object(kind_property)));
        bootstrap_schema.required.push("kind".to_string());
        bootstrap_schema.description = Some(
            "Bootstrap provider configuration. The 'kind' field selects the provider type, \
             and remaining fields are provider-specific."
                .to_string(),
        );

        let config_one_of = OneOf {
            items: bootstrap_schema_names
                .iter()
                .map(|name| RefOr::Ref(Ref::from_schema_name(name)))
                .collect(),
            description: Some("Provider-specific configuration (fields vary by kind)".to_string()),
            ..Default::default()
        };

        let combined = AllOf {
            items: vec![
                RefOr::T(Schema::Object(bootstrap_schema)),
                RefOr::T(Schema::OneOf(config_one_of)),
            ],
            ..Default::default()
        };

        schemas.insert(
            "BootstrapProviderConfig".to_string(),
            RefOr::T(Schema::AllOf(combined)),
        );
    }

    // Add a generic ConfigValue schema referenced by plugin DTOs.
    // Plugin fields typed as ConfigValue<T> generate $ref to "ConfigValue".
    // This aliases to ConfigValueString which is the most common variant.
    if !schemas.contains_key("ConfigValue") {
        schemas.insert(
            "ConfigValue".to_string(),
            RefOr::Ref(Ref::from_schema_name("ConfigValueString")),
        );
    }

    // Add QueryLanguage enum schema (defined in drasi-lib without utoipa)
    if !schemas.contains_key("QueryLanguage") {
        let enum_schema = ObjectBuilder::new()
            .schema_type(utoipa::openapi::SchemaType::String)
            .enum_values(Some(["Cypher", "GQL"]))
            .default(Some("Cypher".into()))
            .build();
        schemas.insert(
            "QueryLanguage".to_string(),
            RefOr::T(Schema::Object(enum_schema)),
        );
    }
}

/// Parse a plugin's JSON schema map and insert all schemas into the OpenAPI components.
fn inject_schemas_from_json(
    json_str: &str,
    schemas: &mut std::collections::BTreeMap<String, RefOr<Schema>>,
) {
    let map: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Failed to parse plugin schema JSON: {}", e);
            return;
        }
    };

    for (name, value) in map {
        match serde_json::from_value::<RefOr<Schema>>(value.clone()) {
            Ok(schema) => {
                schemas.insert(name, schema);
            }
            Err(_) => {
                // Fallback: wrap the raw JSON as an Object schema with additional_properties.
                // This handles schemas with typeless fields (e.g., serde_json::Value properties).
                let mut obj = utoipa::openapi::schema::Object::new();
                if let Some(props) = value.get("properties").and_then(|p| p.as_object()) {
                    for (prop_name, prop_val) in props {
                        if let Ok(prop_schema) =
                            serde_json::from_value::<RefOr<Schema>>(prop_val.clone())
                        {
                            obj.properties.insert(prop_name.clone(), prop_schema);
                        } else {
                            // Typeless properties (like serde_json::Value) — use empty object
                            obj.properties.insert(
                                prop_name.clone(),
                                RefOr::T(Schema::Object(utoipa::openapi::schema::Object::new())),
                            );
                        }
                    }
                }
                if let Some(required) = value.get("required").and_then(|r| r.as_array()) {
                    for r in required {
                        if let Some(s) = r.as_str() {
                            obj.required.push(s.to_string());
                        }
                    }
                }
                if let Some(desc) = value.get("description").and_then(|d| d.as_str()) {
                    obj.description = Some(desc.to_string());
                }
                schemas.insert(name, RefOr::T(Schema::Object(obj)));
            }
        }
    }
}
