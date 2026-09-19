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
use std::time::Duration;

use super::error::{error_codes, ErrorDetail, ErrorResponse};
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

const COMPUTATION_CREATION_TIMEOUT: Duration = Duration::from_secs(30);

// Limit the health wait, not the already-committed node's realization.
pub(crate) async fn wait_for_computation_creation(
    core: &DrasiLib,
    component_type: &str,
    id: &str,
) -> drasi_lib::Result<drasi_lib::computation::v1::ComponentHandle> {
    let health_error = |reason: String| {
        drasi_lib::DrasiError::operation_failed(
            component_type,
            id,
            "wait_created",
            format!("Node was added, but {reason}"),
        )
    };
    let handle = core
        .computation_component(id)
        .map_err(|e| health_error(format!("creation health could not be inspected: {e}")))?;
    tokio::time::timeout(COMPUTATION_CREATION_TIMEOUT, handle.wait_created())
        .await
        .map_err(|_| {
            health_error(format!(
                "creation was not confirmed within {} seconds; it may still be pending or blocked",
                COMPUTATION_CREATION_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| health_error(format!("creation was not confirmed: {e}")))?;
    Ok(handle)
}

/// Helper to persist configuration after a successful in-memory mutation.
///
/// **Important contract:** the in-memory state has already been mutated
/// before this is called. If persistence fails, the runtime is now ahead of
/// the on-disk YAML and the operator must retry or restart after fixing the
/// underlying issue. To make this visible to API callers, persistence
/// failures are surfaced as a [`ErrorResponse`] with code
/// [`error_codes::PERSISTENCE_FAILED`] (HTTP 500). Handlers should
/// `?`-propagate the result.
///
/// The high-level message states the in-memory/on-disk divergence
/// explicitly; the underlying technical error is placed in
/// [`ErrorDetail::technical_details`] (never embedded in `message`) per the
/// project's error-handling convention.
///
/// When persistence is disabled (no config file or `persistConfig: false`),
/// this is a no-op and returns `Ok(())`.
pub async fn persist_after_operation(
    config_persistence: &Option<Arc<ConfigPersistence>>,
    operation: &str,
) -> Result<(), ErrorResponse> {
    let Some(persistence) = config_persistence else {
        return Ok(());
    };
    match persistence.save().await {
        Ok(()) => Ok(()),
        Err(e) => {
            log::error!("Failed to persist configuration after {operation}: {e}");
            Err(ErrorResponse::new(
                error_codes::PERSISTENCE_FAILED,
                format!(
                    "Configuration change applied in memory but could not be persisted to disk \
                     after {operation}. The runtime state has changed; the on-disk \
                     configuration has not. Retry the operation or restart the server \
                     after fixing the underlying persistence issue."
                ),
            )
            .with_details(ErrorDetail {
                component_type: None,
                component_id: None,
                technical_details: Some(e.to_string()),
            }))
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

#[cfg(test)]
mod tests {
    use super::wait_for_computation_creation;
    use async_trait::async_trait;
    use drasi_lib::{
        computation::v1::{
            ComponentDescriptor, ComponentId, ComputationComponent, GraphChangeCodec,
            InputEnvelope, OutputEnvelope, PipeRequirements, PortDescriptor, PortDirection, PortId,
            RealizationState, Transformer,
        },
        DrasiError, DrasiLib, ExecutionMode,
    };
    use std::{task::Poll, time::Duration};

    struct UnconnectedTransformer {
        descriptor: ComponentDescriptor,
    }

    #[async_trait]
    impl ComputationComponent for UnconnectedTransformer {
        fn descriptor(&self) -> &ComponentDescriptor {
            &self.descriptor
        }

        async fn start(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl Transformer for UnconnectedTransformer {
        async fn transform(
            &mut self,
            _input: InputEnvelope,
        ) -> anyhow::Result<Vec<OutputEnvelope>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn computation_creation_wait_times_out_after_30_seconds_for_blocked_transformer() {
        let core = DrasiLib::builder()
            .with_id("blocked-transformer-timeout")
            .with_execution_mode(ExecutionMode::ComputationGraph)
            .build()
            .await
            .unwrap();
        core.start().await.unwrap();
        let id = "unconnected-transformer";
        let ports = [("in", PortDirection::Input), ("out", PortDirection::Output)]
            .into_iter()
            .map(|(name, direction)| {
                PortDescriptor::new(
                    PortId::try_new(name).unwrap(),
                    direction,
                    GraphChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                )
            })
            .collect();
        let handle = core
            .add_transformer_with_handle(UnconnectedTransformer {
                descriptor: ComponentDescriptor::try_new(ComponentId::try_new(id).unwrap(), ports)
                    .unwrap(),
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let observed = handle.observed().unwrap();
                assert!(observed.failure.is_none(), "{observed:?}");
                if observed.realization == RealizationState::Blocked {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a valid transformer with unconnected ports must become Blocked");

        let started = tokio::time::Instant::now();
        let mut creation = Box::pin(wait_for_computation_creation(&core, "transformer", id));
        assert!(futures_util::poll!(&mut creation).is_pending());
        tokio::time::advance(Duration::from_millis(29_999)).await;
        assert!(futures_util::poll!(&mut creation).is_pending());
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        let error = match futures_util::poll!(&mut creation) {
            Poll::Ready(result) => result.unwrap_err(),
            Poll::Pending => panic!("the creation health wait must expire at 30 seconds"),
        };
        assert_eq!(started.elapsed(), Duration::from_secs(30));
        assert!(
            matches!(
                &error,
                DrasiError::OperationFailed {
                    component_type,
                    component_id,
                    operation,
                    reason,
                } if component_type == "transformer"
                    && component_id == id
                    && operation == "wait_created"
                    && reason.contains("Node was added")
                    && reason.contains("30 seconds")
            ),
            "{error}"
        );
        let observed = handle.observed().unwrap();
        assert_eq!(observed.realization, RealizationState::Blocked);
        assert!(observed.failure.is_none(), "{observed:?}");
        assert_eq!(
            core.computation_component(id).unwrap().generation(),
            handle.generation()
        );
        tokio::time::resume();
        core.shutdown().await.unwrap();
    }
}
