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

//! API v1 route definitions.
//!
//! This module provides the route builder for API v1 endpoints.
//! All routes are designed to be nested under `/api/v1/`.

use axum::{
    extract::Extension,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use super::handlers;
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;

/// Build the complete v1 API router with dynamic instance routing.
///
/// Uses `:instanceId` path parameter for dynamic instance lookup at request time.
pub fn build_v1_router(
    registry: InstanceRegistry,
    read_only: Arc<bool>,
    config_persistence: Option<Arc<ConfigPersistence>>,
    plugin_registry: Arc<PluginRegistry>,
) -> Router {
    // Instance management routes
    let instance_routes = Router::new()
        .route("/instances", get(handlers::list_instances))
        .route("/instances", post(handlers::create_instance));

    // Dynamic instance-specific routes using :instanceId path parameter
    let instance_resource_routes = build_dynamic_instance_router();

    // Convenience routes for the default (first) instance
    let default_routes = build_default_instance_router();

    Router::new()
        .merge(instance_routes)
        .nest("/instances/:instanceId", instance_resource_routes)
        .merge(default_routes)
        .layer(Extension(registry))
        .layer(Extension(read_only))
        .layer(Extension(config_persistence))
        .layer(Extension(plugin_registry))
}

/// Build routes for dynamic instance resources.
/// These routes use :instanceId path parameter - handlers look up instance from registry.
fn build_dynamic_instance_router() -> Router {
    Router::new()
        // Source routes
        .route("/sources", get(handlers::list_sources))
        .route("/sources", post(handlers::create_source_handler))
        .route("/sources/:id", put(handlers::upsert_source_handler))
        .route("/sources/:id", get(handlers::get_source))
        .route("/sources/:id/events", get(handlers::get_source_events))
        .route(
            "/sources/:id/events/stream",
            get(handlers::stream_source_events),
        )
        .route("/sources/:id/logs", get(handlers::get_source_logs))
        .route(
            "/sources/:id/logs/stream",
            get(handlers::stream_source_logs),
        )
        .route("/sources/:id", delete(handlers::delete_source))
        .route("/sources/:id/start", post(handlers::start_source))
        .route("/sources/:id/stop", post(handlers::stop_source))
        // Query routes
        .route("/queries", get(handlers::list_queries))
        .route("/queries", post(handlers::create_query))
        .route("/queries/:id", get(handlers::get_query))
        .route("/queries/:id/events", get(handlers::get_query_events))
        .route(
            "/queries/:id/events/stream",
            get(handlers::stream_query_events),
        )
        .route("/queries/:id/logs", get(handlers::get_query_logs))
        .route("/queries/:id/logs/stream", get(handlers::stream_query_logs))
        .route("/queries/:id", delete(handlers::delete_query))
        .route("/queries/:id/start", post(handlers::start_query))
        .route("/queries/:id/stop", post(handlers::stop_query))
        .route("/queries/:id/results", get(handlers::get_query_results))
        .route("/queries/:id/attach", get(handlers::attach_query_stream))
        // Reaction routes
        .route("/reactions", get(handlers::list_reactions))
        .route("/reactions", post(handlers::create_reaction_handler))
        .route("/reactions/:id", put(handlers::upsert_reaction_handler))
        .route("/reactions/:id", get(handlers::get_reaction))
        .route("/reactions/:id/events", get(handlers::get_reaction_events))
        .route(
            "/reactions/:id/events/stream",
            get(handlers::stream_reaction_events),
        )
        .route("/reactions/:id/logs", get(handlers::get_reaction_logs))
        .route(
            "/reactions/:id/logs/stream",
            get(handlers::stream_reaction_logs),
        )
        .route("/reactions/:id", delete(handlers::delete_reaction))
        .route("/reactions/:id/start", post(handlers::start_reaction))
        .route("/reactions/:id/stop", post(handlers::stop_reaction))
}

/// Build convenience routes that operate on the default (first) instance.
fn build_default_instance_router() -> Router {
    Router::new()
        // Source routes (default instance)
        .route("/sources", get(handlers::list_sources_default))
        .route("/sources", post(handlers::create_source_default))
        .route("/sources/:id", put(handlers::upsert_source_default))
        .route("/sources/:id", get(handlers::get_source_default))
        .route(
            "/sources/:id/events",
            get(handlers::get_source_events_default),
        )
        .route(
            "/sources/:id/events/stream",
            get(handlers::stream_source_events_default),
        )
        .route("/sources/:id/logs", get(handlers::get_source_logs_default))
        .route(
            "/sources/:id/logs/stream",
            get(handlers::stream_source_logs_default),
        )
        .route("/sources/:id", delete(handlers::delete_source_default))
        .route("/sources/:id/start", post(handlers::start_source_default))
        .route("/sources/:id/stop", post(handlers::stop_source_default))
        // Query routes (default instance)
        .route("/queries", get(handlers::list_queries_default))
        .route("/queries", post(handlers::create_query_default))
        .route("/queries/:id", get(handlers::get_query_default))
        .route(
            "/queries/:id/events",
            get(handlers::get_query_events_default),
        )
        .route(
            "/queries/:id/events/stream",
            get(handlers::stream_query_events_default),
        )
        .route("/queries/:id/logs", get(handlers::get_query_logs_default))
        .route(
            "/queries/:id/logs/stream",
            get(handlers::stream_query_logs_default),
        )
        .route("/queries/:id", delete(handlers::delete_query_default))
        .route("/queries/:id/start", post(handlers::start_query_default))
        .route("/queries/:id/stop", post(handlers::stop_query_default))
        .route(
            "/queries/:id/results",
            get(handlers::get_query_results_default),
        )
        .route(
            "/queries/:id/attach",
            get(handlers::attach_query_stream_default),
        )
        // Reaction routes (default instance)
        .route("/reactions", get(handlers::list_reactions_default))
        .route("/reactions", post(handlers::create_reaction_default))
        .route("/reactions/:id", put(handlers::upsert_reaction_default))
        .route("/reactions/:id", get(handlers::get_reaction_default))
        .route(
            "/reactions/:id/events",
            get(handlers::get_reaction_events_default),
        )
        .route(
            "/reactions/:id/events/stream",
            get(handlers::stream_reaction_events_default),
        )
        .route(
            "/reactions/:id/logs",
            get(handlers::get_reaction_logs_default),
        )
        .route(
            "/reactions/:id/logs/stream",
            get(handlers::stream_reaction_logs_default),
        )
        .route("/reactions/:id", delete(handlers::delete_reaction_default))
        .route(
            "/reactions/:id/start",
            post(handlers::start_reaction_default),
        )
        .route("/reactions/:id/stop", post(handlers::stop_reaction_default))
}
