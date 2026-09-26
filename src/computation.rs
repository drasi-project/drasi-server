// Copyright 2026 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use drasi_host_sdk::management::HostConfigurationResolver;
use drasi_lib::{computation::v1::*, DrasiLib};
use serde::{Deserialize, Serialize};

fn default_auto_start() -> bool {
    true
}

/// A user-defined graph on the sole runtime, not an execution-engine selector.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputationGraphConfig {
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    #[schema(value_type = serde_json::Value)]
    pub definition: DesiredTopology,
}

/// Provider recipes stored in DesiredTopology::resource_configurations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ComputationResourceConfig {
    MemoryIndexes,
    RocksdbIndexes { path: PathBuf },
    Middleware,
    QueryMiddleware,
    TransactionalTransformers,
    Configuration,
}

impl ComputationResourceConfig {
    fn role(&self) -> ResourceRole {
        match self {
            Self::MemoryIndexes | Self::RocksdbIndexes { .. } => ResourceRole::IndexBackend,
            Self::Middleware | Self::QueryMiddleware => ResourceRole::Middleware,
            Self::TransactionalTransformers => ResourceRole::Component,
            Self::Configuration => ResourceRole::SecretStore,
        }
    }
}

/// Reject host bindings that cannot be reconstructed before changing an instance.
pub fn validate_definition(config: &ComputationGraphConfig) -> Result<()> {
    let definition = &config.definition;
    anyhow::ensure!(
        definition.version == 1,
        "unsupported computation graph configuration version"
    );
    ComponentId::try_new(definition.graph_id.as_str())?;
    anyhow::ensure!(
        definition
            .components
            .iter()
            .all(|component| matches!(component.construction, ComponentConstruction::Factory(_))),
        "server computation graphs require factory specifications, not external component bindings"
    );
    anyhow::ensure!(
        definition.boundary_relationships.is_empty()
            && definition
                .relationships
                .iter()
                .all(|edge| !matches!(edge.pipe, DesiredPipe::External { .. })),
        "server computation graphs cannot reconstruct external pipe or boundary bindings"
    );
    let declarations: BTreeMap<_, _> = definition
        .resources
        .iter()
        .map(|resource| (resource.id.clone(), resource))
        .collect();
    anyhow::ensure!(
        declarations.len() == definition.resources.len(),
        "duplicate computation resource declarations"
    );
    for id in definition.resource_configurations.keys() {
        anyhow::ensure!(
            declarations.contains_key(id),
            "resource recipe {id} has no declaration"
        );
    }
    for resource in &definition.resources {
        let recipe = definition.resource_configurations.get(&resource.id)
            .with_context(|| format!("resource {} has no construction recipe; external bindings cannot be persisted or cloned", resource.id))?;
        let recipe: ComputationResourceConfig = serde_json::from_value(recipe.clone())
            .with_context(|| format!("invalid configuration for resource {}", resource.id))?;
        anyhow::ensure!(
            recipe.role() == resource.role,
            "resource {} role differs from its recipe",
            resource.id
        );
        if let ComputationResourceConfig::RocksdbIndexes { path } = recipe {
            anyhow::ensure!(
                !path.as_os_str().is_empty(),
                "RocksDB path must not be empty"
            );
            anyhow::ensure!(
                resource.ownership == ResourceOwnership::Graph,
                "server-created RocksDB resources require graph cleanup ownership"
            );
        }
    }
    let require_resource = |id: &ResourceId| {
        declarations.get(id).copied().with_context(|| {
            format!("required resource {id} has no declaration or construction recipe")
        })
    };
    for component in &definition.components {
        if let ComponentConstruction::Factory(specification) = &component.construction {
            for id in specification.dependencies.values().flatten() {
                require_resource(id)?;
            }
            for value in specification.configuration.values() {
                if let ConfigurationValue::Reference { resource, .. } = value {
                    require_resource(resource)?;
                }
            }
        }
    }
    for id in definition.component_resources.values().flatten() {
        require_resource(id)?;
    }
    for relationship in &definition.relationships {
        let requirements = match &relationship.pipe {
            DesiredPipe::Retained(pipe) => PipeProvider::resource_dependencies(pipe),
            DesiredPipe::Ranked(pipe) => PipeProvider::resource_dependencies(pipe),
            DesiredPipe::Bounded { .. } | DesiredPipe::Broadcast { .. } => continue,
            DesiredPipe::External { .. } => {
                anyhow::bail!("external pipe binding cannot be reconstructed")
            }
        };
        for (id, role) in requirements {
            anyhow::ensure!(
                require_resource(&id)?.role == role,
                "pipe resource {id} has an incompatible declared role"
            );
        }
    }
    Ok(())
}

/// Bind only explicitly declared host resources. Their recipes stay in the
/// graph's desired configuration and are not recovered from live object pointers.
pub async fn build_graph(
    config: &ComputationGraphConfig,
    core: &DrasiLib,
    factories: FactoryRegistry,
    transactional: Arc<TransactionalTransformerRegistry>,
) -> Result<ComputationGraph> {
    validate_definition(config)?;
    let services = core.computation_plugin_services(&config.definition.graph_id)?;
    let mut bindings = TopologyBindings {
        factories,
        ..Default::default()
    };
    for resource in &config.definition.resources {
        let recipe = config
            .definition
            .resource_configurations
            .get(&resource.id)
            .with_context(|| format!("resource {} has no construction recipe", resource.id))?;
        let recipe: ComputationResourceConfig = serde_json::from_value(recipe.clone())
            .with_context(|| format!("invalid configuration for resource {}", resource.id))?;
        anyhow::ensure!(
            recipe.role() == resource.role,
            "resource {} role differs from its recipe",
            resource.id
        );
        let handle = match recipe {
            ComputationResourceConfig::MemoryIndexes => ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(Arc::new(
                    drasi_core::computation::InMemoryComputationProvider,
                ))),
            ),
            ComputationResourceConfig::RocksdbIndexes { path } => {
                anyhow::ensure!(
                    !path.as_os_str().is_empty(),
                    "RocksDB path must not be empty"
                );
                anyhow::ensure!(
                    resource.ownership == ResourceOwnership::Graph,
                    "server-created RocksDB resources require graph cleanup ownership"
                );
                LegacyIndexProviderAdapter::scoped(
                    Arc::new(drasi_index_rocksdb::RocksDbIndexProvider::new(
                        path, false, false,
                    )),
                    services.scope.clone(),
                )?
                .resource()
            }
            ComputationResourceConfig::Middleware => ResourceHandle::new(
                ResourceRole::Middleware,
                Arc::new(MiddlewareRegistryResource(core.middleware_registry())),
            ),
            ComputationResourceConfig::QueryMiddleware => ResourceHandle::new(
                ResourceRole::Middleware,
                Arc::new(QueryMiddlewareResource(core.middleware_registry())),
            ),
            ComputationResourceConfig::TransactionalTransformers => ResourceHandle::new(
                ResourceRole::Component,
                Arc::new(TransactionalTransformerRegistryResource(
                    transactional.clone(),
                )),
            ),
            ComputationResourceConfig::Configuration => ResourceHandle::new(
                ResourceRole::SecretStore,
                Arc::new(ConfigurationResolverResource(Arc::new(
                    HostConfigurationResolver::new(services.secrets.clone()),
                ))),
            ),
        };
        bindings.resources.insert(resource.id.clone(), handle);
    }
    config
        .definition
        .build(bindings)
        .map_err(anyhow::Error::from)
}

pub fn configurations_from_snapshot(
    snapshot: &InstanceConfigurationSnapshot,
) -> Result<Vec<ComputationGraphConfig>> {
    anyhow::ensure!(
        snapshot.version == 1,
        "unsupported instance configuration snapshot version"
    );
    anyhow::ensure!(
        snapshot.native_components.is_none(),
        "native root components cannot be saved as separate server graphs; supply named graph definitions"
    );
    snapshot
        .graphs
        .iter()
        .map(|graph| {
            let config = ComputationGraphConfig {
                auto_start: graph.options.auto_start,
                definition: graph.graph.topology.clone(),
            };
            validate_definition(&config).with_context(|| {
                format!(
                    "computation graph '{}' cannot be persisted or cloned",
                    config.definition.graph_id
                )
            })?;
            Ok(config)
        })
        .collect()
}

pub async fn register_graph(
    config: &ComputationGraphConfig,
    core: &DrasiLib,
    registry: &crate::plugin_registry::PluginRegistry,
) -> Result<ComputationHandle> {
    let graph = build_graph(
        config,
        core,
        registry.computation_factory_registry()?,
        registry.transactional_transformer_registry(core.middleware_registry())?,
    )
    .await?;
    Ok(core
        .add_computation_graph(
            graph,
            ComputationOptions {
                auto_start: config.auto_start,
            },
        )
        .await?)
}

/// Retains instances if asynchronous rollback fails, so callers can retry cleanup.
pub struct PreparationFailure {
    cause: anyhow::Error,
    cores: Vec<DrasiLib>,
    cleanup_errors: Vec<String>,
}

impl PreparationFailure {
    pub async fn cleanup(&self) -> Result<()> {
        let mut errors = Vec::new();
        let nested: Vec<_> = self
            .cause
            .chain()
            .filter_map(|cause| cause.downcast_ref::<PreparationFailure>())
            .collect();
        for failure in nested {
            if let Err(error) = Box::pin(failure.cleanup()).await {
                errors.push(error.to_string());
            }
        }
        let pending: Vec<_> = self
            .cause
            .chain()
            .filter_map(|cause| cause.downcast_ref::<ComputationCleanupError>())
            .collect();
        for error in pending {
            if let Err(error) = error.cleanup().await {
                errors.push(error.to_string());
            }
        }
        for core in &self.cores {
            if let Err(error) = core.shutdown().await {
                errors.push(error.to_string());
            }
        }
        anyhow::ensure!(
            errors.is_empty(),
            "instance cleanup failed: {}",
            errors.join("; ")
        );
        Ok(())
    }
}

impl std::fmt::Debug for PreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparationFailure")
            .field("cause", &self.cause)
            .field("cleanup_errors", &self.cleanup_errors)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for PreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}; cleanup remains owned and retryable: {}",
            self.cause,
            self.cleanup_errors.join("; ")
        )
    }
}

impl std::error::Error for PreparationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

pub(crate) async fn cleanup_failed_preparation(
    cores: Vec<DrasiLib>,
    cause: anyhow::Error,
) -> anyhow::Error {
    let mut failure = PreparationFailure {
        cause,
        cores,
        cleanup_errors: Vec::new(),
    };
    match failure.cleanup().await {
        Ok(()) => failure.cause,
        Err(error) => {
            log::error!("Server preparation rollback failed: {error:#}");
            failure.cleanup_errors.push(format!("{error:#}"));
            anyhow::Error::new(failure)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_explicit_and_do_not_silently_coerce_values() {
        for key in [
            "env:COUNT",
            "env-json:COUNT",
            "secret:password",
            "secret-json:options",
        ] {
            assert!(reference(key).is_ok());
        }
        for key in ["COUNT", "unknown:COUNT", "secret:", "env:bad\nname"] {
            assert!(reference(key).is_err(), "{key}");
        }
    }

    #[tokio::test]
    async fn secret_references_preserve_strings_unless_json_was_requested() {
        let secrets = drasi_lib::secret_store::MemorySecretStoreProvider::new()
            .with_secret("number", "42")
            .with_secret("invalid-json", "not json");
        let resolver = InstanceConfigurationResolver {
            secrets: Some(Arc::new(secrets)),
        };
        assert_eq!(
            resolver.resolve("secret:number").await.unwrap(),
            serde_json::json!("42")
        );
        assert_eq!(
            resolver.resolve("secret-json:number").await.unwrap(),
            serde_json::json!(42)
        );
        assert!(resolver.resolve("secret:missing").await.is_err());
        assert!(resolver.resolve("secret-json:invalid-json").await.is_err());
        let missing = format!("env:DRASI_ABSENT_{}", uuid::Uuid::new_v4().simple());
        assert!(resolver.resolve(&missing).await.is_err());
        assert_eq!(
            resolver.resolve("env:PATH").await.unwrap(),
            serde_json::json!(std::env::var("PATH").unwrap())
        );
    }
}
