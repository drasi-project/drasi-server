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
    ApiResponse, ApiVersionsResponse, ComponentLinks, ComponentListItem, HealthResponse,
    InstanceListItem,
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

pub(crate) fn require_imperative_configuration(core: &DrasiLib) -> Result<(), ErrorResponse> {
    if core.configuration_is_persistent() {
        return Err(ErrorResponse::new(
            error_codes::MANAGED_CONFIGURATION_REQUIRED,
            "This instance uses durable desired state; submit a revisioned request to computation/desired",
        ));
    }
    Ok(())
}

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

pub(crate) fn component_links(
    api_prefix: &str,
    instance_id: &str,
    kind: &str,
    id: &str,
) -> ComponentLinks {
    const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let instance_id = percent_encoding::utf8_percent_encode(instance_id, SEGMENT);
    let id = percent_encoding::utf8_percent_encode(id, SEGMENT);
    let self_link = format!("{api_prefix}/instances/{instance_id}/{kind}/{id}");
    ComponentLinks {
        self_link: self_link.clone(),
        full: format!("{self_link}?view=full"),
    }
}

pub(crate) async fn component_list(
    core: &DrasiLib,
    kind: drasi_lib::ComponentKind,
    resource: &str,
    instance_id: &str,
    api_prefix: &str,
) -> Result<Vec<ComponentListItem>, ErrorResponse> {
    let snapshot = core.get_graph().await.map_err(ErrorResponse::from)?;
    snapshot
        .nodes
        .into_iter()
        .filter(|node| node.kind == kind)
        .map(|node| {
            let name = node.metadata.get("name").cloned().ok_or_else(|| {
                ErrorResponse::new(
                    error_codes::INTERNAL_ERROR,
                    format!("Component '{}' has no published name", node.id),
                )
            })?;
            Ok(ComponentListItem {
                links: component_links(api_prefix, instance_id, resource, &node.id),
                id: node.id,
                name,
                status: node.status,
                error_message: node.metadata.get("error").cloned(),
                config: None,
            })
        })
        .collect()
}

pub(crate) async fn managed_component(
    core: &DrasiLib,
    id: &str,
    resource: &str,
    instance_id: &str,
    api_prefix: &str,
    include_config: bool,
) -> Result<Option<ComponentListItem>, ErrorResponse> {
    use drasi_lib::management::OrdinaryComponent;
    if !core.has_managed_configuration() {
        return Ok(None);
    }
    let id = drasi_lib::config::ComponentId::try_new(id)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let current = core.desired_configuration().map_err(ErrorResponse::from)?;
    let Some(record) = current
        .desired
        .ordinary
        .iter()
        .find(|record| record.id == id)
    else {
        return Ok(None);
    };
    let graph = core.get_graph().await?;
    let Some(node) = graph.nodes.into_iter().find(|node| node.id == id.as_str()) else {
        return Ok(None);
    };
    let kind = match node.kind {
        drasi_lib::ComponentKind::Source => "sources",
        drasi_lib::ComponentKind::Query => "queries",
        drasi_lib::ComponentKind::Reaction => "reactions",
        _ => return Ok(None),
    };
    if kind != resource {
        return Ok(None);
    }
    if include_config && matches!(record.component, OrdinaryComponent::ProcessLocal { .. }) {
        return Ok(None);
    }
    let config = if include_config {
        let value = (|| -> anyhow::Result<serde_json::Value> {
            Ok(match &record.component {
                OrdinaryComponent::Source { .. } => serde_json::to_value(
                    crate::managed_configuration::ordinary::source_configuration(record)?,
                )?,
                OrdinaryComponent::Query { .. } => serde_json::to_value(
                    crate::api::models::QueryConfigDto::try_from(record.query_configuration()?)?,
                )?,
                OrdinaryComponent::Reaction { .. } => serde_json::to_value(
                    crate::managed_configuration::ordinary::reaction_configuration(record)?,
                )?,
                OrdinaryComponent::ProcessLocal { .. } => anyhow::bail!(
                    "Process-local component {id} has no reconstructible configuration record"
                ),
            })
        })()
        .map_err(|error| {
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Stored component configuration cannot be read: {error}"),
            )
        })?;
        Some(value)
    } else {
        None
    };
    Ok(Some(ComponentListItem {
        id: id.to_string(),
        name: record.name.to_string(),
        status: node.status,
        error_message: node.metadata.get("error").cloned(),
        links: component_links(api_prefix, instance_id, resource, id.as_str()),
        config,
    }))
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
    let handle = core.computation_component(id).map_err(|e| {
        health_error(format!("creation health could not be inspected: {e}")).with_cause(e)
    })?;
    tokio::time::timeout(COMPUTATION_CREATION_TIMEOUT, handle.wait_created())
        .await
        .map_err(|error| {
            health_error(format!(
                "creation was not confirmed within {} seconds; it may still be pending or blocked",
                COMPUTATION_CREATION_TIMEOUT.as_secs()
            ))
            .with_cause(error)
        })?
        .map_err(|e| health_error(format!("creation was not confirmed: {e}")).with_cause(e))?;
    Ok(handle)
}

/// Save host settings after an accepted operation. Component definitions may
/// already have durable receipts; a host-settings failure does not roll them back.
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
        Err(error) => Err(persistence_error(operation, error)),
    }
}

pub(super) fn persistence_error(operation: &str, error: anyhow::Error) -> ErrorResponse {
    log::error!("Failed to persist configuration after {operation}: {error}");
    ErrorResponse::new(
        error_codes::PERSISTENCE_FAILED,
        format!(
            "The operation was accepted, but Server settings could not be saved after \
             {operation}. Component configuration may already be durable. Inspect the \
             current settings and operation receipt before retrying."
        ),
    )
    .with_details(ErrorDetail {
        component_type: None,
        component_id: None,
        technical_details: Some(error.to_string()),
    })
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
    use super::{managed_component, wait_for_computation_creation};
    use async_trait::async_trait;
    use drasi_lib::{
        computation::v1::{
            ComponentDescriptor, ComponentId, ComputationComponent, GraphChangeCodec,
            InputEnvelope, OutputEnvelope, PipeRequirements, PortDescriptor, PortDirection, PortId,
            RealizationState, Transformer,
        },
        DrasiError,
    };
    use std::{sync::Arc, time::Duration};

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

    #[tokio::test]
    async fn computation_creation_health_inspection_keeps_its_graph_cause() {
        let core = crate::query_test_support::builder().build().await.unwrap();
        let missing_id = ComponentId::generate().to_string();
        let error = wait_for_computation_creation(&core, "source", &missing_id)
            .await
            .unwrap_err();
        assert!(error
            .downcast_ref::<drasi_lib::computation::v1::GraphError>()
            .is_some());
        assert!(matches!(
            error.classification(),
            DrasiError::OperationFailed { operation, component_id, reason, .. }
                if operation == "wait_created" && component_id == &missing_id
                    && reason.contains("could not be inspected")
        ));
        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn process_local_status_is_visible_before_construction_and_after_failure() {
        let core = crate::query_test_support::builder().build().await.unwrap();
        let started = Arc::new(tokio::sync::Notify::new());
        let entered = started.clone();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let handle = core
            .create_source_async::<Box<dyn drasi_lib::Source>, _, _>(
                "pending-source",
                None,
                false,
                move |_| async move {
                    entered.notify_one();
                    released.await?;
                    Err(anyhow::anyhow!("fixture construction failed"))
                },
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        let status = managed_component(
            &core,
            handle.id().as_str(),
            "sources",
            "instance",
            "/api/v1",
            false,
        )
        .await
        .unwrap()
        .expect("pending declaration is inspectable");
        assert_eq!(status.id, handle.id().as_str());
        assert_eq!(status.name, "pending-source");
        assert!(status.config.is_none());
        release.send(()).unwrap();
        assert!(handle.wait_created().await.is_err());
        let failed = managed_component(
            &core,
            handle.id().as_str(),
            "sources",
            "instance",
            "/api/v1",
            false,
        )
        .await
        .unwrap()
        .expect("failed declaration is inspectable");
        assert_eq!(failed.status, drasi_lib::ComponentStatus::Error);
        assert!(failed
            .error_message
            .unwrap()
            .contains("fixture construction failed"));
        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn computation_creation_health_does_not_wait_for_transformer_connections() {
        let core = crate::query_test_support::builder()
            .with_id("preconstructed-transformer-health")
            .with_allow_external_component_ids(true)
            .build()
            .await
            .unwrap();
        core.start().await.unwrap();
        let component_id = ComponentId::generate();
        let id = component_id.as_str();
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
            .add_transformer_with_handle(
                drasi_lib::config::ComponentName::try_new("unconnected-transformer").unwrap(),
                UnconnectedTransformer {
                    descriptor: ComponentDescriptor::try_new(component_id.clone(), ports).unwrap(),
                },
            )
            .await
            .unwrap();
        let created = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_computation_creation(&core, "transformer", id),
        )
        .await
        .expect("an existing object must not wait for activation dependencies")
        .unwrap();
        assert_eq!(created.generation(), handle.generation());
        let observed = handle.observed().unwrap();
        assert_eq!(observed.realization, RealizationState::Created);
        assert!(!observed.started);
        assert!(observed.failure.is_none(), "{observed:?}");
        core.shutdown().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn computation_creation_wait_times_out_after_30_seconds_for_pending_constructor() {
        let core = crate::query_test_support::builder()
            .with_construction_timeout_secs(120)
            .build()
            .await
            .unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let started_construction = entered.clone();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let handle = core
            .create_source_async::<Box<dyn drasi_lib::Source>, _, _>(
                "pending-timeout",
                None,
                false,
                move |_| async move {
                    started_construction.notify_one();
                    released.await?;
                    Err(anyhow::anyhow!("fixture construction failed"))
                },
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("the accepted constructor must start");
        let id = handle.id().as_str();

        let started = tokio::time::Instant::now();
        let mut creation = Box::pin(wait_for_computation_creation(&core, "source", id));
        assert!(futures_util::poll!(&mut creation).is_pending());
        tokio::time::advance(Duration::from_millis(29_999)).await;
        assert!(futures_util::poll!(&mut creation).is_pending());
        tokio::time::advance(Duration::from_millis(1)).await;
        let error = creation.await.unwrap_err();
        assert_eq!(started.elapsed(), Duration::from_secs(30));
        assert!(
            matches!(
                error.classification(),
                DrasiError::OperationFailed {
                    component_type,
                    component_id,
                    operation,
                    reason,
                } if component_type == "source"
                    && component_id == id
                    && operation == "wait_created"
                    && reason.contains("Node was added")
                    && reason.contains("30 seconds")
            ),
            "{error}"
        );
        assert!(error
            .downcast_ref::<tokio::time::error::Elapsed>()
            .is_some());
        let observed = handle.observed().unwrap();
        assert_eq!(observed.realization, RealizationState::Creating);
        assert!(observed.failure.is_none(), "{observed:?}");
        assert_eq!(
            core.computation_component(id).unwrap().generation(),
            handle.generation()
        );
        tokio::time::resume();
        release.send(()).unwrap();
        assert!(handle.wait_created().await.is_err());
        core.shutdown().await.unwrap();
    }
}
