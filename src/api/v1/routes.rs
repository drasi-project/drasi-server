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
use indexmap::IndexMap;
use std::sync::Arc;

use super::handlers;
use crate::persistence::ConfigPersistence;

/// Build the complete v1 API router.
///
/// This function constructs all v1 routes and returns a router that should
/// be nested under `/api/v1/` in the main application.
///
/// # Arguments
///
/// * `instances` - Map of instance IDs to their DrasiLib cores
/// * `read_only` - Whether the server is in read-only mode
/// * `config_persistence` - Optional configuration persistence handler
///
/// # Returns
///
/// A Router containing all v1 API routes.
pub fn build_v1_router(
    instances: Arc<IndexMap<String, Arc<drasi_lib::DrasiLib>>>,
    read_only: Arc<bool>,
    config_persistence: Option<Arc<ConfigPersistence>>,
) -> Router {
    let mut router = Router::new()
        // Instance listing
        .route("/instances", get(handlers::list_instances));

    // Build instance-specific routes for each instance
    for (instance_id, core) in instances.iter() {
        let instance_router = build_instance_router(
            core.clone(),
            read_only.clone(),
            config_persistence.clone(),
            instance_id.clone(),
        );
        router = router.nest(&format!("/instances/{instance_id}"), instance_router);
    }

    // Add convenience routes for the first (default) instance
    if let Some((instance_id, core)) = instances.iter().next() {
        let default_router = build_instance_router(
            core.clone(),
            read_only.clone(),
            config_persistence.clone(),
            instance_id.clone(),
        );
        router = router.merge(default_router);
    }

    // Add the instances extension for the list_instances handler
    router.layer(Extension(instances))
}

/// Build routes for a specific DrasiLib instance.
///
/// These routes handle sources, queries, and reactions for a single instance.
fn build_instance_router(
    core: Arc<drasi_lib::DrasiLib>,
    read_only: Arc<bool>,
    config_persistence: Option<Arc<ConfigPersistence>>,
    instance_id: String,
) -> Router {
    Router::new()
        // Source routes
        .route("/sources", get(handlers::list_sources))
        .route("/sources", post(handlers::create_source_handler))
        .route("/sources", put(handlers::upsert_source_handler))
        .route("/sources/:id", get(handlers::get_source))
        .route("/sources/:id/events", get(handlers::get_source_events))
        .route("/sources/:id/events/stream", get(handlers::stream_source_events))
        .route("/sources/:id/logs", get(handlers::get_source_logs))
        .route("/sources/:id/logs/stream", get(handlers::stream_source_logs))
        .route("/sources/:id", delete(handlers::delete_source))
        .route("/sources/:id/start", post(handlers::start_source))
        .route("/sources/:id/stop", post(handlers::stop_source))
        // Query routes
        .route("/queries", get(handlers::list_queries))
        .route("/queries", post(handlers::create_query))
        .route("/queries/:id", get(handlers::get_query))
        .route("/queries/:id/events", get(handlers::get_query_events))
        .route("/queries/:id/events/stream", get(handlers::stream_query_events))
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
        .route("/reactions", put(handlers::upsert_reaction_handler))
        .route("/reactions/:id", get(handlers::get_reaction))
        .route("/reactions/:id/events", get(handlers::get_reaction_events))
        .route("/reactions/:id/events/stream", get(handlers::stream_reaction_events))
        .route("/reactions/:id/logs", get(handlers::get_reaction_logs))
        .route("/reactions/:id/logs/stream", get(handlers::stream_reaction_logs))
        .route("/reactions/:id", delete(handlers::delete_reaction))
        .route("/reactions/:id/start", post(handlers::start_reaction))
        .route("/reactions/:id/stop", post(handlers::stop_reaction))
        // Add extensions
        .layer(Extension(core))
        .layer(Extension(read_only))
        .layer(Extension(config_persistence))
        .layer(Extension(instance_id))
}
