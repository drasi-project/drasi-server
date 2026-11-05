// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_host_sdk::{management::SourceTimeMergeStorageConfig, PluginRegistry};
use drasi_lib::{computation::v1::*, management::DesiredInstance, DrasiLib};
use drasi_server::computation::{
    build_components, build_components_with_registry, configuration_from_snapshot,
    ComputationConfig, ComputationConfigInput,
};
use serde_json::json;
use tokio::sync::{watch, Notify};

struct Factory {
    descriptor: FactoryDescriptor,
    gate: AtomicBool,
    entered: Notify,
    release: Notify,
}

impl Factory {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            descriptor: FactoryDescriptor {
                implementation: ImplementationIdentity::try_new("test/reconstruction", "1")
                    .expect("identity"),
                role: ComponentRole::Service,
                configuration_version: 1,
                configuration: ConfigurationSchema::default(),
                dependencies: BTreeMap::new(),
            },
            gate: AtomicBool::new(false),
            entered: Notify::new(),
            release: Notify::new(),
        })
    }

    fn config(&self, version: u32) -> Result<ComputationConfig> {
        let mut definition = DesiredInstance::default().topology;
        let descriptor =
            ComponentDescriptor::try_new(ComponentId::try_new("service")?, Vec::new())?;
        definition.components.push(DesiredComponent {
            descriptor: descriptor.clone(),
            role: ComponentRole::Service,
            completion: None,
            streams: BTreeMap::new(),
            lifecycle: LifecyclePolicy { auto_start: false },
            input_merge: InputMergePolicy::default(),
            construction: ComponentConstruction::Factory(ComponentSpecification {
                descriptor,
                role: ComponentRole::Service,
                completion: None,
                implementation: self.descriptor.implementation.clone(),
                configuration_version: version,
                configuration: BTreeMap::new(),
                dependencies: BTreeMap::new(),
            }),
        });
        Ok(ComputationConfig { definition })
    }
}

struct Service(ComponentDescriptor);

#[async_trait]
impl ComputationComponent for Service {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.0
    }
    async fn start(&mut self) -> Result<()> {
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ComputationService for Service {
    async fn run(&mut self) -> Result<()> {
        std::future::pending().await
    }
}

#[async_trait]
impl ComponentFactory for Factory {
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }
    fn validate(&self, _: &ComponentSpecification) -> Result<()> {
        Ok(())
    }
    async fn create(
        &self,
        context: ConstructionContext,
    ) -> std::result::Result<ConstructedComponent, ComponentCreationError> {
        if self.gate.load(Ordering::Acquire) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(ConstructedComponent::service(Box::new(Service(
            context.specification.descriptor.clone(),
        ))))
    }
}

async fn batch(
    config: &ComputationConfig,
    core: &DrasiLib,
    factory: Arc<Factory>,
) -> Result<ComponentBatch> {
    let mut factories = FactoryRegistry::standard();
    factories.register(factory)?;
    build_components(
        config,
        core,
        factories,
        Arc::new(TransactionalTransformerRegistry::standard(
            core.middleware_registry(),
        )),
    )
    .await
}

#[tokio::test]
async fn server_construction_versions_fail_on_retained_nodes_not_before_declaration() -> Result<()>
{
    let factory = Factory::new();
    let core = DrasiLib::builder().build().await?;
    let config = factory.config(2)?;
    let prepared = batch(&config, &core, factory).await?;
    assert!(
        core.computation_component("service").is_err(),
        "preparation has no runtime side effects"
    );
    let report = core.add_components(prepared).await?;
    assert!(report.committed);
    let handle = core.computation_component("service")?;
    assert_eq!(
        handle.observed()?.realization,
        RealizationState::CreationFailed
    );
    assert!(handle.wait_created().await.is_err());
    let exported = configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
        .context("retained configuration")?;
    assert_eq!(exported.definition.components, config.definition.components);
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn missing_bootstrap_factories_restore_as_retryable_resource_declarations() -> Result<()> {
    let mut definition = DesiredInstance::default().topology;
    let resource = ResourceId::try_new("bootstrap")?;
    definition.resources.push(ResourceSpecification {
        id: resource.clone(),
        role: ResourceRole::Bootstrap,
        ownership: ResourceOwnership::Graph,
        binding: "bootstrap".into(),
    });
    definition.resource_configurations.insert(
        resource.clone(),
        json!({
            "kind":"nativeBootstrap", "component":"future-query",
            "implementation":ImplementationIdentity::try_new("unavailable/bootstrap", "1")?,
            "configurationVersion":1, "configuration":{}
        }),
    );
    let mut config = ComputationConfig {
        definition: DesiredInstance::from(definition).normalized()?.topology,
    };
    for _ in 0..2 {
        let core = DrasiLib::builder().with_id("same-scope").build().await?;
        let prepared =
            build_components_with_registry(&config, &core, &PluginRegistry::new()).await?;
        assert!(
            prepared.bindings.resources.is_empty(),
            "all recipes are constructed by the graph"
        );
        assert!(prepared
            .bindings
            .resource_constructors
            .contains_key(&resource));
        let report = core.add_components(prepared).await?;
        assert!(report.committed);
        let observed = core.computation_control()?.observed();
        let state = observed
            .resources
            .get(&resource)
            .context("retained resource")?;
        assert!(state.failure.is_some(), "{state:?}");
        let exported =
            configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
                .context("retained recipe")?;
        assert_eq!(
            exported.definition.resource_configurations,
            config.definition.resource_configurations
        );
        config = exported;
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_server_restoration_retains_the_accepted_definition() -> Result<()> {
    let factory = Factory::new();
    factory.gate.store(true, Ordering::Release);
    let config = factory.config(1)?;
    let core = DrasiLib::builder().build().await?;
    let prepared = batch(&config, &core, factory.clone()).await?;
    let waiter = tokio::spawn({
        let core = core.clone();
        async move { core.add_components(prepared).await }
    });
    tokio::time::timeout(Duration::from_secs(5), factory.entered.notified()).await?;
    waiter.abort();
    assert!(waiter.await.expect_err("cancelled caller").is_cancelled());
    let exported = configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
        .context("accepted definition")?;
    assert_eq!(exported.definition.components, config.definition.components);
    tokio::time::timeout(Duration::from_secs(5), core.shutdown()).await??;
    drop(core);
    factory.gate.store(false, Ordering::Release);
    let core = DrasiLib::builder().build().await?;
    let report = core
        .add_components(batch(&exported, &core, factory).await?)
        .await?;
    assert_eq!(report.summary, OperationSummary::Completed);
    core.shutdown().await?;
    Ok(())
}

#[test]
fn readable_source_time_merger_preserves_its_versioned_storage_dependency() -> Result<()> {
    let storage = SourceTimeMergeStorageConfig::new(
        ComponentId::try_new("merge")?,
        ResourceId::try_new("indexes")?,
    )?;
    let mut recipe = serde_json::to_value(storage)?;
    recipe["kind"] = "sourceTimeMergeStorage".into();
    let input: ComputationConfigInput = serde_json::from_value(json!({"graph":{
        "resources": {
            "indexes":{"kind":"rocksdbIndexes","path":"data/merge"},
            "storage":recipe
        },
        "resourceDependencies":{"storage":["indexes"]},
        "components":[{
            "id":"merge","kind":"drasi/source-time-merge","version":"1",
            "resources":{"storage":"storage"},
            "streams":{"out":"merge/out"},
            "config":{
                "definition":{
                    "graph_id":DesiredInstance::default().topology.graph_id,
                    "id":"merge","sources":["source/out"],"output_stream":"merge/out",
                    "late_output_stream":null,"reorder_window_ms":10,"max_wait_ms":100,
                    "idle_timeout_ms":null,"max_buffered_events":8,"max_buffered_bytes":65536,
                    "late_policy":"FailAndRetain"
                },
                "persistent":true
            }
        }]
    }}))?;
    let registry = FactoryRegistry::standard();
    let canonical = input.resolve(&registry)?.into_owned();
    assert_eq!(
        canonical.definition.resource_dependencies[&ResourceId::try_new("storage")?],
        BTreeMap::from([(ResourceId::try_new("indexes")?, ResourceRole::IndexBackend)])
    );
    let encoded = serde_json::to_string(&ComputationConfigInput::from(canonical.clone()))?;
    let decoded: ComputationConfigInput = serde_json::from_str(&encoded)?;
    assert_eq!(decoded.resolve(&registry)?.definition, canonical.definition);
    Ok(())
}

struct StreamFactory {
    descriptor: FactoryDescriptor,
    next: Arc<Mutex<Option<ChangeEnvelope>>>,
    output: watch::Sender<Vec<(String, ChangeEnvelope)>>,
}

impl StreamFactory {
    fn new(
        source: bool,
        next: Arc<Mutex<Option<ChangeEnvelope>>>,
        output: watch::Sender<Vec<(String, ChangeEnvelope)>>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            descriptor: FactoryDescriptor {
                implementation: ImplementationIdentity::try_new(
                    if source {
                        "test/server-source"
                    } else {
                        "test/server-sink"
                    },
                    "1",
                )?,
                role: if source {
                    ComponentRole::Source
                } else {
                    ComponentRole::Sink
                },
                configuration_version: 1,
                configuration: ConfigurationSchema::default(),
                dependencies: BTreeMap::new(),
            },
            next,
            output,
        }))
    }

    fn specification(&self, name: &str) -> Result<ComponentSpecification> {
        let source = self.descriptor.role == ComponentRole::Source;
        Ok(ComponentSpecification {
            descriptor: ComponentDescriptor::try_new(
                ComponentId::try_new(name)?,
                vec![PortDescriptor::new(
                    PortId::try_new(if source { "out" } else { "in" })?,
                    if source {
                        PortDirection::Output
                    } else {
                        PortDirection::Input
                    },
                    GraphChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                )],
            )?,
            role: self.descriptor.role,
            completion: (!source).then_some(SinkCompletion::Handled),
            implementation: self.descriptor.implementation.clone(),
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        })
    }
}

struct StreamComponent {
    descriptor: ComponentDescriptor,
    next: Option<ChangeEnvelope>,
    output: watch::Sender<Vec<(String, ChangeEnvelope)>>,
}

#[async_trait]
impl ComputationComponent for StreamComponent {
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
impl EnvelopeSource for StreamComponent {
    async fn next(&mut self) -> Result<Option<OutputEnvelope>> {
        Ok(self.next.take().map(|envelope| OutputEnvelope {
            port: PortId::try_new("out").expect("fixed output port"),
            envelope,
        }))
    }
}

#[async_trait]
impl EnvelopeSink for StreamComponent {
    fn completion(&self) -> SinkCompletion {
        SinkCompletion::Handled
    }
    async fn handle(&mut self, input: InputEnvelope) -> Result<()> {
        self.output.send_modify(|output| {
            output.push((self.descriptor.id().to_string(), input.envelope));
        });
        Ok(())
    }
}

#[async_trait]
impl ComponentFactory for StreamFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }
    fn validate(&self, _: &ComponentSpecification) -> Result<()> {
        Ok(())
    }
    async fn create(
        &self,
        context: ConstructionContext,
    ) -> std::result::Result<ConstructedComponent, ComponentCreationError> {
        let source = self.descriptor.role == ComponentRole::Source;
        let component = Box::new(StreamComponent {
            descriptor: context.specification.descriptor.clone(),
            next: if source {
                self.next
                    .lock()
                    .map_err(|_| {
                        ComponentCreationError::terminal(anyhow::anyhow!(
                            "source fixture ownership poisoned"
                        ))
                    })?
                    .take()
            } else {
                None
            },
            output: self.output.clone(),
        });
        Ok(if source {
            ConstructedComponent::source(component)
        } else {
            ConstructedComponent::sink(component)
        })
    }
}

fn source_event(sequence: u64, timestamp: u64) -> Result<ChangeEnvelope> {
    use drasi_core::models::{ElementMetadata, ElementReference, SourceChange};
    Ok(GraphChangeCodec::encode_change(
        SourceChange::Delete {
            metadata: ElementMetadata {
                reference: ElementReference::new("source", &sequence.to_string()),
                labels: Arc::from([]),
                effective_from: timestamp,
            },
        },
        StreamId::try_new("source/out")?,
        sequence,
        Some(
            chrono::DateTime::from_timestamp_millis(i64::try_from(timestamp)?)
                .context("source timestamp")?,
        ),
    )?)
}

#[tokio::test]
async fn server_rocksdb_merger_recipe_reopens_the_same_frontier_and_qos_journals() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), async {
        let directory = tempfile::tempdir()?;
        let next = Arc::new(Mutex::new(Some(source_event(1, 100)?)));
        let output = watch::channel(Vec::new()).0;
        let source = StreamFactory::new(true, next.clone(), output.clone())?;
        let sink = StreamFactory::new(false, next.clone(), output.clone())?;
        let mut factories = FactoryRegistry::standard();
        factories.register(source.clone())?;
        factories.register(sink.clone())?;
        let mut definition = DesiredInstance::default().topology;
        let merge = SourceTimeMergeDefinition {
            graph_id: definition.graph_id.clone(),
            id: ComponentId::try_new("merge")?,
            sources: vec![StreamId::try_new("source/out")?],
            output_stream: StreamId::try_new("merge/out")?,
            late_output_stream: Some(StreamId::try_new("merge/late")?),
            reorder_window_ms: 0,
            max_wait_ms: 5,
            idle_timeout_ms: None,
            max_buffered_events: NonZeroUsize::new(8).context("capacity")?,
            max_buffered_bytes: NonZeroUsize::new(65536).context("byte capacity")?,
            late_policy: LateEventPolicy::Route,
        };
        definition.components = vec![
            ComponentAddition::declaration(source.specification("source")?)
                .bind_stream(PortId::try_new("out")?, StreamId::try_new("source/out")?)
                .definition,
            ComponentAddition::declaration(
                merge.specification(Some(ResourceId::try_new("storage")?))?,
            )
            .bind_stream(PortId::try_new("out")?, StreamId::try_new("merge/out")?)
            .bind_stream(PortId::try_new("late")?, StreamId::try_new("merge/late")?)
            .definition,
            ComponentAddition::declaration(sink.specification("main")?).definition,
            ComponentAddition::declaration(sink.specification("late")?).definition,
        ];
        let mut storage = serde_json::to_value(SourceTimeMergeStorageConfig::new(
            ComponentId::try_new("merge")?,
            ResourceId::try_new("indexes")?,
        )?)?;
        storage["kind"] = "sourceTimeMergeStorage".into();
        for (name, role, recipe) in [
            (
                "indexes",
                ResourceRole::IndexBackend,
                json!({"kind":"rocksdbIndexes","path":directory.path().join("indexes")}),
            ),
            ("storage", ResourceRole::IndexBackend, storage),
        ] {
            let id = ResourceId::try_new(name)?;
            definition.resources.push(ResourceSpecification {
                id: id.clone(),
                role,
                ownership: ResourceOwnership::Graph,
                binding: name.into(),
            });
            definition.resource_configurations.insert(id, recipe);
        }
        definition.resource_dependencies.insert(
            ResourceId::try_new("storage")?,
            BTreeMap::from([(ResourceId::try_new("indexes")?, ResourceRole::IndexBackend)]),
        );
        for (from, out, to) in [
            ("source", "out", "merge"),
            ("merge", "out", "main"),
            ("merge", "late", "late"),
        ] {
            let pipe = if from == "source" {
                DesiredPipe::Bounded { capacity: 8 }
            } else {
                let resource = ResourceId::try_new(format!("{to}-journal"))?;
                let channel = QosChannelDefinition {
                    stream: StreamId::try_new(format!("merge/{out}"))?,
                    capacity: NonZeroUsize::new(8).context("capacity")?,
                    durable: true,
                    retention: RetentionPolicy::Backpressure,
                    subscribers: BTreeMap::from([(to.into(), SubscriptionStart::Earliest)]),
                };
                definition.resources.push(ResourceSpecification {
                    id: resource.clone(),
                    role: ResourceRole::StateStore,
                    ownership: ResourceOwnership::Graph,
                    binding: resource.as_str().into(),
                });
                definition.resource_configurations.insert(resource.clone(), json!({
                    "kind":"qos","path":directory.path().join("journals"),"definition":channel,
                    "recovery":{"kind":"replay","failureScope":"processRestart","receiptCapacity":8}
                }));
                DesiredPipe::Qos(QosPipeConfig {
                    resource,
                    subscriber: to.into(),
                    definition: channel,
                    gap_policy: ReplayGapPolicy::Strict,
                    shared_storage: None,
                })
            };
            definition.relationships.push(DesiredRelationship {
                definition: EdgeDefinition::new(
                    Endpoint::new(ComponentId::try_new(from)?, PortId::try_new(out)?),
                    Endpoint::new(ComponentId::try_new(to)?, PortId::try_new("in")?),
                ),
                policy: RelationshipPolicy::default(),
                pipe,
            });
        }
        let mut config = ComputationConfig { definition };
        for (count, destination) in [(1, "main"), (2, "late")] {
            let core = DrasiLib::builder().with_id("server-merge").build().await?;
            let components = build_components(
                &config,
                &core,
                factories.clone(),
                Arc::new(TransactionalTransformerRegistry::standard(
                    core.middleware_registry(),
                )),
            )
            .await?;
            assert_eq!(
                core.add_components(components).await?.summary,
                OperationSummary::Completed
            );
            core.start().await?;
            let mut receiver = output.subscribe();
            let values = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let values = receiver.borrow_and_update().clone();
                    if values.len() >= count {
                        return Ok::<_, anyhow::Error>(values);
                    }
                    receiver.changed().await?;
                }
            })
            .await??;
            assert_eq!(values[count - 1].0, destination);
            assert_eq!(
                GraphProducerProgress::from_envelope(&values[count - 1].1)?
                    .context("producer")?
                    .identity()
                    .construction_scope(),
                "server-merge"
            );
            let exported =
                configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
                    .context("saved recipe")?;
            assert_eq!(
                exported.definition.relationships,
                config.definition.relationships
            );
            assert_eq!(
                exported.definition.resource_configurations,
                config.definition.resource_configurations
            );
            core.shutdown().await?;
            drop(core);
            config = exported;
            *next
                .lock()
                .map_err(|_| anyhow::anyhow!("source fixture poisoned"))? =
                Some(source_event(2, 50)?);
        }
        Ok(())
    })
    .await?
}
