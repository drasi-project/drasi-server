// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use anyhow::{Context, Result};
use drasi_lib::computation::v1::*;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

use super::{validate_definition, ComputationConfig, ComputationResourceConfig};

/// File/API input. Resolution produces the existing canonical configuration;
/// neither form is a second runtime graph.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum ComputationConfigInput {
    Definition(Box<ComputationConfig>),
    Graph { graph: ComputationGraphConfig },
}

impl<'de> Deserialize<'de> for ComputationConfigInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct InputVisitor;
        impl<'de> Visitor<'de> for InputVisitor {
            type Value = ComputationConfigInput;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("exactly one of computation.graph or computation.definition")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let key = map.next_key::<String>()?.ok_or_else(|| {
                    de::Error::custom("computation requires exactly one of 'graph' or 'definition'")
                })?;
                let input = match key.as_str() {
                    "definition" => ComputationConfigInput::from(ComputationConfig {
                        definition: map.next_value()?,
                    }),
                    "graph" => ComputationConfigInput::Graph {
                        graph: map.next_value()?,
                    },
                    _ => return Err(de::Error::unknown_field(&key, &["graph", "definition"])),
                };
                if map.next_key::<String>()?.is_some() {
                    return Err(de::Error::custom(
                        "computation requires exactly one of 'graph' or 'definition', with no additional fields",
                    ));
                }
                Ok(input)
            }
        }
        deserializer.deserialize_map(InputVisitor)
    }
}

impl From<ComputationConfig> for ComputationConfigInput {
    fn from(config: ComputationConfig) -> Self {
        Self::Definition(Box::new(config))
    }
}

impl ComputationConfigInput {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Definition(config) => validate_definition(config),
            Self::Graph { graph } => graph.validate().map(|_| ()),
        }
    }

    pub fn resolve(&self, factories: &FactoryRegistry) -> Result<Cow<'_, ComputationConfig>> {
        match self {
            Self::Definition(config) => {
                validate_definition(config)?;
                Ok(Cow::Borrowed(config))
            }
            Self::Graph { graph } => Ok(Cow::Owned(graph.resolve(factories)?)),
        }
    }
}

/// Readable declarations. Roles, completion contracts and port schemas belong
/// to the selected factory, not to a user's copy of its implementation details.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputationGraphConfig {
    pub components: Vec<GraphComponentConfig>,
    // Decode recipes at resolution, not file parsing: accepted managed state
    // must still restore when the YAML seed contains an obsolete recipe.
    #[serde(default, deserialize_with = "deserialize_resources")]
    #[schema(value_type = BTreeMap<String, ComputationResourceConfig>)]
    pub resources: BTreeMap<ResourceId, serde_json::Value>,
    #[serde(default)]
    pub connections: Vec<GraphConnectionConfig>,
    #[serde(default = "default_capacity")]
    #[schema(minimum = 1)]
    pub default_pipe_capacity: usize,
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, Vec<String>>)]
    pub resource_dependencies: BTreeMap<ResourceId, Vec<ResourceId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphComponentConfig {
    #[schema(value_type = String)]
    pub id: ComponentId,
    pub kind: String,
    pub version: Option<String>,
    #[serde(default = "empty_configuration")]
    pub config: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, GraphResourceBinding>)]
    pub resources: BTreeMap<Arc<str>, GraphResourceBinding>,
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, String>)]
    pub streams: BTreeMap<PortId, StreamId>,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum GraphResourceBinding {
    One(#[schema(value_type = String)] ResourceId),
    Many(#[schema(value_type = Vec<String>)] Vec<ResourceId>),
}

impl GraphResourceBinding {
    fn ids(&self) -> &[ResourceId] {
        match self {
            Self::One(id) => std::slice::from_ref(id),
            Self::Many(ids) => ids,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphConnectionConfig {
    pub from: GraphEndpoint,
    pub to: GraphEndpoint,
    #[schema(minimum = 1)]
    pub capacity: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum GraphEndpoint {
    Component(#[schema(value_type = String)] ComponentId),
    Port {
        #[schema(value_type = String)]
        component: ComponentId,
        #[schema(value_type = String)]
        port: PortId,
    },
}

impl GraphEndpoint {
    fn component(&self) -> &ComponentId {
        match self {
            Self::Component(id) | Self::Port { component: id, .. } => id,
        }
    }

    fn resolve(
        &self,
        components: &BTreeMap<ComponentId, ComponentDescriptor>,
        direction: PortDirection,
    ) -> Result<Endpoint> {
        let id = self.component();
        let descriptor = components
            .get(id)
            .with_context(|| format!("unknown component '{id}'"))?;
        let mut ports = descriptor
            .ports()
            .iter()
            .filter(|port| port.direction() == direction);
        let port = match self {
            Self::Port { port, .. } => ports
                .find(|candidate| candidate.id() == port)
                .with_context(|| format!("component '{id}' has no {direction:?} port '{port}'"))?,
            Self::Component(_) => {
                let port = ports
                    .next()
                    .with_context(|| format!("component '{id}' has no {direction:?} port"))?;
                anyhow::ensure!(
                    ports.next().is_none(),
                    "component '{id}' has multiple {direction:?} ports; specify {{component, port}}"
                );
                port
            }
        };
        Ok(Endpoint::new(id.clone(), port.id().clone()))
    }
}

impl ComputationGraphConfig {
    fn validate(&self) -> Result<BTreeMap<ResourceId, ComputationResourceConfig>> {
        anyhow::ensure!(
            self.default_pipe_capacity > 0,
            "defaultPipeCapacity must be positive"
        );
        let mut components = BTreeSet::new();
        for component in &self.components {
            anyhow::ensure!(
                components.insert(component.id.clone()),
                "duplicate computation component '{}'",
                component.id
            );
            ImplementationIdentity::try_new(
                component.kind.as_str(),
                component.version.as_deref().unwrap_or("1"),
            )?;
            anyhow::ensure!(
                component.config.is_object(),
                "component '{}' config must be an object",
                component.id
            );
            for id in component
                .resources
                .values()
                .flat_map(GraphResourceBinding::ids)
            {
                anyhow::ensure!(
                    self.resources.contains_key(id),
                    "component '{}' references unknown resource '{id}'",
                    component.id
                );
            }
        }
        for connection in &self.connections {
            anyhow::ensure!(
                connection.capacity.is_none_or(|capacity| capacity > 0),
                "connection capacity must be positive"
            );
            for endpoint in [&connection.from, &connection.to] {
                anyhow::ensure!(
                    components.contains(endpoint.component()),
                    "connection references unknown component '{}'",
                    endpoint.component()
                );
            }
        }
        for (id, dependencies) in &self.resource_dependencies {
            anyhow::ensure!(
                self.resources.contains_key(id),
                "unknown dependent resource '{id}'"
            );
            let mut seen = BTreeSet::new();
            for dependency in dependencies {
                anyhow::ensure!(
                    self.resources.contains_key(dependency),
                    "resource '{id}' references unknown resource '{dependency}'"
                );
                anyhow::ensure!(
                    seen.insert(dependency),
                    "resource '{id}' repeats dependency '{dependency}'"
                );
            }
        }
        self.resources
            .iter()
            .map(|(id, recipe)| {
                serde_json::from_value(recipe.clone())
                    .map(|recipe| (id.clone(), recipe))
                    .with_context(|| format!("invalid configuration for resource '{id}'"))
            })
            .collect()
    }

    fn resolve(&self, factories: &FactoryRegistry) -> Result<ComputationConfig> {
        let resources = self.validate()?;
        let mut definition = drasi_lib::management::DesiredInstance::default().topology;
        for (id, recipe) in &resources {
            let ownership = match recipe {
                ComputationResourceConfig::Middleware {}
                | ComputationResourceConfig::QueryMiddleware {}
                | ComputationResourceConfig::TransactionalTransformers {}
                | ComputationResourceConfig::Configuration {} => ResourceOwnership::Borrowed,
                _ => ResourceOwnership::Graph,
            };
            definition.resources.push(ResourceSpecification {
                id: id.clone(),
                role: recipe.role(),
                ownership,
                binding: Arc::from(id.as_str()),
            });
            definition
                .resource_configurations
                .insert(id.clone(), serde_json::to_value(recipe)?);
        }
        for (id, dependencies) in &self.resource_dependencies {
            definition.resource_dependencies.insert(
                id.clone(),
                dependencies
                    .iter()
                    .map(|dependency| (dependency.clone(), resources[dependency].role()))
                    .collect(),
            );
        }
        let declarations = resources
            .iter()
            .map(|(id, recipe)| (id.clone(), recipe.role()))
            .collect();
        let mut descriptors = BTreeMap::new();
        for component in &self.components {
            let factory = factories
                .get_by_name(&component.kind, component.version.as_deref())
                .with_context(|| format!("resolve component '{}'", component.id))?;
            let mut specification = factory
                .specification_from_config(component.id.clone(), component.config.clone())
                .with_context(|| format!("describe component '{}'", component.id))?;
            anyhow::ensure!(
                specification.descriptor.id() == &component.id
                    && specification.implementation == factory.descriptor().implementation
                    && specification.role == factory.descriptor().role
                    && specification.configuration_version
                        == factory.descriptor().configuration_version,
                "factory description for component '{}' disagrees with its registered identity",
                component.id
            );
            specification.dependencies = component
                .resources
                .iter()
                .map(|(name, binding)| (name.clone(), binding.ids().to_vec()))
                .collect();
            factory
                .descriptor()
                .configuration
                .validate_values(
                    &specification.configuration,
                    &declarations,
                    &BTreeMap::new(),
                )
                .with_context(|| {
                    format!("invalid configuration for component '{}'", component.id)
                })?;
            factory
                .validate(&specification)
                .with_context(|| format!("invalid component '{}'", component.id))?;
            let mut streams = BTreeMap::new();
            for port in specification
                .descriptor
                .ports()
                .iter()
                .filter(|port| port.direction() == PortDirection::Output)
            {
                streams.insert(
                    port.id().clone(),
                    match component.streams.get(port.id()) {
                        Some(stream) => stream.clone(),
                        None => StreamId::try_new(format!("{}/{}", component.id, port.id()))?,
                    },
                );
            }
            anyhow::ensure!(
                component
                    .streams
                    .keys()
                    .all(|port| streams.contains_key(port)),
                "component '{}' stream binding must name an output port",
                component.id
            );
            descriptors.insert(component.id.clone(), specification.descriptor.clone());
            definition.components.push(DesiredComponent {
                descriptor: specification.descriptor.clone(),
                role: specification.role,
                completion: specification.completion,
                streams,
                lifecycle: LifecyclePolicy {
                    auto_start: component.auto_start,
                },
                input_merge: InputMergePolicy::default(),
                construction: ComponentConstruction::Factory(specification),
            });
        }
        for connection in &self.connections {
            definition.relationships.push(DesiredRelationship {
                definition: EdgeDefinition::new(
                    connection
                        .from
                        .resolve(&descriptors, PortDirection::Output)?,
                    connection.to.resolve(&descriptors, PortDirection::Input)?,
                ),
                policy: RelationshipPolicy::default(),
                pipe: DesiredPipe::Bounded {
                    capacity: connection.capacity.unwrap_or(self.default_pipe_capacity),
                },
            });
        }
        let config = ComputationConfig { definition };
        validate_definition(&config)?;
        Ok(config)
    }
}

fn default_capacity() -> usize {
    32
}

fn default_auto_start() -> bool {
    true
}

fn empty_configuration() -> serde_json::Value {
    serde_json::json!({})
}

fn deserialize_resources<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<ResourceId, serde_json::Value>, D::Error> {
    struct ResourcesVisitor;
    impl<'de> Visitor<'de> for ResourcesVisitor {
        type Value = BTreeMap<ResourceId, serde_json::Value>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("unique resource IDs mapped to resource configurations")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut resources = BTreeMap::new();
            while let Some((id, recipe)) = map.next_entry::<ResourceId, serde_json::Value>()? {
                if resources.insert(id.clone(), recipe).is_some() {
                    return Err(de::Error::custom(format!(
                        "duplicate computation resource '{id}'"
                    )));
                }
            }
            Ok(resources)
        }
    }
    deserializer.deserialize_map(ResourcesVisitor)
}
