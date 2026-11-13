// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_core::models::{
    Element, ElementMetadata, ElementPropertyMap, ElementReference, ElementValue, SourceChange,
};
use drasi_lib::{channels::ResultDiff, computation::v1::*, DrasiLib};
use drasi_reaction_application::ApplicationReaction;
use drasi_server::{
    computation::{
        build_components, configuration_from_snapshot, validate_definition, ComputationConfig,
    },
    plugin_registry::PluginRegistry,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, num::NonZeroUsize, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex};

const DEADLINE: Duration = Duration::from_secs(10);

fn id(value: &str) -> ComponentId {
    let value = match value {
        "input" => 1,
        "one" => 2,
        "two" => 3,
        "outlet" => 4,
        other => panic!("missing explicit component ID for {other}"),
    };
    ComponentId::from_uuid(uuid::Uuid::from_u128(value))
}
fn resource(value: &str) -> ResourceId {
    ResourceId::try_new(value).unwrap()
}
fn endpoint(component: &str, port: &str) -> Endpoint {
    Endpoint::new(id(component), PortId::try_new(port).unwrap())
}

fn source_descriptor() -> ComponentDescriptor {
    ComponentDescriptor::try_new(
        id("input"),
        vec![PortDescriptor::new(
            PortId::try_new("out").unwrap(),
            PortDirection::Output,
            GraphChangeCodec::schema().descriptor().clone(),
            PipeRequirements::default(),
        )],
    )
    .unwrap()
}

fn desired(name: &str, specification: ComponentSpecification) -> DesiredComponent {
    let streams = specification
        .descriptor
        .ports()
        .iter()
        .filter(|port| port.direction() == PortDirection::Output)
        .map(|port| {
            (
                port.id().clone(),
                StreamId::try_new(format!("{}/out", specification.descriptor.id())).unwrap(),
            )
        })
        .collect();
    DesiredComponent {
        name: ComponentName::try_new(name).unwrap(),
        descriptor: specification.descriptor.clone(),
        role: specification.role,
        completion: specification.completion,
        streams,
        lifecycle: LifecyclePolicy::default(),
        input_merge: InputMergePolicy::default(),
        construction: ComponentConstruction::Factory(specification),
    }
}

fn configuration() -> Result<ComputationConfig> {
    let graph_id = drasi_lib::management::DesiredInstance::default()
        .topology
        .graph_id;
    let catalog = resource("results");
    let indexes = resource("memory");
    let mut components = vec![desired(
        "input",
        ComponentSpecification {
            descriptor: source_descriptor(),
            role: ComponentRole::Source,
            completion: None,
            implementation: ImplementationIdentity::try_new("test/catalog-input", "1")?,
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        },
    )];
    for name in ["one", "two"] {
        let query = ContinuousQueryDefinition {
            graph_id: graph_id.clone(),
            id: id(name),
            query: "MATCH (n:Item) RETURN n.id AS id, n.value AS value".into(),
            language: ComputationQueryLanguage::Cypher,
            output_stream: StreamId::try_new(format!("{name}/out"))?,
            outbox_capacity: NonZeroUsize::new(16).unwrap(),
        };
        components.push(desired(
            name,
            ComponentSpecification {
                descriptor: query.descriptor(),
                role: ComponentRole::Transformer,
                completion: None,
                implementation: ContinuousQueryFactory::default()
                    .descriptor()
                    .implementation
                    .clone(),
                configuration_version: 1,
                configuration: BTreeMap::from([
                    (
                        "query".into(),
                        ConfigurationValue::Literal(json!(query.query)),
                    ),
                    (
                        "stream".into(),
                        ConfigurationValue::Literal(json!(query.output_stream)),
                    ),
                ]),
                dependencies: BTreeMap::from([
                    ("indexes".into(), vec![indexes.clone()]),
                    ("catalog".into(), vec![catalog.clone()]),
                    (
                        "source_progress".into(),
                        vec![resource(&format!("{name}-progress"))],
                    ),
                ]),
            },
        ));
    }
    components.push(desired(
        "outlet",
        ComponentSpecification {
            descriptor: QueryResultsOutlet::new(
                id("outlet"),
                QueryResultsCatalog::new(graph_id.as_str())?,
            )
            .descriptor()
            .clone(),
            role: ComponentRole::Sink,
            completion: Some(SinkCompletion::Handled),
            implementation: QueryResultsOutletFactory::default()
                .descriptor()
                .implementation
                .clone(),
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::from([("catalog".into(), vec![catalog.clone()])]),
        },
    ));
    let relationships = [
        ("input", "one"),
        ("input", "two"),
        ("one", "outlet"),
        ("two", "outlet"),
    ]
    .into_iter()
    .map(|(from, to)| DesiredRelationship {
        definition: EdgeDefinition::new(endpoint(from, "out"), endpoint(to, "in")),
        policy: RelationshipPolicy::default(),
        pipe: DesiredPipe::Bounded { capacity: 16 },
    })
    .collect::<Vec<_>>();
    let resources = [
        (indexes.clone(), ResourceRole::IndexBackend),
        (catalog.clone(), ResourceRole::QueryCatalog),
        (resource("one-progress"), ResourceRole::Checkpoint),
        (resource("two-progress"), ResourceRole::Checkpoint),
    ]
    .into_iter()
    .map(|(id, role)| ResourceSpecification {
        binding: Arc::from(id.as_str()),
        id,
        role,
        ownership: ResourceOwnership::Graph,
    })
    .collect::<Vec<_>>();
    Ok(ComputationConfig {
        definition: serde_json::from_value(json!({
            "version":DESIRED_TOPOLOGY_VERSION, "graph_id":graph_id, "revision":0,
            "components":components, "relationships":relationships, "resources":resources,
            "resource_configurations":{
                "memory":{"kind":"memoryIndexes"},
                "results":{"kind":"queryCatalog"},
                "one-progress":{"kind":"sourceProgress","component":id("one")},
                "two-progress":{"kind":"sourceProgress","component":id("two")}
            },
            "requirements":PipeRequirements::default(), "boundary_relationships":[],
        }))?,
    })
}

fn specification<'a>(
    config: &'a mut ComputationConfig,
    name: &str,
) -> &'a mut ComponentSpecification {
    let component = config
        .definition
        .components
        .iter_mut()
        .find(|component| component.name.as_str() == name)
        .unwrap();
    let ComponentConstruction::Factory(specification) = &mut component.construction else {
        panic!("expected factory specification");
    };
    specification
}

#[test]
fn query_catalog_recipe_roundtrips_with_a_shared_outlet() -> Result<()> {
    let config = configuration()?;
    validate_definition(&config)?;
    let yaml = serde_yaml::to_string(&config)?;
    assert!(yaml.contains("kind: queryCatalog"));
    assert!(yaml.contains("kind: sourceProgress"));
    let restored: ComputationConfig = serde_yaml::from_str(&yaml)?;
    validate_definition(&restored)?;
    assert_eq!(restored.definition, config.definition);
    Ok(())
}

#[test]
fn progress_recipes_reject_wrong_roles_and_cannot_restore_live_state() -> Result<()> {
    for recipe in [
        json!({"kind":"sourceProgress","component":""}),
        json!({"kind":"sourceProgress","component":"one","persistent":true}),
        json!({"kind":"sourceProgress","component":"one","ready":true}),
        json!({"kind":"sourceProgress","component":"one","graph":"other"}),
        json!({"kind":"sourceProgress","component":"one","checkpoints":{}}),
    ] {
        let mut config = configuration()?;
        config
            .definition
            .resource_configurations
            .insert(resource("one-progress"), recipe);
        assert!(validate_definition(&config).is_err());
    }
    let mut config = configuration()?;
    config
        .definition
        .resources
        .iter_mut()
        .find(|value| value.id == resource("one-progress"))
        .unwrap()
        .role = ResourceRole::StateStore;
    assert!(validate_definition(&config).is_err());
    Ok(())
}

#[test]
fn rejects_missing_wrong_or_distinct_catalog_dependencies() -> Result<()> {
    for component in ["one", "outlet"] {
        for dependencies in [
            None,
            Some(Vec::new()),
            Some(vec![resource("results"), resource("results")]),
        ] {
            let mut config = configuration()?;
            let spec = specification(&mut config, component);
            spec.dependencies.remove("catalog");
            if let Some(dependencies) = dependencies {
                spec.dependencies.insert("catalog".into(), dependencies);
            }
            let error = validate_definition(&config).unwrap_err();
            assert!(
                error.to_string().contains("exactly one catalog"),
                "{error:#}"
            );
        }
    }
    let mut config = configuration()?;
    let second = ResourceSpecification {
        id: resource("other"),
        role: ResourceRole::QueryCatalog,
        ownership: ResourceOwnership::Graph,
        binding: "other".into(),
    };
    config.definition.resources.push(second);
    config
        .definition
        .resource_configurations
        .insert(resource("other"), json!({"kind":"queryCatalog"}));
    specification(&mut config, "outlet")
        .dependencies
        .insert("catalog".into(), vec![resource("other")]);
    let error = validate_definition(&config).unwrap_err();
    assert!(error.to_string().contains("same queryCatalog"), "{error:#}");
    let mut config = configuration()?;
    for component in ["one", "two", "outlet"] {
        specification(&mut config, component)
            .dependencies
            .insert("catalog".into(), vec![resource("memory")]);
    }
    let error = validate_definition(&config).unwrap_err();
    assert!(
        error.to_string().contains("must reference a queryCatalog"),
        "{error:#}"
    );
    let mut config = configuration()?;
    config.definition.resource_configurations.insert(
        resource("results"),
        json!({"kind":"queryCatalog","graph":"wrong"}),
    );
    assert!(validate_definition(&config).is_err());
    Ok(())
}

#[test]
fn direct_native_queries_do_not_require_a_subscription_catalog() -> Result<()> {
    let mut config = configuration()?;
    config
        .definition
        .components
        .retain(|component| component.descriptor.id() != &id("outlet"));
    config
        .definition
        .relationships
        .retain(|edge| edge.definition.to.component != id("outlet"));
    config
        .definition
        .resources
        .retain(|value| value.id != resource("results"));
    config
        .definition
        .resource_configurations
        .remove(&resource("results"));
    for name in ["one", "two"] {
        specification(&mut config, name)
            .dependencies
            .remove("catalog");
    }
    validate_definition(&config)
}

#[tokio::test(flavor = "current_thread")]
async fn progress_recipe_must_name_its_actual_consumer() -> Result<()> {
    let mut config = configuration()?;
    config
        .definition
        .components
        .retain(|component| component.descriptor.id() == &id("one"));
    config.definition.relationships.clear();
    config.definition.resource_configurations.insert(
        resource("one-progress"),
        json!({"kind":"sourceProgress","component":"two"}),
    );
    let core = crate::query_plugin::builder()
        .with_id("invalid-progress-owner")
        .build()
        .await?;
    let mut plugins = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugins);
    let result = build_components(
        &config,
        &core,
        plugins.computation_factory_registry()?,
        plugins.transactional_transformer_registry(core.middleware_registry())?,
    )
    .await;
    core.shutdown().await?;
    let error = result
        .err()
        .context("foreign progress owner was accepted")?;
    assert!(
        format!("{error:#}").contains("source progress belongs to another graph/query"),
        "{error:#}"
    );
    Ok(())
}

struct InputSource {
    descriptor: ComponentDescriptor,
    input: mpsc::Receiver<OutputEnvelope>,
}
#[async_trait]
impl ComputationComponent for InputSource {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    async fn start(&mut self) -> Result<()> {
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
#[async_trait]
impl EnvelopeSource for InputSource {
    async fn next(&mut self) -> Result<Option<OutputEnvelope>> {
        Ok(self.input.recv().await)
    }
}
struct InputFactory {
    descriptor: FactoryDescriptor,
    input: Mutex<Option<mpsc::Receiver<OutputEnvelope>>>,
}
#[async_trait]
impl ComponentFactory for InputFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }
    fn validate(&self, _: &ComponentSpecification) -> Result<()> {
        Ok(())
    }
    async fn create(
        &self,
        _: ConstructionContext,
    ) -> std::result::Result<ConstructedComponent, ComponentCreationError> {
        let input = self.input.lock().await.take().ok_or_else(|| {
            ComponentCreationError::terminal(anyhow::anyhow!("input already created"))
        })?;
        Ok(ConstructedComponent::source(Box::new(InputSource {
            descriptor: source_descriptor(),
            input,
        })))
    }
}

async fn instance(
    name: &str,
) -> Result<(
    DrasiLib,
    mpsc::Sender<OutputEnvelope>,
    mpsc::Receiver<drasi_lib::channels::QueryResult>,
)> {
    let (reaction, handle) = ApplicationReaction::new(
        ComponentId::generate(),
        vec![
            drasi_lib::config::ComponentReference::by_name(ComponentName::try_new("one")?),
            drasi_lib::config::ComponentReference::by_name(ComponentName::try_new("two")?),
        ],
    );
    let core = crate::query_plugin::builder()
        .with_id(name)
        .with_allow_external_component_ids(true)
        .with_reaction(ComponentName::try_new("capture")?, reaction)
        .build()
        .await?;
    let (sender, input) = mpsc::channel(16);
    let mut plugins = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugins);
    let mut factories = plugins.computation_factory_registry()?;
    factories.register(Arc::new(InputFactory {
        descriptor: FactoryDescriptor {
            implementation: ImplementationIdentity::try_new("test/catalog-input", "1")?,
            role: ComponentRole::Source,
            configuration_version: 1,
            configuration: ConfigurationSchema::default(),
            dependencies: BTreeMap::new(),
        },
        input: Mutex::new(Some(input)),
    }))?;
    let config = configuration()?;
    let batch = build_components(
        &config,
        &core,
        factories,
        plugins.transactional_transformer_registry(core.middleware_registry())?,
    )
    .await?;
    core.add_components(batch.auto_start(false)).await?;
    for query in ["one", "two"] {
        core.component_by_name(&ComponentName::try_new(query)?)?
            .wait_created()
            .await?;
        assert!(
            core.get_query_results_by_name(&ComponentName::try_new(query)?)
                .await
                .is_err(),
            "unstarted query is not running"
        );
    }
    let snapshot = core.snapshot_computation_configuration().await?;
    let restored = configuration_from_snapshot(&snapshot)?.context("native definition missing")?;
    assert_eq!(
        restored.definition.resource_configurations[&resource("results")],
        json!({"kind":"queryCatalog"})
    );
    for query in ["one", "two"] {
        assert_eq!(
            restored.definition.resource_configurations[&resource(&format!("{query}-progress"))],
            json!({"kind":"sourceProgress","component":query})
        );
    }
    let control = core.computation_control()?;
    let started = control
        .start_requested(
            control.desired_snapshot().revision,
            GraphSelection::Exact(
                ["input", "one", "two", "outlet"]
                    .into_iter()
                    .map(id)
                    .collect(),
            ),
        )
        .await?;
    assert_eq!(started.summary, OperationSummary::Completed, "{started:?}");
    core.start().await?;
    for component in ["input", "one", "two", "outlet", "capture"] {
        tokio::time::timeout(
            DEADLINE,
            core.component_by_name(&ComponentName::try_new(component)?)?
                .wait_started(),
        )
        .await??;
    }
    Ok((
        core,
        sender,
        handle
            .take_receiver()
            .await
            .context("reaction receiver missing")?,
    ))
}

fn change(sequence: u64, value: Option<i64>) -> Result<OutputEnvelope> {
    let metadata = ElementMetadata {
        reference: ElementReference::new("input", "row"),
        labels: Arc::from([Arc::from("Item")]),
        effective_from: sequence,
    };
    let change = if let Some(value) = value {
        let mut properties = ElementPropertyMap::new();
        properties.insert("id", ElementValue::String("row".into()));
        properties.insert("value", ElementValue::Integer(value));
        let element = Element::Node {
            metadata,
            properties,
        };
        if sequence == 1 {
            SourceChange::Insert { element }
        } else {
            SourceChange::Update {
                element,
                removed_properties: Default::default(),
            }
        }
    } else {
        SourceChange::Delete { metadata }
    };
    Ok(OutputEnvelope {
        port: PortId::try_new("out")?,
        envelope: GraphChangeCodec::encode_change(
            change,
            StreamId::try_new("input/out")?,
            sequence,
            None,
        )?,
    })
}

#[tokio::test]
async fn server_catalog_feeds_existing_reactions_and_keeps_instances_isolated() -> Result<()> {
    let (first, sender, mut output) = instance("first").await?;
    let (second, _other_sender, _other_output) = instance("second").await?;
    for (sequence, value) in [(1, Some(7)), (2, Some(11)), (3, None)] {
        sender.send(change(sequence, value)?).await?;
        let mut queries = std::collections::BTreeSet::new();
        while queries.len() < 2 {
            let batch = tokio::time::timeout(DEADLINE, output.recv())
                .await?
                .context("result missing")?;
            assert!(
                queries.insert(batch.query_id.clone()),
                "duplicate query delivery"
            );
            match batch.results.as_slice() {
                [ResultDiff::Add { data, .. }] if sequence == 1 => {
                    assert_eq!(data, &json!({"id":"row","value":7}))
                }
                [ResultDiff::Update { before, after, .. }] if sequence == 2 => {
                    assert_eq!(before, &json!({"id":"row","value":7}));
                    assert_eq!(after, &json!({"id":"row","value":11}));
                }
                [ResultDiff::Delete { data, .. }] if sequence == 3 => {
                    assert_eq!(data, &json!({"id":"row","value":11}))
                }
                other => anyhow::bail!("unexpected output for sequence {sequence}: {other:?}"),
            }
        }
        for query in ["one", "two"] {
            let actual = first
                .get_query_results_by_name(&ComponentName::try_new(query)?)
                .await?;
            let expected = value
                .map(|value| vec![json!({"id":"row","value":value})])
                .unwrap_or_default();
            assert_eq!(actual, expected);
            assert_eq!(
                second
                    .get_query_results_by_name(&ComponentName::try_new(query)?)
                    .await?,
                Vec::<Value>::new()
            );
        }
    }
    first.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[path = "query_plugin/mod.rs"]
mod query_plugin;
