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

//! Shared handler implementations used across API versions.
//!
//! These handler functions contain the core business logic that can be
//! reused by version-specific handlers. Each API version may wrap these
//! with version-specific path annotations.

mod instance_handlers;
mod query_handlers;
mod reaction_handlers;
mod source_handlers;

pub use instance_handlers::*;
pub use query_handlers::*;
pub use reaction_handlers::*;
pub use source_handlers::*;

use axum::{
    extract::Extension,
    response::{sse::Event, Json},
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

use super::error::{error_codes, ErrorResponse};
use super::responses::{
    ApiResponse, ApiVersionsResponse, ComponentLinks, HealthResponse, InstanceListItem,
};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use drasi_lib::DrasiLib;

/// The URL path prefix for the current API version (e.g., `/api/v1`).
///
/// Injected as an Axum `Extension` so that shared handlers can build
/// version-correct HATEOAS links without hardcoding a version string.
#[derive(Debug, Clone)]
pub struct ApiPrefix(pub String);

/// Path parameters for instance-specific routes
#[derive(Debug, Deserialize)]
pub struct InstancePath {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
}

/// Path parameters for resource-specific routes
#[derive(Debug, Deserialize)]
pub struct ResourcePath {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    pub id: String,
}

/// Helper to get an instance from the registry, returning an error response if not found
pub async fn get_instance_or_error(
    registry: &InstanceRegistry,
    instance_id: &str,
) -> Result<Arc<DrasiLib>, ErrorResponse> {
    match registry.get(instance_id).await {
        Some(core) => Ok(core),
        None => Err(ErrorResponse::new(
            error_codes::INSTANCE_NOT_FOUND,
            format!("Instance '{instance_id}' not found"),
        )),
    }
}

/// Helper to get the default instance from the registry
pub async fn get_default_instance_or_error(
    registry: &InstanceRegistry,
) -> Result<(String, Arc<DrasiLib>), ErrorResponse> {
    match registry.get_default().await {
        Some((id, core)) => Ok((id, core)),
        None => Err(ErrorResponse::new(
            error_codes::INSTANCE_NOT_FOUND,
            "No instances configured",
        )),
    }
}

pub(crate) fn component_links(
    api_prefix: &str,
    instance_id: &str,
    kind: &str,
    id: &str,
) -> ComponentLinks {
    let self_link = format!("{api_prefix}/instances/{instance_id}/{kind}/{id}");
    ComponentLinks {
        self_link: self_link.clone(),
        full: format!("{self_link}?view=full"),
    }
}

#[derive(Debug, Deserialize)]
pub struct ComponentViewQuery {
    view: Option<String>,
}

impl ComponentViewQuery {
    pub fn new(view: Option<String>) -> Self {
        Self { view }
    }

    pub(crate) fn include_config(&self) -> bool {
        matches!(self.view.as_deref(), Some("full"))
    }
}

const DEFAULT_OBSERVABILITY_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ObservabilityQuery {
    pub limit: Option<usize>,
}

pub(crate) fn apply_limit<T>(mut items: Vec<T>, limit: Option<usize>) -> Vec<T> {
    let limit = limit.unwrap_or(DEFAULT_OBSERVABILITY_LIMIT);
    if limit == 0 {
        return Vec::new();
    }
    if items.len() > limit {
        let start = items.len() - limit;
        items = items.split_off(start);
    }
    items
}

pub(crate) fn sse_event<T: Serialize>(payload: T) -> Option<Result<Event, Infallible>> {
    match Event::default().json_data(payload) {
        Ok(event) => Some(Ok(event)),
        Err(e) => {
            log::warn!("Failed to serialize SSE payload: {e}");
            None
        }
    }
}

pub(crate) async fn sse_event_async<T: Serialize>(payload: T) -> Option<Result<Event, Infallible>> {
    sse_event(payload)
}

/// Helper function to persist configuration after a successful operation.
/// Logs errors but does not fail the request - persistence failures are non-fatal.
pub async fn persist_after_operation(
    config_persistence: &Option<Arc<ConfigPersistence>>,
    operation: &str,
) {
    if let Some(persistence) = config_persistence {
        if let Err(e) = persistence.save().await {
            log::error!("Failed to persist configuration after {operation}: {e}");
        }
    }
}

/// List available API versions
pub async fn list_api_versions() -> Json<ApiVersionsResponse> {
    Json(ApiVersionsResponse {
        versions: vec!["v1".to_string()],
        current: "v1".to_string(),
    })
}

/// Check server health
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// List configured DrasiLib instances
pub async fn list_instances(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(api_prefix): Extension<ApiPrefix>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    let instances = registry.list().await;
    let mut data = Vec::with_capacity(instances.len());

    for (id, instance) in instances {
        let source_count = instance.list_sources().await.map(|v| v.len()).unwrap_or(0);
        let query_count = instance.list_queries().await.map(|v| v.len()).unwrap_or(0);
        let reaction_count = instance
            .list_reactions()
            .await
            .map(|v| v.len())
            .unwrap_or(0);

        let base_path = format!("{}/instances/{id}", api_prefix.0);
        data.push(InstanceListItem {
            id: id.clone(),
            source_count,
            query_count,
            reaction_count,
            links: crate::api::shared::InstanceLinks {
                self_link: base_path.clone(),
                sources: format!("{base_path}/sources"),
                queries: format!("{base_path}/queries"),
                reactions: format!("{base_path}/reactions"),
            },
        });
    }

    Json(ApiResponse::success(data))
}
