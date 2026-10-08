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
use async_trait::async_trait;
use drasi_core::computation::ComputationIndexProvider;
use drasi_host_sdk::{
    computation::NativeBootstrapFactories,
    management::{
        HostConfigurationResolver, NativeBootstrapConfig, NativeConsumerConfig, QosRecoveryConfig,
        SharedQosConfig,
    },
};
use drasi_lib::{
    computation::v1::*, management::ManagementResourceResolver, secret_store::SecretStoreProvider,
    DrasiLib,
};
use serde::{Deserialize, Serialize};

/// Native components, connections and resource recipes for a DrasiLib instance.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputationConfig {
    #[schema(value_type = serde_json::Value)]
    pub definition: DesiredTopology,
}

/// Provider recipes stored in DesiredTopology::resource_configurations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ComputationResourceConfig {
    MemoryIndexes,
    QueryCatalog {},
    SourceProgress {
        #[schema(value_type = String)]
        component: ComponentId,
    },
    RocksdbIndexes {
        path: PathBuf,
    },
    SharedStorage {
        path: PathBuf,
        #[schema(value_type = String)]
        component: ComponentId,
    },
    SharedQos(#[schema(value_type = serde_json::Value)] SharedQosConfig),
    NativeConsumer(#[schema(value_type = serde_json::Value)] NativeConsumerConfig),
    NativeBootstrap(#[schema(value_type = serde_json::Value)] NativeBootstrapConfig),
    Middleware,
    QueryMiddleware,
    TransactionalTransformers,
    Configuration,
    Qos {
        #[schema(value_type = serde_json::Value)]
        definition: QosChannelDefinition,
        path: Option<PathBuf>,
        #[schema(value_type = Option<serde_json::Value>)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recovery: Option<QosRecoveryConfig>,
    },
}

impl ComputationResourceConfig {
    fn role(&self) -> ResourceRole {
        match self {
            Self::MemoryIndexes
            | Self::RocksdbIndexes { .. }
            | Self::SharedStorage { .. }
            | Self::NativeConsumer(_) => ResourceRole::IndexBackend,
            Self::QueryCatalog {} => ResourceRole::QueryCatalog,
            Self::NativeBootstrap(_) => ResourceRole::Bootstrap,
            Self::SourceProgress { .. } => ResourceRole::Checkpoint,
            Self::Middleware | Self::QueryMiddleware => ResourceRole::Middleware,
            Self::TransactionalTransformers => ResourceRole::Component,
            Self::Configuration => ResourceRole::SecretStore,
            Self::Qos { .. } | Self::SharedQos(_) => ResourceRole::StateStore,
        }
    }
}

/// Reject host bindings that cannot be reconstructed before changing an instance.
pub fn validate_definition(config: &ComputationConfig) -> Result<()> {
    let definition = &config.definition;
    anyhow::ensure!(
        definition.version == 1,
        "unsupported computation graph configuration version"
    );
    ComponentId::try_new(definition.graph_id.as_str())?;
    anyhow::ensure!(
        definition.graph_id
            == drasi_lib::management::DesiredInstance::default()
                .topology
                .graph_id,
        "component definition must use the instance graph identity"
    );
    anyhow::ensure!(
        definition
            .components
            .iter()
            .all(|component| matches!(component.construction, ComponentConstruction::Factory(_))),
        "server computation components require factory specifications, not external component bindings"
    );
    anyhow::ensure!(
        definition.boundary_relationships.is_empty()
            && definition
                .relationships
                .iter()
                .all(|edge| !matches!(edge.pipe, DesiredPipe::External { .. })),
        "server computation cannot reconstruct external pipe or boundary bindings"
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
        let dependencies = definition
            .resource_dependencies
            .get(&resource.id)
            .cloned()
            .unwrap_or_default();
        if let ComputationResourceConfig::SharedQos(config) = &recipe {
            config.validate(resource)?;
            anyhow::ensure!(
                dependencies.len() == 1
                    && dependencies
                        .values()
                        .all(|role| *role == ResourceRole::IndexBackend),
                "shared QoS must declare exactly its storage-group dependency"
            );
        } else if let ComputationResourceConfig::NativeConsumer(config) = &recipe {
            config.validate_dependencies(resource, &dependencies)?;
        } else if let ComputationResourceConfig::NativeBootstrap(config) = &recipe {
            config.validate(resource, &dependencies)?;
            config.validate_owner(&resource.id, definition)?;
        } else {
            anyhow::ensure!(
                dependencies.is_empty(),
                "this server resource recipe does not accept dependencies"
            );
        }
        if let ComputationResourceConfig::Qos {
            definition,
            path,
            recovery,
        } = &recipe
        {
            definition.validate()?;
            recovery
                .as_ref()
                .map(|recovery| recovery.options("validation", &config.definition.graph_id))
                .unwrap_or_default()
                .validate(definition)?;
            anyhow::ensure!(
                definition.durable == path.is_some()
                    && path
                        .as_ref()
                        .is_none_or(|path| !path.as_os_str().is_empty()),
                "durable QoS requires a nonempty storage path; volatile QoS must omit it"
            );
            anyhow::ensure!(
                resource.ownership == ResourceOwnership::Graph,
                "server QoS requires graph ownership"
            );
        }
        if let ComputationResourceConfig::RocksdbIndexes { path }
        | ComputationResourceConfig::SharedStorage { path, .. } = recipe
        {
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
            let specifications: BTreeMap<_, _> = definition
                .components
                .iter()
                .filter_map(|component| match &component.construction {
                    ComponentConstruction::Factory(spec) => Some((spec.descriptor.id(), spec)),
                    _ => None,
                })
                .collect();
            let query_factory = ContinuousQueryFactory::default();
            let outlet_factory = QueryResultsOutletFactory::default();
            for relationship in &definition.relationships {
                let edge = &relationship.definition;
                let (Some(query), Some(outlet)) = (
                    specifications.get(&edge.from.component),
                    specifications.get(&edge.to.component),
                ) else {
                    continue;
                };
                if query.implementation != query_factory.descriptor().implementation
                    || outlet.implementation != outlet_factory.descriptor().implementation
                {
                    continue;
                }
                let query_catalog = result_catalog_dependency(query)?;
                let outlet_catalog = result_catalog_dependency(outlet)?;
                anyhow::ensure!(
                    require_resource(query_catalog)?.role == ResourceRole::QueryCatalog,
                    "query '{}' catalog dependency must reference a queryCatalog resource",
                    query.descriptor.id()
                );
                anyhow::ensure!(
                    query_catalog == outlet_catalog,
                    "query '{}' and result outlet '{}' must reference the same queryCatalog resource",
                    query.descriptor.id(),
                    outlet.descriptor.id()
                );
            }
        }
    }
    for id in definition.component_resources.values().flatten() {
        require_resource(id)?;
    }
    for relationship in &definition.relationships {
        let requirements = match &relationship.pipe {
            DesiredPipe::Retained(pipe) => PipeProvider::resource_dependencies(pipe),
            DesiredPipe::Qos(pipe) => PipeProvider::resource_dependencies(pipe),
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
    definition.validate_structure()?;
    Ok(())
}

fn result_catalog_dependency(specification: &ComponentSpecification) -> Result<&ResourceId> {
    specification
        .dependencies
        .get("catalog")
        .filter(|resources| resources.len() == 1)
        .and_then(|resources| resources.first())
        .with_context(|| {
            format!(
                "component '{}' connected to a query result outlet requires exactly one catalog dependency",
                specification.descriptor.id()
            )
        })
}

/// Bind only explicitly declared host resources. Their recipes stay in the
/// graph's desired configuration and are not recovered from live object pointers.
pub async fn build_components(
    config: &ComputationConfig,
    core: &DrasiLib,
    factories: FactoryRegistry,
    transactional: Arc<TransactionalTransformerRegistry>,
) -> Result<ComponentBatch> {
    build_components_with_resources(
        config,
        core,
        factories,
        transactional,
        NativeBootstrapFactories::new(),
    )
    .await
}

/// Include all independently negotiated native services from the loaded registry.
pub async fn build_components_with_registry(
    config: &ComputationConfig,
    core: &DrasiLib,
    registry: &crate::plugin_registry::PluginRegistry,
) -> Result<ComponentBatch> {
    build_components_with_resources(
        config,
        core,
        registry.computation_factory_registry()?,
        registry.transactional_transformer_registry(core.middleware_registry())?,
        registry.computation_bootstrap_factories()?,
    )
    .await
}

async fn build_components_with_resources(
    config: &ComputationConfig,
    core: &DrasiLib,
    factories: FactoryRegistry,
    transactional: Arc<TransactionalTransformerRegistry>,
    bootstraps: NativeBootstrapFactories,
) -> Result<ComponentBatch> {
    validate_definition(config)?;
    for component in &config.definition.components {
        if let ComponentConstruction::Factory(specification) = &component.construction {
            factories
                .get(&specification.implementation)
                .with_context(|| {
                    format!(
                        "component factory '{}' is not registered",
                        specification.implementation.name
                    )
                })?
                .validate(specification)
                .with_context(|| {
                    format!("invalid component '{}'", specification.descriptor.id())
                })?;
        }
    }
    let services = core.computation_plugin_services()?;
    let resolver = Arc::new(ServerManagementResources {
        factories: factories.clone(),
        middleware: core.middleware_registry(),
        transactional,
        bootstraps,
        secrets: services.secrets,
    });
    resolver.validate_transition(
        &drasi_lib::management::DesiredInstance::default().topology,
        &config.definition,
    )?;
    let mut bindings = TopologyBindings {
        factories,
        ..Default::default()
    };
    if !config.definition.resource_dependencies.is_empty() {
        for resource in &config.definition.resources {
            let recipe = config
                .definition
                .resource_configurations
                .get(&resource.id)
                .with_context(|| format!("resource {} has no construction recipe", resource.id))?;
            bindings.resource_constructors.insert(
                resource.id.clone(),
                drasi_lib::management::resource_constructor(
                    resolver.clone(),
                    services.scope.as_ref(),
                    &config.definition.graph_id,
                    resource.clone(),
                    recipe.clone(),
                ),
            );
        }
        return Ok(ComponentBatch {
            definition: config.definition.clone(),
            bindings,
        });
    }
    for resource in &config.definition.resources {
        let recipe = config
            .definition
            .resource_configurations
            .get(&resource.id)
            .with_context(|| format!("resource {} has no construction recipe", resource.id))?;
        let handle = resolver
            .resolve(
                &services.scope,
                &config.definition.graph_id,
                resource,
                recipe,
            )
            .await?;
        bindings.resources.insert(resource.id.clone(), handle);
    }
    Ok(config.definition.build_components(bindings)?)
}

/// The same recipes serve imperative construction and durable reconciliation.
pub struct ServerManagementResources {
    factories: FactoryRegistry,
    bootstraps: NativeBootstrapFactories,
    middleware: Arc<drasi_core::middleware::MiddlewareTypeRegistry>,
    transactional: Arc<TransactionalTransformerRegistry>,
    secrets: Option<Arc<dyn SecretStoreProvider>>,
}

impl ServerManagementResources {
    pub fn new(
        registry: &crate::plugin_registry::PluginRegistry,
        secrets: Option<Arc<dyn SecretStoreProvider>>,
    ) -> Result<Self> {
        let middleware = DrasiLib::default_middleware_registry();
        Ok(Self {
            factories: registry.computation_factory_registry()?,
            bootstraps: registry.computation_bootstrap_factories()?,
            transactional: registry.transactional_transformer_registry(middleware.clone())?,
            middleware,
            secrets,
        })
    }
}

#[async_trait]
impl ManagementResourceResolver for ServerManagementResources {
    fn validate_transition(
        &self,
        previous: &DesiredTopology,
        desired: &DesiredTopology,
    ) -> Result<()> {
        for resource in &desired.resources {
            let configuration = desired
                .resource_configurations
                .get(&resource.id)
                .context("managed resource recipe is unavailable")?;
            let dependencies = desired
                .resource_dependencies
                .get(&resource.id)
                .cloned()
                .unwrap_or_default();
            match serde_json::from_value(configuration.clone())? {
                ComputationResourceConfig::NativeBootstrap(config) => {
                    config.validate(resource, &dependencies)?;
                    config.validate_owner(&resource.id, desired)?;
                    config.validate_configuration(
                        &self.bootstraps,
                        &dependencies,
                        &BTreeMap::new(),
                    )?;
                }
                ComputationResourceConfig::NativeConsumer(config) => {
                    config.validate_dependencies(resource, &dependencies)?
                }
                _ => {}
            }
        }
        let mut protected = Vec::new();
        for resource in &previous.resources {
            let configuration = previous
                .resource_configurations
                .get(&resource.id)
                .context("managed resource recipe is unavailable")?;
            let recipe: ComputationResourceConfig = serde_json::from_value(configuration.clone())?;
            let persistent = match recipe {
                ComputationResourceConfig::RocksdbIndexes { .. }
                | ComputationResourceConfig::SharedStorage { .. }
                | ComputationResourceConfig::SharedQos(_)
                | ComputationResourceConfig::NativeConsumer(_) => true,
                ComputationResourceConfig::Qos { path, recovery, .. } => {
                    path.is_some() || recovery.is_some()
                }
                _ => false,
            };
            if persistent {
                protected.push(resource.id.clone());
            }
        }
        drasi_lib::management::validate_recovery_resource_changes(previous, desired, protected)
    }

    async fn resolve(
        &self,
        instance: &str,
        graph: &str,
        resource: &ResourceSpecification,
        configuration: &serde_json::Value,
    ) -> Result<ResourceHandle> {
        self.resolve_with_dependencies(instance, graph, resource, configuration, &BTreeMap::new())
            .await
    }

    async fn resolve_with_dependencies(
        &self,
        instance: &str,
        graph: &str,
        resource: &ResourceSpecification,
        configuration: &serde_json::Value,
        dependencies: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> Result<ResourceHandle> {
        let recipe: ComputationResourceConfig = serde_json::from_value(configuration.clone())
            .with_context(|| format!("invalid configuration for resource {}", resource.id))?;
        anyhow::ensure!(
            recipe.role() == resource.role,
            "resource {} role differs from its recipe",
            resource.id
        );
        if let ComputationResourceConfig::SharedQos(config) = recipe {
            return config
                .resolve(resource, dependencies, &self.factories)
                .await;
        }
        if let ComputationResourceConfig::NativeConsumer(config) = recipe {
            return config.resolve(instance, graph, resource, dependencies);
        }
        if let ComputationResourceConfig::NativeBootstrap(config) = recipe {
            return config
                .resolve(instance, graph, resource, dependencies, &self.bootstraps)
                .await;
        }
        anyhow::ensure!(
            dependencies.is_empty(),
            "this server recipe does not accept resource dependencies"
        );
        let handle = match recipe {
            ComputationResourceConfig::MemoryIndexes => ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(Arc::new(
                    drasi_core::computation::InMemoryComputationProvider,
                ))),
            ),
            ComputationResourceConfig::QueryCatalog {} => ResourceHandle::new(
                ResourceRole::QueryCatalog,
                Arc::new(QueryResultsCatalog::new(graph)?),
            ),
            ComputationResourceConfig::SourceProgress { component } => ResourceHandle::new(
                ResourceRole::Checkpoint,
                Arc::new(QuerySourceProgressResource(Arc::new(
                    QuerySourceProgress::new(graph, component)?,
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
                    instance,
                )?
                .resource()
            }
            ComputationResourceConfig::SharedStorage { path, component } => {
                anyhow::ensure!(
                    !path.as_os_str().is_empty(),
                    "shared storage path must not be empty"
                );
                anyhow::ensure!(
                    resource.ownership == ResourceOwnership::Graph,
                    "shared storage requires graph ownership"
                );
                let provider = LegacyIndexProviderAdapter::scoped(
                    Arc::new(drasi_index_rocksdb::RocksDbIndexProvider::new(
                        path, false, false,
                    )),
                    instance,
                )?;
                let indexes = provider.create_indexes(graph, resource.id.as_str()).await?;
                SharedStorageGroup::new(graph, component, indexes)?.resource()
            }
            ComputationResourceConfig::SharedQos(_) => {
                unreachable!("shared QoS resolved with dependencies")
            }
            ComputationResourceConfig::NativeConsumer(_) => {
                unreachable!("native consumer resolved with dependencies")
            }
            ComputationResourceConfig::NativeBootstrap(_) => {
                unreachable!("native bootstrap resolved with dependencies")
            }
            ComputationResourceConfig::Middleware => ResourceHandle::new(
                ResourceRole::Middleware,
                Arc::new(MiddlewareRegistryResource(self.middleware.clone())),
            ),
            ComputationResourceConfig::QueryMiddleware => ResourceHandle::new(
                ResourceRole::Middleware,
                Arc::new(QueryMiddlewareResource(self.middleware.clone())),
            ),
            ComputationResourceConfig::TransactionalTransformers => ResourceHandle::new(
                ResourceRole::Component,
                Arc::new(TransactionalTransformerRegistryResource(
                    self.transactional.clone(),
                )),
            ),
            ComputationResourceConfig::Configuration => ResourceHandle::new(
                ResourceRole::SecretStore,
                Arc::new(ConfigurationResolverResource(Arc::new(
                    HostConfigurationResolver::new(self.secrets.clone()),
                ))),
            ),
            ComputationResourceConfig::Qos {
                definition,
                path,
                recovery,
            } => {
                definition.validate()?;
                let recovery = recovery
                    .map(|recovery| recovery.options(instance, graph))
                    .unwrap_or_default();
                recovery.validate(&definition)?;
                anyhow::ensure!(
                    definition.durable == path.is_some()
                        && path
                            .as_ref()
                            .is_none_or(|path| !path.as_os_str().is_empty()),
                    "durable QoS requires a nonempty storage path; volatile QoS must omit it"
                );
                anyhow::ensure!(
                    resource.ownership == ResourceOwnership::Graph,
                    "server QoS requires graph ownership"
                );
                let channel = if let Some(path) = path {
                    let provider = LegacyIndexProviderAdapter::scoped(
                        Arc::new(drasi_index_rocksdb::RocksDbIndexProvider::new(
                            path, false, false,
                        )),
                        instance,
                    )?;
                    let indexes = provider.create_indexes(graph, resource.id.as_str()).await?;
                    QosChannel::persistent_with_recovery(
                        definition,
                        indexes,
                        self.factories.envelope_codec(
                            std::num::NonZeroUsize::new(64 * 1024 * 1024).expect("constant size"),
                        )?,
                        resource.id.as_str(),
                        recovery,
                    )
                    .await?
                } else {
                    QosChannel::volatile(definition)?
                };
                channel.resource()
            }
        };
        Ok(handle)
    }
}

pub fn configuration_from_snapshot(
    snapshot: &InstanceConfigurationSnapshot,
) -> Result<Option<ComputationConfig>> {
    anyhow::ensure!(
        snapshot.version == 1,
        "unsupported instance configuration snapshot version"
    );
    let Some(native) = &snapshot.native_components else {
        return Ok(None);
    };
    let mut config = ComputationConfig {
        definition: native.topology.clone(),
    };
    config.definition.revision = GraphRevision(0);
    validate_definition(&config).context("native components cannot be persisted or cloned")?;
    Ok(Some(config))
}

pub async fn register_components(
    config: &ComputationConfig,
    core: &DrasiLib,
    registry: &crate::plugin_registry::PluginRegistry,
) -> Result<ReconciliationReport> {
    let components = build_components_with_registry(config, core, registry).await?;
    Ok(core.add_components(components).await?)
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
        let resolver = HostConfigurationResolver::new(None);
        for key in [
            "env:COUNT",
            "env-json:COUNT",
            "secret:password",
            "secret-json:options",
        ] {
            assert!(resolver.validate_reference(key).is_ok());
        }
        for key in ["COUNT", "unknown:COUNT", "secret:", "env:bad\nname"] {
            assert!(resolver.validate_reference(key).is_err(), "{key}");
        }
    }

    #[tokio::test]
    async fn secret_references_preserve_strings_unless_json_was_requested() {
        let secrets = drasi_lib::secret_store::MemorySecretStoreProvider::new()
            .with_secret("number", "42")
            .with_secret("invalid-json", "not json");
        let resolver = HostConfigurationResolver::new(Some(Arc::new(secrets)));
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
