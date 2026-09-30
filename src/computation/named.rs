// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{collections::BTreeMap, num::NonZeroUsize, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use drasi_lib::computation::v1::*;
use serde::{Deserialize, Serialize};

use super::{ComputationConfig, ComputationResourceConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum NamedTransport {
    Bounded {
        capacity: usize,
    },
    Broadcast {
        capacity: usize,
        #[serde(rename = "lagPolicy")]
        lag_policy: BroadcastLagPolicy,
    },
    Qos {
        definition: QosChannelDefinition,
        path: Option<PathBuf>,
    },
}

impl NamedTransport {
    pub(super) fn role(&self) -> ResourceRole {
        match self {
            Self::Qos { .. } => ResourceRole::StateStore,
            _ => ResourceRole::Pipe,
        }
    }

    fn point_provider(&self) -> Result<Box<dyn PipeProvider>> {
        match self {
            Self::Bounded { capacity } => Ok(Box::new(BoundedPipeConfig {
                capacity: *capacity,
            })),
            Self::Broadcast {
                capacity,
                lag_policy,
            } => Ok(Box::new(BroadcastPipeConfig {
                capacity: *capacity,
                lag_policy: *lag_policy,
            })),
            Self::Qos { .. } => anyhow::bail!("QoS requires a shared channel"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum NamedPipeConfig {
    Bounded {
        capacity: usize,
    },
    Broadcast {
        capacity: usize,
        #[serde(rename = "lagPolicy")]
        #[schema(value_type = String)]
        lag_policy: BroadcastLagPolicy,
    },
    Qos {
        capacity: NonZeroUsize,
        #[serde(default = "backpressure")]
        #[schema(value_type = String)]
        retention: RetentionPolicy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
    },
}

fn backpressure() -> RetentionPolicy {
    RetentionPolicy::Backpressure
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum PortBinding {
    Pipe(String),
    Options {
        pipe: String,
        #[serde(default)]
        #[schema(value_type = serde_json::Value)]
        policy: RelationshipPolicy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscriber: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(value_type = Option<serde_json::Value>)]
        start: Option<SubscriptionStart>,
        #[serde(default, rename = "gapPolicy", skip_serializing_if = "Option::is_none")]
        #[schema(value_type = Option<String>)]
        gap_policy: Option<ReplayGapPolicy>,
    },
}

impl<'de> Deserialize<'de> for PortBinding {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct BindingVisitor;
        impl<'de> serde::de::Visitor<'de> for BindingVisitor {
            type Value = PortBinding;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a pipe name or an input binding")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(PortBinding::Pipe(value.to_string()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Options {
                    pipe: String,
                    #[serde(default)]
                    policy: RelationshipPolicy,
                    subscriber: Option<String>,
                    start: Option<SubscriptionStart>,
                    gap_policy: Option<ReplayGapPolicy>,
                }
                // Do not buffer YAML values through an untagged enum: After(n)
                // and other Core enum values use native YAML tags.
                let value =
                    Options::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(PortBinding::Options {
                    pipe: value.pipe,
                    policy: value.policy,
                    subscriber: value.subscriber,
                    start: value.start,
                    gap_policy: value.gap_policy,
                })
            }
        }
        deserializer.deserialize_any(BindingVisitor)
    }
}

impl PortBinding {
    fn pipe(&self) -> &str {
        match self {
            Self::Pipe(pipe) | Self::Options { pipe, .. } => pipe,
        }
    }

    fn consumer(
        &self,
        endpoint: &Endpoint,
        qos: bool,
    ) -> Result<(
        RelationshipPolicy,
        String,
        SubscriptionStart,
        ReplayGapPolicy,
    )> {
        let default_id = format!(
            "{}:{}:{}",
            endpoint.component.as_str().len(),
            endpoint.component,
            endpoint.port
        );
        match self {
            Self::Pipe(_) => Ok((
                RelationshipPolicy::default(),
                default_id,
                SubscriptionStart::Earliest,
                ReplayGapPolicy::Strict,
            )),
            Self::Options {
                policy,
                subscriber,
                start,
                gap_policy,
                ..
            } => {
                anyhow::ensure!(
                    qos || (subscriber.is_none() && start.is_none() && gap_policy.is_none()),
                    "subscriber, start and gapPolicy require a QoS input binding"
                );
                Ok((
                    policy.clone(),
                    subscriber.clone().unwrap_or(default_id),
                    start.unwrap_or(SubscriptionStart::Earliest),
                    gap_policy.unwrap_or(ReplayGapPolicy::Strict),
                ))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamedComponentConfig {
    #[schema(value_type = serde_json::Value)]
    pub factory: ComponentSpecification,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    #[schema(value_type = BTreeMap<String, PortBinding>)]
    pub ports: BTreeMap<PortId, PortBinding>,
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, String>)]
    pub streams: BTreeMap<PortId, StreamId>,
    #[serde(default)]
    #[schema(value_type = serde_json::Value)]
    pub lifecycle: LifecyclePolicy,
    #[serde(default)]
    #[schema(value_type = String)]
    pub input_merge: InputMergePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamedGraphConfig {
    #[serde(default, deserialize_with = "unique_map")]
    pub pipes: BTreeMap<String, NamedPipeConfig>,
    #[serde(default, deserialize_with = "unique_map")]
    #[schema(value_type = BTreeMap<String, NamedComponentConfig>)]
    pub components: BTreeMap<ComponentId, NamedComponentConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub resources: Vec<ResourceSpecification>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(value_type = BTreeMap<String, serde_json::Value>)]
    pub resource_configurations: BTreeMap<ResourceId, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub relationships: Vec<DesiredRelationship>,
    #[serde(default)]
    #[schema(value_type = serde_json::Value)]
    pub requirements: PipeRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub control_connections: Vec<(ComponentId, ComponentId)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub subscriptions: Vec<(ComponentId, ComponentId)>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    #[schema(value_type = Vec<String>)]
    pub readiness_required: std::collections::BTreeSet<ComponentId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(value_type = BTreeMap<String, Vec<String>>)]
    pub component_resources: BTreeMap<ComponentId, std::collections::BTreeSet<ResourceId>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(value_type = BTreeMap<String, serde_json::Value>)]
    pub component_plugins: BTreeMap<ComponentId, PluginIdentity>,
    #[serde(default)]
    pub allow_incomplete: bool,
}

pub(super) fn optional_unique_map<'de, D, K, V>(
    deserializer: D,
) -> std::result::Result<Option<BTreeMap<K, V>>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Deserialize<'de> + Ord + std::fmt::Display,
    V: Deserialize<'de>,
{
    unique_map(deserializer).map(Some)
}

pub(super) fn unique_map<'de, D, K, V>(
    deserializer: D,
) -> std::result::Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Deserialize<'de> + Ord + std::fmt::Display,
    V: Deserialize<'de>,
{
    struct Unique<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K, V> serde::de::Visitor<'de> for Unique<K, V>
    where
        K: Deserialize<'de> + Ord + std::fmt::Display,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a mapping without duplicate keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<K, V>()? {
                if result.contains_key(&key) {
                    return Err(serde::de::Error::custom(format!("duplicate key '{key}'")));
                }
                result.insert(key, value);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

fn resource_id(name: &str) -> Result<ResourceId> {
    ComponentId::try_new(name)?;
    Ok(ResourceId::try_new(format!("server-pipe/{name}"))?)
}

fn external(id: &ResourceId, transport: &NamedTransport) -> Result<DesiredPipe> {
    let provider = transport.point_provider()?;
    let capabilities = provider.capabilities()?;
    Ok(DesiredPipe::External {
        binding: id.to_string(),
        capabilities: capabilities.supported().iter().copied().collect(),
        capacity: capabilities.capacity().map(NonZeroUsize::get),
        resources: BTreeMap::from([(id.clone(), ResourceRole::Pipe)]),
        exclusive_resources: vec![id.clone()],
    })
}

impl NamedGraphConfig {
    pub fn compile(self) -> Result<ComputationConfig> {
        let mut definition = drasi_lib::management::DesiredInstance::default().topology;
        definition.revision = GraphRevision(0);
        definition.resources = self.resources;
        definition.resource_configurations = self.resource_configurations;
        definition.relationships = self.relationships;
        definition.requirements = self.requirements;
        definition.control_connections = self.control_connections;
        definition.subscriptions = self.subscriptions;
        definition.readiness_required = self.readiness_required;
        definition.component_resources = self.component_resources;
        definition.component_plugins = self.component_plugins;
        definition.allow_incomplete = self.allow_incomplete;
        let mut producers: BTreeMap<String, Vec<Endpoint>> = BTreeMap::new();
        let mut consumers: BTreeMap<String, Vec<(Endpoint, PortBinding)>> = BTreeMap::new();
        for (id, component) in self.components {
            anyhow::ensure!(
                &id == component.factory.descriptor.id(),
                "component '{id}' differs from its factory descriptor ID"
            );
            for (port, binding) in component.ports {
                anyhow::ensure!(
                    self.pipes.contains_key(binding.pipe()),
                    "component '{id}' references unknown pipe '{}'",
                    binding.pipe()
                );
                let descriptor = component
                    .factory
                    .descriptor
                    .ports()
                    .iter()
                    .find(|value| value.id() == &port)
                    .with_context(|| format!("component '{id}' has no port '{port}'"))?;
                let endpoint = Endpoint::new(id.clone(), port);
                match descriptor.direction() {
                    PortDirection::Output => {
                        anyhow::ensure!(matches!(binding, PortBinding::Pipe(_)), "output port bindings use a pipe name; relationship/subscriber options belong on inputs");
                        producers
                            .entry(binding.pipe().to_string())
                            .or_default()
                            .push(endpoint);
                    }
                    PortDirection::Input => consumers
                        .entry(binding.pipe().to_string())
                        .or_default()
                        .push((endpoint, binding)),
                }
            }
            definition.components.push(DesiredComponent {
                descriptor: component.factory.descriptor.clone(),
                role: component.factory.role,
                completion: component.factory.completion,
                construction: ComponentConstruction::Factory(component.factory),
                streams: component.streams,
                lifecycle: component.lifecycle,
                input_merge: component.input_merge,
            });
        }
        for (name, config) in self.pipes {
            let id = resource_id(&name)?;
            anyhow::ensure!(
                !definition
                    .resources
                    .iter()
                    .any(|resource| resource.id == id),
                "named pipe '{name}' collides with resource {id}"
            );
            let outputs = producers.remove(&name).unwrap_or_default();
            let inputs = consumers.remove(&name).unwrap_or_default();
            anyhow::ensure!(
                outputs.len() == 1,
                "pipe '{name}' requires exactly one producer"
            );
            anyhow::ensure!(!inputs.is_empty(), "pipe '{name}' requires a consumer");
            let qos = matches!(config, NamedPipeConfig::Qos { .. });
            anyhow::ensure!(
                qos || inputs.len() == 1,
                "point-to-point pipe '{name}' requires exactly one consumer; use QoS for multicast"
            );
            let output = &outputs[0];
            let mut subscribers = BTreeMap::new();
            let mut connections = Vec::new();
            for (input, binding) in inputs {
                let (policy, subscriber, start, gap_policy) = binding.consumer(&input, qos)?;
                anyhow::ensure!(
                    subscribers.insert(subscriber.clone(), start).is_none(),
                    "pipe '{name}' repeats subscriber '{subscriber}'"
                );
                connections.push((input, policy, subscriber, gap_policy));
            }
            let transport = match config {
                NamedPipeConfig::Bounded { capacity } => NamedTransport::Bounded { capacity },
                NamedPipeConfig::Broadcast {
                    capacity,
                    lag_policy,
                } => NamedTransport::Broadcast {
                    capacity,
                    lag_policy,
                },
                NamedPipeConfig::Qos {
                    capacity,
                    retention,
                    path,
                } => {
                    let stream = definition
                        .components
                        .iter()
                        .find(|component| component.descriptor.id() == &output.component)
                        .and_then(|component| component.streams.get(&output.port))
                        .context("QoS producer must declare its output stream")?;
                    NamedTransport::Qos {
                        definition: QosChannelDefinition {
                            stream: stream.clone(),
                            capacity,
                            durable: path.is_some(),
                            retention,
                            subscribers,
                        },
                        path,
                    }
                }
            };
            for (input, policy, subscriber, gap_policy) in connections {
                let pipe = match &transport {
                    NamedTransport::Qos { definition, .. } => DesiredPipe::Qos(QosPipeConfig {
                        resource: id.clone(),
                        subscriber,
                        definition: definition.clone(),
                        gap_policy,
                    }),
                    _ => external(&id, &transport)?,
                };
                definition.relationships.push(DesiredRelationship {
                    definition: EdgeDefinition::new(output.clone(), input),
                    policy,
                    pipe,
                });
            }
            definition.resources.push(ResourceSpecification {
                id: id.clone(),
                role: transport.role(),
                ownership: ResourceOwnership::Graph,
                binding: Arc::from(id.as_str()),
            });
            definition.resource_configurations.insert(
                id,
                serde_json::to_value(ComputationResourceConfig::NamedPipe { name, transport })?,
            );
        }
        let config = ComputationConfig { definition };
        super::validate_definition(&config)?;
        Ok(config)
    }
}

pub(super) fn declarations(
    definition: &DesiredTopology,
) -> Result<BTreeMap<ResourceId, (String, NamedTransport)>> {
    let mut result = BTreeMap::new();
    for (id, value) in &definition.resource_configurations {
        if value.get("kind").and_then(serde_json::Value::as_str) == Some("namedPipe") {
            let ComputationResourceConfig::NamedPipe { name, transport } =
                serde_json::from_value(value.clone())?
            else {
                unreachable!()
            };
            anyhow::ensure!(
                resource_id(&name)? == *id,
                "named pipe resource '{id}' does not match its name '{name}'"
            );
            let resource = definition
                .resources
                .iter()
                .find(|resource| &resource.id == id)
                .context("named pipe resource is undeclared")?;
            anyhow::ensure!(
                resource.role == transport.role()
                    && resource.ownership == ResourceOwnership::Graph
                    && resource.binding.as_ref() == id.as_str(),
                "named pipe '{name}' requires its canonical graph-owned resource"
            );
            result.insert(id.clone(), (name, transport));
        }
    }
    Ok(result)
}

pub(super) fn validate(definition: &DesiredTopology) -> Result<()> {
    validate_recipes(definition, true)
}

pub(super) fn validate_in_instance(definition: &DesiredTopology) -> Result<()> {
    validate_recipes(definition, false)
}

fn validate_recipes(definition: &DesiredTopology, reject_unknown_external: bool) -> Result<()> {
    let named = declarations(definition)?;
    for edge in &definition.relationships {
        if let DesiredPipe::External { binding, .. } = &edge.pipe {
            let Some((id, (_, transport))) = named.iter().find(|(id, _)| id.as_str() == binding)
            else {
                anyhow::ensure!(
                    !reject_unknown_external,
                    "external pipe binding cannot be reconstructed"
                );
                continue;
            };
            anyhow::ensure!(
                edge.pipe == external(id, transport)?,
                "named pipe external descriptor differs from its construction recipe"
            );
        }
    }
    for (id, (name, transport)) in &named {
        let edges: Vec<_> = definition
            .relationships
            .iter()
            .filter(|edge| match &edge.pipe {
                DesiredPipe::External { resources, .. } => resources.contains_key(id),
                DesiredPipe::Qos(config) => &config.resource == id,
                _ => false,
            })
            .collect();
        anyhow::ensure!(!edges.is_empty(), "named pipe '{name}' has no connections");
        match transport {
            NamedTransport::Qos { definition, path } => {
                definition.validate()?;
                anyhow::ensure!(
                    definition.durable == path.is_some()
                        && path
                            .as_ref()
                            .is_none_or(|path| !path.as_os_str().is_empty()),
                    "durable named QoS requires a nonempty storage root"
                );
                let mut actual = std::collections::BTreeSet::new();
                for edge in &edges {
                    let DesiredPipe::Qos(config) = &edge.pipe else {
                        anyhow::bail!("named QoS requires QoS relationships")
                    };
                    anyhow::ensure!(
                        &config.definition == definition
                            && edge.definition.from == edges[0].definition.from,
                        "named QoS channel or producer differs between subscribers"
                    );
                    anyhow::ensure!(
                        actual.insert(config.subscriber.clone()),
                        "duplicate named QoS subscriber"
                    );
                    config.capabilities()?;
                }
                anyhow::ensure!(
                    actual == definition.subscribers.keys().cloned().collect(),
                    "named QoS requires a binding for every subscriber"
                );
            }
            _ => {
                anyhow::ensure!(
                    edges.len() == 1 && edges[0].pipe == external(id, transport)?,
                    "point-to-point named pipe '{name}' must own exactly one connection"
                );
            }
        }
    }
    Ok(())
}

struct NamedProvider {
    id: ResourceId,
    transport: NamedTransport,
    specification: DesiredPipe,
}

impl PipeProvider for NamedProvider {
    fn capabilities(&self) -> std::result::Result<PipeCapabilities, PipeError> {
        self.transport
            .point_provider()
            .map_err(PipeError::Backend)?
            .capabilities()
    }
    fn specification(&self) -> Option<DesiredPipe> {
        Some(self.specification.clone())
    }
    fn resource_dependencies(&self) -> BTreeMap<ResourceId, ResourceRole> {
        BTreeMap::from([(self.id.clone(), ResourceRole::Pipe)])
    }
    fn exclusive_resources(&self) -> Vec<ResourceId> {
        vec![self.id.clone()]
    }
    fn validate_resources(
        &self,
        resources: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> std::result::Result<(), PipeError> {
        if let Some(resource) = resources.get(&self.id) {
            let transport = resource
                .get::<NamedTransport>()
                .map_err(|error| PipeError::Backend(error.into()))?;
            if *transport != self.transport {
                return Err(PipeError::Backend(anyhow::anyhow!(
                    "named pipe resource differs from its declaration"
                )));
            }
        }
        Ok(())
    }
    fn create(&self) -> std::result::Result<ProvidedPipe, PipeError> {
        Err(PipeError::Backend(anyhow::anyhow!(
            "named pipe requires its declared resource"
        )))
    }
    fn create_with_resources(
        &self,
        resources: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> std::result::Result<ProvidedPipe, PipeError> {
        self.validate_resources(resources)?;
        let transport = resources
            .get(&self.id)
            .ok_or_else(|| PipeError::Backend(anyhow::anyhow!("named pipe resource is not bound")))?
            .get::<NamedTransport>()
            .map_err(|error| PipeError::Backend(error.into()))?;
        transport
            .point_provider()
            .map_err(PipeError::Backend)?
            .create()
    }
}

pub(super) fn bind(definition: &DesiredTopology, bindings: &mut TopologyBindings) -> Result<()> {
    validate(definition)?;
    for (id, (_, transport)) in declarations(definition)? {
        if !matches!(transport, NamedTransport::Qos { .. }) {
            let specification = external(&id, &transport)?;
            bindings.pipes.insert(
                id.to_string(),
                Box::new(NamedProvider {
                    id,
                    transport,
                    specification,
                }),
            );
        }
    }
    Ok(())
}

pub(super) fn export(definition: &DesiredTopology) -> Result<Option<NamedGraphConfig>> {
    let named = declarations(definition)?;
    if named.is_empty() {
        return Ok(None);
    }
    super::validate_definition(&ComputationConfig {
        definition: definition.clone(),
    })?;
    anyhow::ensure!(
        definition.boundary_relationships.is_empty(),
        "named pipe export cannot omit boundary relationships"
    );
    let mut result = NamedGraphConfig {
        resources: definition
            .resources
            .iter()
            .filter(|resource| !named.contains_key(&resource.id))
            .cloned()
            .collect(),
        resource_configurations: definition
            .resource_configurations
            .iter()
            .filter(|(id, _)| !named.contains_key(*id))
            .map(|(id, recipe)| (id.clone(), recipe.clone()))
            .collect(),
        requirements: definition.requirements.clone(),
        control_connections: definition.control_connections.clone(),
        subscriptions: definition.subscriptions.clone(),
        readiness_required: definition.readiness_required.clone(),
        component_resources: definition.component_resources.clone(),
        component_plugins: definition.component_plugins.clone(),
        allow_incomplete: definition.allow_incomplete,
        ..Default::default()
    };
    for component in &definition.components {
        let ComponentConstruction::Factory(factory) = &component.construction else {
            anyhow::bail!("named component export requires a factory specification")
        };
        result.components.insert(
            component.descriptor.id().clone(),
            NamedComponentConfig {
                factory: factory.clone(),
                ports: BTreeMap::new(),
                streams: component.streams.clone(),
                lifecycle: component.lifecycle.clone(),
                input_merge: component.input_merge,
            },
        );
    }
    for (name, transport) in named.values() {
        result.pipes.insert(
            name.clone(),
            match transport {
                NamedTransport::Bounded { capacity } => NamedPipeConfig::Bounded {
                    capacity: *capacity,
                },
                NamedTransport::Broadcast {
                    capacity,
                    lag_policy,
                } => NamedPipeConfig::Broadcast {
                    capacity: *capacity,
                    lag_policy: *lag_policy,
                },
                NamedTransport::Qos { definition, path } => NamedPipeConfig::Qos {
                    capacity: definition.capacity,
                    retention: definition.retention,
                    path: path.clone(),
                },
            },
        );
    }
    for edge in &definition.relationships {
        let id = match &edge.pipe {
            DesiredPipe::External { binding, .. } => named.keys().find(|id| id.as_str() == binding),
            DesiredPipe::Qos(config) => named.get_key_value(&config.resource).map(|(id, _)| id),
            _ => None,
        };
        let Some(id) = id else {
            result.relationships.push(edge.clone());
            continue;
        };
        let (name, transport) = &named[id];
        let output = &edge.definition.from;
        let input = &edge.definition.to;
        let producer = result
            .components
            .get_mut(&output.component)
            .context("named producer missing")?;
        if let Some(prior) = producer.ports.get(&output.port) {
            anyhow::ensure!(
                prior.pipe() == name,
                "one output port cannot bind multiple named pipes"
            );
        } else {
            producer
                .ports
                .insert(output.port.clone(), PortBinding::Pipe(name.clone()));
        }
        let (subscriber, start, gap_policy) = match (&edge.pipe, transport) {
            (DesiredPipe::Qos(config), NamedTransport::Qos { definition, .. }) => (
                Some(config.subscriber.clone()),
                Some(definition.subscribers[&config.subscriber]),
                Some(config.gap_policy),
            ),
            _ => (None, None, None),
        };
        let consumer = result
            .components
            .get_mut(&input.component)
            .context("named consumer missing")?;
        anyhow::ensure!(
            consumer
                .ports
                .insert(
                    input.port.clone(),
                    PortBinding::Options {
                        pipe: name.clone(),
                        policy: edge.policy.clone(),
                        subscriber,
                        start,
                        gap_policy
                    }
                )
                .is_none(),
            "one input port cannot bind multiple named pipes"
        );
    }
    Ok(Some(result))
}

pub(super) fn storage_path(root: PathBuf, instance: &str, name: &str) -> PathBuf {
    fn encoded(value: &str) -> String {
        value.bytes().map(|byte| format!("{byte:02x}")).collect()
    }
    root.join(encoded(instance)).join(encoded(name))
}
