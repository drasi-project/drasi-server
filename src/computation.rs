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
use drasi_core::computation::ComputationIndexProvider;
use drasi_host_sdk::management::HostConfigurationResolver;
use drasi_lib::{computation::v1::*, DrasiLib};
use serde::{Deserialize, Serialize};

pub mod named;

/// Native components, connections and resource recipes for a DrasiLib instance.
#[derive(Debug, Clone)]
pub struct ComputationConfig {
    pub definition: DesiredTopology,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(untagged)]
#[schema(as = ComputationConfig)]
#[allow(dead_code)]
enum ComputationConfigSchema {
    Legacy { definition: serde_json::Value },
    Named(named::NamedGraphConfig),
}

impl<'s> utoipa::ToSchema<'s> for ComputationConfig {
    fn schema() -> (
        &'s str,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    ) {
        ComputationConfigSchema::schema()
    }
}

impl Serialize for ComputationConfig {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        if let Some(named) = named::export(&self.definition).map_err(serde::ser::Error::custom)? {
            named.serialize(serializer)
        } else {
            use serde::ser::SerializeStruct;
            let mut value = serializer.serialize_struct("ComputationConfig", 1)?;
            value.serialize_field("definition", &self.definition)?;
            value.end()
        }
    }
}

impl<'de> Deserialize<'de> for ComputationConfig {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            definition: Option<DesiredTopology>,
            #[serde(default, deserialize_with = "named::optional_unique_map")]
            pipes: Option<BTreeMap<String, named::NamedPipeConfig>>,
            #[serde(default, deserialize_with = "named::optional_unique_map")]
            components: Option<BTreeMap<ComponentId, named::NamedComponentConfig>>,
            resources: Option<Vec<ResourceSpecification>>,
            resource_configurations: Option<BTreeMap<ResourceId, serde_json::Value>>,
            relationships: Option<Vec<DesiredRelationship>>,
            requirements: Option<PipeRequirements>,
            control_connections: Option<Vec<(ComponentId, ComponentId)>>,
            subscriptions: Option<Vec<(ComponentId, ComponentId)>>,
            readiness_required: Option<std::collections::BTreeSet<ComponentId>>,
            component_resources:
                Option<BTreeMap<ComponentId, std::collections::BTreeSet<ResourceId>>>,
            component_plugins: Option<BTreeMap<ComponentId, PluginIdentity>>,
            allow_incomplete: Option<bool>,
        }
        let wire = Wire::deserialize(deserializer)?;
        if let Some(definition) = wire.definition {
            if wire.pipes.is_some()
                || wire.components.is_some()
                || wire.resources.is_some()
                || wire.resource_configurations.is_some()
                || wire.relationships.is_some()
                || wire.requirements.is_some()
                || wire.control_connections.is_some()
                || wire.subscriptions.is_some()
                || wire.readiness_required.is_some()
                || wire.component_resources.is_some()
                || wire.component_plugins.is_some()
                || wire.allow_incomplete.is_some()
            {
                return Err(serde::de::Error::custom(
                    "use either computation.definition or named pipes/components, not both",
                ));
            }
            return Ok(Self { definition });
        }
        let pipes = wire
            .pipes
            .ok_or_else(|| serde::de::Error::missing_field("pipes"))?;
        let components = wire
            .components
            .ok_or_else(|| serde::de::Error::missing_field("components"))?;
        named::NamedGraphConfig {
            pipes,
            components,
            resources: wire.resources.unwrap_or_default(),
            resource_configurations: wire.resource_configurations.unwrap_or_default(),
            relationships: wire.relationships.unwrap_or_default(),
            requirements: wire.requirements.unwrap_or_default(),
            control_connections: wire.control_connections.unwrap_or_default(),
            subscriptions: wire.subscriptions.unwrap_or_default(),
            readiness_required: wire.readiness_required.unwrap_or_default(),
            component_resources: wire.component_resources.unwrap_or_default(),
            component_plugins: wire.component_plugins.unwrap_or_default(),
            allow_incomplete: wire.allow_incomplete.unwrap_or_default(),
        }
        .compile()
        .map_err(serde::de::Error::custom)
    }
}

/// Provider recipes stored in DesiredTopology::resource_configurations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ComputationResourceConfig {
    MemoryIndexes,
    QueryCatalog {},
    RocksdbIndexes {
        path: PathBuf,
    },
    Middleware,
    QueryMiddleware,
    TransactionalTransformers,
    Configuration,
    Qos {
        #[schema(value_type = serde_json::Value)]
        definition: QosChannelDefinition,
        path: Option<PathBuf>,
    },
    NamedPipe {
        name: String,
        #[schema(value_type = serde_json::Value)]
        transport: named::NamedTransport,
    },
}

impl ComputationResourceConfig {
    fn role(&self) -> ResourceRole {
        match self {
            Self::MemoryIndexes | Self::RocksdbIndexes { .. } => ResourceRole::IndexBackend,
            Self::QueryCatalog {} => ResourceRole::QueryCatalog,
            Self::Middleware | Self::QueryMiddleware => ResourceRole::Middleware,
            Self::TransactionalTransformers => ResourceRole::Component,
            Self::Configuration => ResourceRole::SecretStore,
            Self::Qos { .. } => ResourceRole::StateStore,
            Self::NamedPipe { transport, .. } => transport.role(),
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
        definition.boundary_relationships.is_empty(),
        "server computation cannot reconstruct boundary bindings"
    );
    named::validate(definition)?;
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
        if let ComputationResourceConfig::Qos { definition, path } = &recipe {
            definition.validate()?;
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
            DesiredPipe::External { resources, .. } => resources.clone(),
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
    validate_definition(config)?;
    let current = core.computation_control()?.desired_snapshot();
    for resource in &config.definition.resources {
        let is_named = |recipe: Option<&serde_json::Value>| {
            recipe
                .and_then(|value| value.get("kind"))
                .and_then(serde_json::Value::as_str)
                == Some("namedPipe")
        };
        if is_named(config.definition.resource_configurations.get(&resource.id))
            || is_named(current.resource_configurations.get(&resource.id))
        {
            anyhow::ensure!(
                !current.resources.contains_key(&resource.id),
                "named pipe resource '{}' already exists in this instance",
                resource.id
            );
        }
    }
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
    let instance_id = core.get_current_config().await?.id;
    let mut bindings = TopologyBindings {
        factories,
        ..Default::default()
    };
    bind_named_pipes(config, &mut bindings)?;
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
        let recipe = match recipe {
            ComputationResourceConfig::NamedPipe {
                name,
                transport: named::NamedTransport::Qos { definition, path },
            } => ComputationResourceConfig::Qos {
                definition,
                path: path.map(|path| named::storage_path(path, &instance_id, &name)),
            },
            recipe => recipe,
        };
        let handle = match recipe {
            ComputationResourceConfig::MemoryIndexes => ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(Arc::new(
                    drasi_core::computation::InMemoryComputationProvider,
                ))),
            ),
            ComputationResourceConfig::QueryCatalog {} => ResourceHandle::new(
                ResourceRole::QueryCatalog,
                Arc::new(QueryResultsCatalog::new(
                    config.definition.graph_id.as_str(),
                )?),
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
            ComputationResourceConfig::Qos { definition, path } => {
                let channel = if let Some(path) = path {
                    let provider = LegacyIndexProviderAdapter::scoped(
                        Arc::new(drasi_index_rocksdb::RocksDbIndexProvider::new(
                            path, false, false,
                        )),
                        services.scope.clone(),
                    )?;
                    let indexes = provider
                        .create_indexes(&config.definition.graph_id, resource.id.as_str())
                        .await?;
                    QosChannel::persistent(
                        definition,
                        indexes,
                        bindings.factories.envelope_codec(
                            std::num::NonZeroUsize::new(64 * 1024 * 1024).expect("constant size"),
                        )?,
                        resource.id.as_str(),
                    )
                    .await?
                } else {
                    QosChannel::volatile(definition)?
                };
                channel.resource()
            }
            ComputationResourceConfig::NamedPipe { transport, .. } => {
                ResourceHandle::new(ResourceRole::Pipe, Arc::new(transport))
            }
        };
        bindings.resources.insert(resource.id.clone(), handle);
    }
    Ok(config.definition.build_components(bindings)?)
}

pub fn bind_named_pipes(config: &ComputationConfig, bindings: &mut TopologyBindings) -> Result<()> {
    named::bind(&config.definition, bindings)
}

pub fn validate_named_mutation(definition: &DesiredTopology) -> Result<()> {
    named::validate_in_instance(definition)
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
    let components = build_components(
        config,
        core,
        registry.computation_factory_registry()?,
        registry.transactional_transformer_registry(core.middleware_registry())?,
    )
    .await?;
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
