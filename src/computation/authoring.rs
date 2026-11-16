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
    #[schema(value_type = String)]
    pub name: ComponentName,
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
    /// Whether the component bootstraps. When enabled (the default), each
    /// data connection whose producer has a bootstrap output generates a
    /// bootstrap connection to this component's bootstrap input, unless the
    /// component declares a local `bootstrapProvider` or explicit bootstrap
    /// connections.
    #[serde(default = "default_enable_bootstrap")]
    pub enable_bootstrap: bool,
    /// The component's bootstrap provider: the ID of a provider component in
    /// this graph, or an inline provider declaration that generates one. On a
    /// source it serves the source's consumers; on a transformer or query it
    /// is an exclusive local provider and the component never asks its inputs
    /// for bootstrap data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap_provider: Option<GraphBootstrapProvider>,
}

/// A component's bootstrap provider: a reference to a provider component in
/// the graph, or an inline provider that generates a visible provider
/// component and its bootstrap connection.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum GraphBootstrapProvider {
    Component(#[schema(value_type = String)] ComponentId),
    Inline(Box<GraphBootstrapProviderConfig>),
}

/// An inline bootstrap provider. `kind` names a bootstrap provider factory,
/// such as `drasi.postgres/snapshot`. Without `id` or `name` the generated
/// component derives them from its owner.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphBootstrapProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub id: Option<ComponentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub name: Option<ComponentName>,
    pub kind: String,
    pub version: Option<String>,
    #[serde(default = "empty_configuration")]
    pub config: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, GraphResourceBinding>)]
    pub resources: BTreeMap<Arc<str>, GraphResourceBinding>,
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
    /// Data or bootstrap. Inferred from explicit ports, otherwise bootstrap
    /// when the producer has no data output (a bootstrap provider).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<GraphConnectionKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum GraphConnectionKind {
    Data,
    Bootstrap,
}

impl GraphConnectionKind {
    fn port_kind(self) -> PortKind {
        match self {
            Self::Data => PortKind::Data,
            Self::Bootstrap => PortKind::Bootstrap,
        }
    }

    fn of(kind: PortKind) -> Self {
        match kind {
            PortKind::Data => Self::Data,
            PortKind::Bootstrap => Self::Bootstrap,
        }
    }
}

impl fmt::Display for GraphConnectionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Data => "data",
            Self::Bootstrap => "bootstrap",
        })
    }
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

    fn explicit_port(&self) -> Option<&PortId> {
        match self {
            Self::Port { port, .. } => Some(port),
            Self::Component(_) => None,
        }
    }

    fn resolve(
        &self,
        components: &BTreeMap<ComponentId, ComponentDescriptor>,
        primaries: &BTreeMap<ComponentId, PortPrimaries>,
        direction: PortDirection,
        kind: GraphConnectionKind,
    ) -> Result<Endpoint> {
        let id = self.component();
        let descriptor = components
            .get(id)
            .with_context(|| format!("unknown component '{id}'"))?;
        let port = match (self.explicit_port(), kind) {
            (Some(port), _) => {
                let found = find_port(descriptor, direction, port)?;
                anyhow::ensure!(
                    found.kind() == kind.port_kind(),
                    "{kind} connection uses {} port '{port}' on component '{id}'",
                    GraphConnectionKind::of(found.kind())
                );
                port.clone()
            }
            (None, GraphConnectionKind::Data) if !has_data_port(descriptor, direction) => {
                anyhow::bail!(
                    "component '{id}' has no data {} port{}",
                    direction_name(direction),
                    if bootstrap_port(descriptor, direction)?.is_some() {
                        "; it takes part only in bootstrap, so use a bootstrap connection"
                    } else {
                        ""
                    }
                )
            }
            (None, GraphConnectionKind::Data) => primaries
                .get(id)
                .cloned()
                .unwrap_or_default()
                .resolve(descriptor.ports(), direction, None)
                .with_context(|| format!("resolve {direction:?} port on component '{id}'"))?,
            (None, GraphConnectionKind::Bootstrap) => bootstrap_port(descriptor, direction)?
                .with_context(|| {
                    format!(
                        "component '{id}' has no bootstrap {} port; its kind does not take part in bootstrap",
                        direction_name(direction)
                    )
                })?,
        };
        Ok(Endpoint::new(id.clone(), port))
    }
}

fn direction_name(direction: PortDirection) -> &'static str {
    match direction {
        PortDirection::Input => "input",
        PortDirection::Output => "output",
    }
}

fn find_port<'a>(
    descriptor: &'a ComponentDescriptor,
    direction: PortDirection,
    port: &PortId,
) -> Result<&'a PortDescriptor> {
    descriptor
        .ports()
        .iter()
        .find(|candidate| candidate.id() == port && candidate.direction() == direction)
        .with_context(|| {
            format!(
                "component '{}' has no {} port '{port}'",
                descriptor.id(),
                direction_name(direction)
            )
        })
}

/// The component's only bootstrap port of a direction, if it has one.
fn bootstrap_port(
    descriptor: &ComponentDescriptor,
    direction: PortDirection,
) -> Result<Option<PortId>> {
    let mut ports = descriptor
        .ports()
        .iter()
        .filter(|port| port.direction() == direction && port.kind() == PortKind::Bootstrap)
        .map(PortDescriptor::id);
    match (ports.next(), ports.next()) {
        (None, _) => Ok(None),
        (Some(port), None) => Ok(Some(port.clone())),
        (Some(first), Some(second)) => anyhow::bail!(
            "component '{}' has ambiguous bootstrap {} ports '{first}' and '{second}'; name the port",
            descriptor.id(),
            direction_name(direction)
        ),
    }
}

fn has_data_port(descriptor: &ComponentDescriptor, direction: PortDirection) -> bool {
    descriptor
        .ports()
        .iter()
        .any(|port| port.direction() == direction && port.kind() == PortKind::Data)
}

impl GraphConnectionConfig {
    /// The declared kind, else the kind of an explicit port, else bootstrap
    /// when the producer has only bootstrap outputs (a bootstrap provider).
    fn resolve_kind(
        &self,
        components: &BTreeMap<ComponentId, ComponentDescriptor>,
    ) -> Result<GraphConnectionKind> {
        let mut implied = None;
        for (endpoint, direction) in [
            (&self.from, PortDirection::Output),
            (&self.to, PortDirection::Input),
        ] {
            let Some(port) = endpoint.explicit_port() else {
                continue;
            };
            let descriptor = components
                .get(endpoint.component())
                .with_context(|| format!("unknown component '{}'", endpoint.component()))?;
            let kind = GraphConnectionKind::of(find_port(descriptor, direction, port)?.kind());
            match implied {
                Some(other) if other != kind => anyhow::bail!(
                    "connection from '{}' to '{}' joins a {other} port to a {kind} port",
                    self.from.component(),
                    self.to.component()
                ),
                _ => implied = Some(kind),
            }
        }
        match (self.kind, implied) {
            (Some(kind), Some(implied)) if kind != implied => anyhow::bail!(
                "connection from '{}' to '{}' is declared {kind} but names a {implied} port",
                self.from.component(),
                self.to.component()
            ),
            (Some(kind), _) | (None, Some(kind)) => Ok(kind),
            (None, None) => {
                let from = components
                    .get(self.from.component())
                    .with_context(|| format!("unknown component '{}'", self.from.component()))?;
                Ok(
                    if !has_data_port(from, PortDirection::Output)
                        && bootstrap_port(from, PortDirection::Output)?.is_some()
                    {
                        GraphConnectionKind::Bootstrap
                    } else {
                        GraphConnectionKind::Data
                    },
                )
            }
        }
    }
}

impl GraphComponentConfig {
    fn local_provider(&self) -> Result<Option<GraphComponentConfig>> {
        let Some(GraphBootstrapProvider::Inline(provider)) = &self.bootstrap_provider else {
            return Ok(None);
        };
        Ok(Some(GraphComponentConfig {
            id: match &provider.id {
                Some(id) => id.clone(),
                None => ComponentId::derive_owned(&self.id, "bootstrap-provider")?,
            },
            name: match &provider.name {
                Some(name) => name.clone(),
                None => ComponentName::try_new(format!("{}-bootstrap", self.name))?,
            },
            kind: provider.kind.clone(),
            version: provider.version.clone(),
            config: provider.config.clone(),
            resources: provider.resources.clone(),
            streams: BTreeMap::new(),
            auto_start: self.auto_start,
            enable_bootstrap: true,
            bootstrap_provider: None,
        }))
    }

    fn provider_id(&self) -> Result<Option<ComponentId>> {
        Ok(match &self.bootstrap_provider {
            None => None,
            Some(GraphBootstrapProvider::Component(id)) => Some(id.clone()),
            Some(GraphBootstrapProvider::Inline(_)) => self.local_provider()?.map(|node| node.id),
        })
    }
}

impl ComputationGraphConfig {
    /// Declared components followed by the provider components that inline
    /// `bootstrapProvider` declarations generate. Generated components are
    /// ordinary graph components.
    fn expanded_components(&self) -> Result<Vec<GraphComponentConfig>> {
        let mut components = self.components.clone();
        for component in &self.components {
            if let Some(provider) = component.local_provider().with_context(|| {
                format!(
                    "generate bootstrap provider for component '{}'",
                    component.id
                )
            })? {
                components.push(provider);
            }
        }
        Ok(components)
    }

    fn validate(&self) -> Result<BTreeMap<ResourceId, ComputationResourceConfig>> {
        anyhow::ensure!(
            self.default_pipe_capacity > 0,
            "defaultPipeCapacity must be positive"
        );
        let expanded = self.expanded_components()?;
        let mut components = BTreeSet::new();
        let mut names = BTreeSet::new();
        for component in &expanded {
            anyhow::ensure!(
                components.insert(component.id.clone()),
                "duplicate computation component '{}'",
                component.id
            );
            anyhow::ensure!(
                names.insert(component.name.clone()),
                "duplicate computation component name '{}'",
                component.name
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
        for component in &self.components {
            let Some(provider) = component.provider_id()? else {
                continue;
            };
            anyhow::ensure!(
                component.enable_bootstrap,
                "component '{}' declares a bootstrapProvider but disables bootstrap",
                component.id
            );
            anyhow::ensure!(
                provider != component.id,
                "component '{}' cannot be its own bootstrapProvider",
                component.id
            );
            anyhow::ensure!(
                components.contains(&provider),
                "component '{}' references unknown bootstrapProvider '{provider}'",
                component.id
            );
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
        let expanded = self.expanded_components()?;
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
        let mut roles = BTreeMap::new();
        for component in &expanded {
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
            let primary = factory.primary_ports();
            primary.validate(specification.descriptor.ports())?;
            primaries.insert(component.id.clone(), primary);
            roles.insert(component.id.clone(), specification.role);
            // Bootstrap outputs carry addressed sessions, not published streams.
            let mut streams = BTreeMap::new();
            for port in specification.descriptor.ports().iter().filter(|port| {
                port.direction() == PortDirection::Output && port.kind() == PortKind::Data
            }) {
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
                "component '{}' stream binding must name a data output port",
                component.id
            );
            descriptors.insert(component.id.clone(), specification.descriptor.clone());
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
        let bootstrap_relationship = |definition: EdgeDefinition, capacity| DesiredRelationship {
            policy: RelationshipPolicy::default(),
            definition,
            pipe: DesiredPipe::Bounded { capacity },
        };
        let mut local_providers = BTreeMap::new();
        for component in &self.components {
            let Some(provider) = component.provider_id()? else {
                continue;
            };
            anyhow::ensure!(
                roles[&provider] == ComponentRole::BootstrapProvider,
                "bootstrapProvider '{provider}' of component '{}' is a {:?} component, not a bootstrap provider",
                component.id,
                roles[&provider]
            );
            anyhow::ensure!(
                bootstrap_port(&descriptors[&component.id], PortDirection::Input)?.is_some(),
                "component '{}' (kind '{}') has no bootstrap input, so it cannot declare a bootstrapProvider",
                component.id,
                component.kind
            );
            local_providers.insert(component.id.clone(), provider);
        }
        let enabled: BTreeMap<_, _> = expanded
            .iter()
            .map(|component| (component.id.clone(), component.enable_bootstrap))
            .collect();
        let mut bootstrap_edges = BTreeSet::new();
        let mut explicit_bootstrap = BTreeSet::new();
        let mut data_inputs = Vec::new();
        for connection in &self.connections {
            let kind = connection.resolve_kind(&descriptors)?;
            let edge = EdgeDefinition::new(
                connection
                    .from
                    .resolve(&descriptors, &primaries, PortDirection::Output, kind)?,
                connection
                    .to
                    .resolve(&descriptors, &primaries, PortDirection::Input, kind)?,
            );
            let capacity = connection.capacity.unwrap_or(self.default_pipe_capacity);
            match kind {
                GraphConnectionKind::Data => {
                    data_inputs.push((edge.from.component.clone(), edge.to.component.clone()));
                    definition.relationships.push(DesiredRelationship {
                        definition: edge,
                        policy: RelationshipPolicy::default(),
                        pipe: DesiredPipe::Bounded { capacity },
                    });
                }
                GraphConnectionKind::Bootstrap => {
                    let (from, to) = (&edge.from.component, &edge.to.component);
                    anyhow::ensure!(
                        enabled[to],
                        "component '{to}' disables bootstrap but has a bootstrap connection from '{from}'"
                    );
                    if let Some(provider) = local_providers.get(to) {
                        anyhow::ensure!(
                            provider == from,
                            "component '{to}' has the exclusive local bootstrapProvider '{provider}'; remove its bootstrap connection from '{from}'"
                        );
                    }
                    explicit_bootstrap.insert(to.clone());
                    bootstrap_edges.insert(edge.clone());
                    definition
                        .relationships
                        .push(bootstrap_relationship(edge, capacity));
                }
            }
        }
        let mut generated = Vec::new();
        for (component, provider) in &local_providers {
            generated.push(EdgeDefinition::new(
                Endpoint::new(
                    provider.clone(),
                    bootstrap_port(&descriptors[provider], PortDirection::Output)?.with_context(
                        || format!("bootstrap provider '{provider}' has no bootstrap output"),
                    )?,
                ),
                Endpoint::new(
                    component.clone(),
                    bootstrap_port(&descriptors[component], PortDirection::Input)?
                        .context("bootstrap input")?,
                ),
            ));
        }
        for (from, to) in data_inputs {
            if !enabled[&to]
                || local_providers.contains_key(&to)
                || explicit_bootstrap.contains(&to)
            {
                continue;
            }
            let (Some(output), Some(input)) = (
                bootstrap_port(&descriptors[&from], PortDirection::Output)?,
                bootstrap_port(&descriptors[&to], PortDirection::Input)?,
            ) else {
                continue;
            };
            generated.push(EdgeDefinition::new(
                Endpoint::new(from, output),
                Endpoint::new(to, input),
            ));
        }
        for edge in generated {
            if bootstrap_edges.insert(edge.clone()) {
                definition
                    .relationships
                    .push(bootstrap_relationship(edge, self.default_pipe_capacity));
            }
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

fn default_enable_bootstrap() -> bool {
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
