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
use drasi_lib::config::ComponentReference;
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

#[derive(Deserialize)]
#[serde(transparent)]
struct CanonicalDefinition(
    #[serde(with = "serde_yaml::with::singleton_map_recursive")] DesiredTopology,
);

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
                        definition: map.next_value::<CanonicalDefinition>()?.0,
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
        self.resolve_with_ids(factories, |component| {
            component.id.clone().with_context(|| {
                format!(
                    "Component '{}' requires DrasiLib-owned UUID allocation",
                    component.name
                )
            })
        })
    }

    pub fn resolve_with_ids(
        &self,
        factories: &FactoryRegistry,
        allocate: impl FnMut(&GraphComponentConfig) -> Result<ComponentId>,
    ) -> Result<Cow<'_, ComputationConfig>> {
        match self {
            Self::Definition(config) => {
                validate_definition(config)?;
                Ok(Cow::Borrowed(config))
            }
            Self::Graph { graph } => Ok(Cow::Owned(graph.resolve(factories, allocate)?)),
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
    pub name: ComponentName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub id: Option<ComponentId>,
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
    Component(#[schema(value_type = crate::api::models::ComponentReferenceDto)] ComponentReference),
    Port {
        #[schema(value_type = crate::api::models::ComponentReferenceDto)]
        component: ComponentReference,
        #[schema(value_type = String)]
        port: PortId,
    },
}

impl GraphEndpoint {
    fn component(&self) -> &ComponentReference {
        match self {
            Self::Component(id) | Self::Port { component: id, .. } => id,
        }
    }

    fn resolve(
        &self,
        identities: &BTreeMap<ComponentName, ComponentId>,
        components: &BTreeMap<ComponentId, ComponentDescriptor>,
        primaries: &BTreeMap<ComponentId, PortPrimaries>,
        direction: PortDirection,
    ) -> Result<Endpoint> {
        let id = resolve_reference(self.component(), identities)?;
        let descriptor = components
            .get(&id)
            .with_context(|| format!("unknown component '{id}'"))?;
        let explicit = match self {
            Self::Port { port, .. } => Some(port),
            Self::Component(_) => None,
        };
        let primary = primaries.get(&id).cloned().unwrap_or_default();
        let port = primary
            .resolve(descriptor.ports(), direction, explicit)
            .with_context(|| format!("resolve {direction:?} port on component '{id}'"))?;
        Ok(Endpoint::new(id, port))
    }
}

impl ComputationGraphConfig {
    fn validate(&self) -> Result<BTreeMap<ResourceId, ComputationResourceConfig>> {
        let identities = self
            .components
            .iter()
            .map(|component| {
                (
                    component.name.clone(),
                    component.id.clone().unwrap_or_else(ComponentId::generate),
                )
            })
            .collect();
        self.validate_with_ids(&identities)
    }

    fn validate_with_ids(
        &self,
        identities: &BTreeMap<ComponentName, ComponentId>,
    ) -> Result<BTreeMap<ResourceId, ComputationResourceConfig>> {
        anyhow::ensure!(
            self.default_pipe_capacity > 0,
            "defaultPipeCapacity must be positive"
        );
        let mut components = BTreeSet::new();
        let mut names = BTreeSet::new();
        for component in &self.components {
            anyhow::ensure!(
                names.insert(component.name.clone()),
                "duplicate computation component name '{}'",
                component.name
            );
            let id = identities
                .get(&component.name)
                .with_context(|| format!("Component '{}' has no assigned UUID", component.name))?;
            anyhow::ensure!(
                components.insert(id.clone()),
                "duplicate computation component UUID '{id}'"
            );
            ImplementationIdentity::try_new(
                component.kind.as_str(),
                component.version.as_deref().unwrap_or("1"),
            )?;
            anyhow::ensure!(
                component.config.is_object(),
                "component '{}' config must be an object",
                component.name
            );
            for id in component
                .resources
                .values()
                .flat_map(GraphResourceBinding::ids)
            {
                anyhow::ensure!(
                    self.resources.contains_key(id),
                    "component '{}' references unknown resource '{id}'",
                    component.name
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
                    components.contains(&resolve_reference(endpoint.component(), identities)?),
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
                serde_json::from_value(resolve_resource_references(recipe, identities)?)
                    .map(|recipe| (id.clone(), recipe))
                    .with_context(|| format!("invalid configuration for resource '{id}'"))
            })
            .collect()
    }

    fn resolve(
        &self,
        factories: &FactoryRegistry,
        mut allocate: impl FnMut(&GraphComponentConfig) -> Result<ComponentId>,
    ) -> Result<ComputationConfig> {
        let mut identities = BTreeMap::new();
        for component in &self.components {
            let id = allocate(component)?;
            anyhow::ensure!(
                component
                    .id
                    .as_ref()
                    .is_none_or(|requested| requested == &id),
                "Component '{}' UUID assignment changed its requested identity",
                component.name
            );
            anyhow::ensure!(
                identities.insert(component.name.clone(), id).is_none(),
                "duplicate computation component name '{}'",
                component.name
            );
        }
        let resources = self.validate_with_ids(&identities)?;
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
        let mut primaries = BTreeMap::new();
        for component in &self.components {
            let id = &identities[&component.name];
            let factory = factories
                .get_by_name(&component.kind, component.version.as_deref())
                .with_context(|| format!("resolve component '{}'", component.name))?;
            let mut specification = factory
                .specification_from_config(
                    id.clone(),
                    resolve_configuration_references(&component.config, &identities)?,
                )
                .with_context(|| format!("describe component '{}'", component.name))?;
            anyhow::ensure!(
                specification.descriptor.id() == id
                    && specification.implementation == factory.descriptor().implementation
                    && specification.role == factory.descriptor().role
                    && specification.configuration_version
                        == factory.descriptor().configuration_version,
                "factory description for component '{}' disagrees with its registered identity",
                component.name
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
                    format!("invalid configuration for component '{}'", component.name)
                })?;
            factory
                .validate(&specification)
                .with_context(|| format!("invalid component '{}'", component.name))?;
            let primary = factory.primary_ports();
            primary.validate(specification.descriptor.ports())?;
            primaries.insert(id.clone(), primary);
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
                        None => StreamId::try_new(format!("{id}/{}", port.id()))?,
                    },
                );
            }
            anyhow::ensure!(
                component
                    .streams
                    .keys()
                    .all(|port| streams.contains_key(port)),
                "component '{}' stream binding must name an output port",
                component.name
            );
            descriptors.insert(id.clone(), specification.descriptor.clone());
            definition.components.push(DesiredComponent {
                name: component.name.clone(),
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
                    connection.from.resolve(
                        &identities,
                        &descriptors,
                        &primaries,
                        PortDirection::Output,
                    )?,
                    connection.to.resolve(
                        &identities,
                        &descriptors,
                        &primaries,
                        PortDirection::Input,
                    )?,
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

fn resolve_reference(
    reference: &ComponentReference,
    identities: &BTreeMap<ComponentName, ComponentId>,
) -> Result<ComponentId> {
    match reference {
        ComponentReference::Name(name) => identities
            .get(name)
            .cloned()
            .with_context(|| format!("unknown component name '{name}'")),
        ComponentReference::Id { id } => Ok(id.clone()),
    }
}

fn resolve_resource_references(
    value: &serde_json::Value,
    identities: &BTreeMap<ComponentName, ComponentId>,
) -> Result<serde_json::Value> {
    let mut value = value.clone();
    let fields = value
        .as_object_mut()
        .context("resource recipe must be an object")?;
    let kind = fields
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .context("resource recipe requires kind")?
        .to_owned();
    let bind = |value: &mut serde_json::Value| -> Result<()> {
        if let Some(fields) = value.as_object().filter(|fields| fields.contains_key("$component")) {
            anyhow::ensure!(fields.len() == 1, "$component references cannot contain extra fields");
        }
        let reference = value
            .as_object()
            .and_then(|object| object.get("$component"))
            .unwrap_or(value);
        let reference: ComponentReference = serde_json::from_value(reference.clone())?;
        *value = serde_json::json!(resolve_reference(&reference, identities)?);
        Ok(())
    };
    match kind.as_str() {
        "sourceProgress"
        | "sharedStorage"
        | "sourceTimeMergeStorage"
        | "nativeBootstrap"
        | "nativeSourceBootstrap" => {
            bind(
                fields
                    .get_mut("component")
                    .context("resource requires a component reference")?,
            )?;
        }
        "consumerProgress" => {
            bind(
                fields
                    .get_mut("producer")
                    .context("consumer progress requires a producer")?,
            )?;
            let consumers = fields
                .get_mut("consumers")
                .and_then(serde_json::Value::as_object_mut)
                .context("consumer progress requires consumer-name to resource bindings")?;
            let bindings = std::mem::take(consumers);
            for (name, resource) in bindings {
                let reference = ComponentReference::by_name(name)?;
                let id = resolve_reference(&reference, identities)?;
                anyhow::ensure!(
                    consumers.insert(id.to_string(), resource).is_none(),
                    "duplicate consumer UUID"
                );
            }
        }
        "nativeOutputSnapshots" => {
            for output in fields
                .get_mut("outputs")
                .and_then(serde_json::Value::as_array_mut)
                .context("output snapshots require outputs")?
            {
                bind(
                    output
                        .get_mut("component")
                        .context("output requires a component reference")?,
                )?;
            }
        }
        _ => {}
    }
    resolve_configuration_references(&value, identities)
}

/// Authoring-only UUID substitution for otherwise opaque plugin/resource values.
/// Accepted definitions contain UUID literals, never a reusable name binding.
fn resolve_configuration_references(
    value: &serde_json::Value,
    identities: &BTreeMap<ComponentName, ComponentId>,
) -> Result<serde_json::Value> {
    match value {
        serde_json::Value::Object(fields) => {
            if let Some(reference) = fields.get("$component") {
                anyhow::ensure!(
                    fields.len() == 1,
                    "$component references cannot contain extra fields"
                );
                let reference: ComponentReference = serde_json::from_value(reference.clone())?;
                return Ok(serde_json::json!(resolve_reference(
                    &reference, identities
                )?));
            }
            fields
                .iter()
                .map(|(key, value)| {
                    Ok((
                        key.clone(),
                        resolve_configuration_references(value, identities)?,
                    ))
                })
                .collect::<Result<serde_json::Map<_, _>>>()
                .map(serde_json::Value::Object)
        }
        serde_json::Value::Array(values) => values
            .iter()
            .map(|value| resolve_configuration_references(value, identities))
            .collect::<Result<Vec<_>>>()
            .map(serde_json::Value::Array),
        value => Ok(value.clone()),
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
