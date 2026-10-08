use super::*;
use async_trait::async_trait;
use drasi_lib::management::{
    DesiredInstance, ManagementResourceResolver, RecoveryRetirementAuthorization,
};
use drasi_server::computation::{validate_definition, ServerManagementResources};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, Weak,
    },
};

fn resource(name: &str) -> ResourceId {
    ResourceId::try_new(name).expect("resource")
}

fn config(path: &Path) -> Result<ComputationConfig> {
    let mut definition = DesiredInstance::default().topology;
    for (name, role) in [
        ("a-journal", ResourceRole::StateStore),
        ("z-storage", ResourceRole::IndexBackend),
    ] {
        definition.resources.push(ResourceSpecification {
            id: resource(name),
            role,
            ownership: ResourceOwnership::Graph,
            binding: name.into(),
        });
    }
    let channel = QosChannelDefinition {
        stream: StreamId::try_new("query/out")?,
        capacity: NonZeroUsize::new(4).expect("capacity"),
        durable: true,
        retention: RetentionPolicy::Backpressure,
        subscribers: BTreeMap::from([("sink".into(), SubscriptionStart::Earliest)]),
    };
    definition.resource_configurations.insert(
        resource("z-storage"),
        json!({"kind":"sharedStorage","component":"query","path":path}),
    );
    definition.resource_configurations.insert(resource("a-journal"),
        json!({"kind":"sharedQos","definition":channel,
            "recovery":{"kind":"replay","failureScope":drasi_core::interface::FailureMode::ProcessRestart,"receiptCapacity":8}}));
    definition.resource_dependencies.insert(
        resource("a-journal"),
        BTreeMap::from([(resource("z-storage"), ResourceRole::IndexBackend)]),
    );
    Ok(ComputationConfig { definition })
}

struct ObservedResolver {
    inner: ServerManagementResources,
    journal: Mutex<Option<Weak<QosChannel>>>,
    storage: Mutex<Option<Weak<QueryIndexProviderResource>>>,
}

#[async_trait]
impl ManagementResourceResolver for ObservedResolver {
    fn validate_transition(
        &self,
        previous: &DesiredTopology,
        desired: &DesiredTopology,
    ) -> Result<()> {
        self.inner.validate_transition(previous, desired)
    }

    async fn resolve(
        &self,
        instance: &str,
        graph: &str,
        specification: &ResourceSpecification,
        configuration: &Value,
    ) -> Result<ResourceHandle> {
        self.resolve_with_dependencies(
            instance,
            graph,
            specification,
            configuration,
            &BTreeMap::new(),
        )
        .await
    }
    async fn resolve_with_dependencies(
        &self,
        instance: &str,
        graph: &str,
        specification: &ResourceSpecification,
        configuration: &Value,
        dependencies: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> Result<ResourceHandle> {
        let handle = self
            .inner
            .resolve_with_dependencies(instance, graph, specification, configuration, dependencies)
            .await?;
        if specification.id == resource("z-storage") {
            *self.storage.lock().expect("observations") =
                Some(Arc::downgrade(&handle.get::<QueryIndexProviderResource>()?));
        }
        if specification.id == resource("a-journal") {
            let channel = handle.get::<QosChannel>()?;
            let mut resources = dependencies.clone();
            resources.insert(specification.id.clone(), handle.clone());
            let mut pipe = channel.definition().pipe(specification.id.clone(), "sink");
            if dependencies.contains_key(&resource("z-storage")) {
                pipe = pipe.with_shared_storage(resource("z-storage"));
            }
            pipe.validate_resources(&resources)?;
            *self.journal.lock().expect("observations") = Some(Arc::downgrade(&channel));
        }
        Ok(handle)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn shared_recipes_restore_graph_owned_group_and_journal_from_accepted_configuration(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let configuration = config(&directory.path().join("processing"))?;
    let yaml: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&configuration)?)?;
    assert_eq!(yaml.definition, configuration.definition);
    validate_definition(&yaml)?;
    let mut identity = None;
    for restart in [false, true] {
        let resolver = Arc::new(ObservedResolver {
            inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
            journal: Mutex::new(None),
            storage: Mutex::new(None),
        });
        let core = DrasiLib::builder()
            .with_id("shared")
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [27; 32],
                )?,
            ))
            .with_management_resources(resolver.clone())
            .build()
            .await?;
        if !restart {
            core.apply_desired_state(
                0,
                "shared-group",
                DesiredInstance::from(configuration.definition.clone()),
            )
            .await?;
        }
        assert!(core.reconcile_desired_state().await?.converged());
        let journal = resolver
            .journal
            .lock()
            .expect("observations")
            .as_ref()
            .and_then(Weak::upgrade)
            .context("constructed journal")?;
        assert!(journal.output_journal_identity().is_some());
        if restart {
            assert_eq!(journal.output_journal_identity(), identity);
            assert_eq!(
                core.desired_configuration()?
                    .desired
                    .topology
                    .resource_dependencies,
                configuration.definition.resource_dependencies
            );
        } else {
            identity = journal.output_journal_identity();
        }
        core.shutdown().await?;
        assert!(journal.progress().await.is_err());
    }
    let mut missing = configuration.clone();
    missing.definition.resource_dependencies.clear();
    assert!(validate_definition(&missing).is_err());
    let mut substituted = configuration.clone();
    // A same-named independent index resource cannot pass actual group checks.
    substituted
        .definition
        .resource_configurations
        .insert(resource("z-storage"), json!({"kind":"memoryIndexes"}));
    validate_definition(&substituted)?;
    let resolver = Arc::new(ServerManagementResources::new(
        &PluginRegistry::new(),
        None,
    )?);
    let core = DrasiLib::builder()
        .with_management_resources(resolver)
        .build()
        .await?;
    core.apply_desired_state(
        0,
        "not-a-group",
        DesiredInstance::from(substituted.definition),
    )
    .await?;
    let status = core.reconcile_desired_state().await?;
    assert!(!status.converged());
    assert!(status.resource_errors.contains_key(&resource("a-journal")));
    core.shutdown().await?;
    Ok(())
}

struct Source {
    descriptor: ComponentDescriptor,
    event: Option<ChangeEnvelope>,
}
#[async_trait]
impl ComputationComponent for Source {
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
impl EnvelopeSource for Source {
    async fn next(&mut self) -> Result<Option<OutputEnvelope>> {
        match self.event.take() {
            Some(event) => Ok(Some(OutputEnvelope {
                port: PortId::try_new("out")?,
                envelope: event,
            })),
            None => std::future::pending().await,
        }
    }
}

struct Sink {
    descriptor: ComponentDescriptor,
    received: tokio::sync::mpsc::Sender<ChangeEnvelope>,
    block: bool,
}
#[async_trait]
impl ComputationComponent for Sink {
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
impl EnvelopeSink for Sink {
    fn completion(&self) -> SinkCompletion {
        SinkCompletion::Handled
    }
    async fn handle(&mut self, input: InputEnvelope) -> Result<()> {
        self.received.send(input.envelope.clone()).await?;
        if self.block {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
}

struct EndpointFactory {
    descriptor: FactoryDescriptor,
    received: tokio::sync::mpsc::Sender<ChangeEnvelope>,
    emit: Arc<AtomicBool>,
    block: bool,
}
#[async_trait]
impl ComponentFactory for EndpointFactory {
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
        if self.descriptor.role == ComponentRole::Source {
            let event = GraphChangeCodec::encode_change(
                SourceChange::Insert {
                    element: Element::Node {
                        metadata: drasi_core::models::ElementMetadata {
                            reference: drasi_core::models::ElementReference::new("source", "one"),
                            labels: Arc::from([Arc::from("Item")]),
                            effective_from: 1,
                        },
                        properties: Default::default(),
                    },
                },
                StreamId::try_new("source/out").expect("stream"),
                1,
                None,
            )
            .map_err(ComponentCreationError::terminal)?;
            Ok(ConstructedComponent::source(Box::new(Source {
                descriptor: context.specification.descriptor.clone(),
                event: self.emit.swap(false, Ordering::AcqRel).then_some(event),
            })))
        } else {
            Ok(ConstructedComponent::sink(Box::new(Sink {
                descriptor: context.specification.descriptor.clone(),
                received: self.received.clone(),
                block: self.block,
            })))
        }
    }
}

fn pipeline(
    path: &Path,
    send: &tokio::sync::mpsc::Sender<ChangeEnvelope>,
    emit: Arc<AtomicBool>,
    block: bool,
) -> Result<(ComputationConfig, FactoryRegistry)> {
    let mut config = config(path)?;
    let mut factories = FactoryRegistry::standard();
    for (name, role, schema, direction, port) in [
        (
            "source",
            ComponentRole::Source,
            GraphChangeCodec::schema(),
            PortDirection::Output,
            "out",
        ),
        (
            "sink",
            ComponentRole::Sink,
            QueryChangeCodec::schema(),
            PortDirection::Input,
            "in",
        ),
    ] {
        let implementation = ImplementationIdentity::try_new(format!("test/{name}"), "1")?;
        factories.register(Arc::new(EndpointFactory {
            descriptor: FactoryDescriptor {
                implementation: implementation.clone(),
                role,
                configuration_version: 1,
                configuration: ConfigurationSchema::default(),
                dependencies: BTreeMap::new(),
            },
            received: send.clone(),
            emit: emit.clone(),
            block,
        }))?;
        let descriptor = ComponentDescriptor::try_new(
            ComponentId::try_new(name)?,
            vec![PortDescriptor::new(
                PortId::try_new(port)?,
                direction,
                schema.descriptor().clone(),
                PipeRequirements::default(),
            )],
        )?;
        let completion = (role == ComponentRole::Sink).then_some(SinkCompletion::Handled);
        config.definition.components.push(DesiredComponent {
            descriptor: descriptor.clone(),
            role,
            completion,
            streams: if role == ComponentRole::Source {
                BTreeMap::from([(PortId::try_new("out")?, StreamId::try_new("source/out")?)])
            } else {
                BTreeMap::new()
            },
            lifecycle: LifecyclePolicy::default(),
            input_merge: InputMergePolicy::default(),
            construction: ComponentConstruction::Factory(ComponentSpecification {
                descriptor,
                role,
                completion,
                implementation,
                configuration_version: 1,
                configuration: BTreeMap::new(),
                dependencies: BTreeMap::new(),
            }),
        });
    }
    let query = ContinuousQueryDefinition {
        graph_id: config.definition.graph_id.clone(),
        id: ComponentId::try_new("query")?,
        query: "MATCH (n:Item) RETURN 7 AS value".into(),
        language: ComputationQueryLanguage::Cypher,
        output_stream: StreamId::try_new("query/out")?,
        outbox_capacity: NonZeroUsize::new(4).expect("capacity"),
    };
    let descriptor = query.descriptor();
    config.definition.components.push(DesiredComponent {
        descriptor: descriptor.clone(),
        role: ComponentRole::Query,
        completion: None,
        streams: BTreeMap::from([(PortId::try_new("out")?, query.output_stream.clone())]),
        lifecycle: LifecyclePolicy::default(),
        input_merge: InputMergePolicy::default(),
        construction: ComponentConstruction::Factory(ComponentSpecification {
            descriptor,
            role: ComponentRole::Query,
            completion: None,
            implementation: ContinuousQueryFactory::default()
                .descriptor()
                .implementation
                .clone(),
            configuration_version: 1,
            configuration: BTreeMap::from([
                (
                    Arc::from("query"),
                    ConfigurationValue::Literal(json!(query.query)),
                ),
                (
                    Arc::from("stream"),
                    ConfigurationValue::Literal(json!(query.output_stream)),
                ),
                (
                    Arc::from("outbox_capacity"),
                    ConfigurationValue::Literal(json!(4)),
                ),
            ]),
            dependencies: BTreeMap::from([(Arc::from("indexes"), vec![resource("z-storage")])]),
        }),
    });
    let journal: QosChannelDefinition = serde_json::from_value(
        config.definition.resource_configurations[&resource("a-journal")]["definition"].clone(),
    )?;
    for (from, to, pipe) in [
        ("source", "query", DesiredPipe::Bounded { capacity: 4 }),
        (
            "query",
            "sink",
            DesiredPipe::Qos(
                journal
                    .pipe(resource("a-journal"), "sink")
                    .with_shared_storage(resource("z-storage")),
            ),
        ),
    ] {
        config.definition.relationships.push(DesiredRelationship {
            definition: EdgeDefinition {
                from: Endpoint::new(ComponentId::try_new(from)?, PortId::try_new("out")?),
                to: Endpoint::new(ComponentId::try_new(to)?, PortId::try_new("in")?),
            },
            policy: RelationshipPolicy::default(),
            pipe,
        });
    }
    Ok((config, factories))
}

#[tokio::test(flavor = "current_thread")]
async fn imperative_server_shared_configuration_executes_actual_query_and_pipe() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let (config, factories) = pipeline(
        directory.path(),
        &send,
        Arc::new(AtomicBool::new(true)),
        false,
    )?;
    let core = DrasiLib::builder()
        .with_id("shared-pipeline")
        .build()
        .await?;
    let transactional = Arc::new(TransactionalTransformerRegistry::standard(
        core.middleware_registry(),
    ));
    let batch = build_components(&config, &core, factories, transactional).await?;
    assert!(
        batch.bindings.resources.is_empty(),
        "storage must be acquired by the graph"
    );
    assert_eq!(batch.bindings.resource_constructors.len(), 2);
    core.add_components(batch).await?;
    core.start().await?;
    let result = tokio::time::timeout(DEADLINE, receive.recv())
        .await?
        .context("query output")?;
    let ChangeOperation::Added { after, .. } = &result.changes().operations()[0] else {
        anyhow::bail!("expected added row");
    };
    let row = QueryChangeCodec::decode_row(after)?;
    assert_eq!(
        row.values.get("value"),
        Some(&drasi_core::evaluation::variable_value::VariableValue::Integer(7.into()))
    );
    let identity = QueryRecoveryIdentity::from_envelope(&result)?;
    assert_eq!(identity.construction_scope(), "shared-pipeline");
    assert_eq!(identity.graph_id(), config.definition.graph_id);
    assert_eq!(identity.query_id(), "query");
    assert!(identity.incarnation().is_none());
    core.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn managed_server_shared_pipeline_replays_unhandled_output_after_reconstruction() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let emit = Arc::new(AtomicBool::new(true));
    let mut original = None;
    for restart in [false, true] {
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let (config, factories) = pipeline(
            &directory.path().join("processing"),
            &send,
            emit.clone(),
            !restart,
        )?;
        validate_definition(&config)?;
        let resolver = Arc::new(ObservedResolver {
            inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
            journal: Mutex::new(None),
            storage: Mutex::new(None),
        });
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("shared-recovery")
                .with_component_factories(factories)
                .with_configuration_store(Arc::new(
                    drasi_state_store_redb::RedbConfigurationStore::new(
                        directory.path().join("configuration.redb"),
                        [29; 32],
                    )?,
                ))
                .with_management_resources(resolver.clone())
                .build()
                .await?,
        );
        if !restart {
            core.apply_desired_state(0, "pipeline", DesiredInstance::from(config.definition))
                .await?;
        }
        assert!(core.reconcile_desired_state().await?.converged());
        core.start().await?;
        let result = tokio::time::timeout(DEADLINE, receive.recv())
            .await?
            .context("retained query output")?;
        let journal = resolver
            .journal
            .lock()
            .expect("observations")
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        let identity = QueryRecoveryIdentity::from_envelope(&result)?;
        assert_eq!(identity.construction_scope(), "shared-recovery");
        assert!(identity.incarnation().is_none());
        let changes = (
            result.changes().id().clone(),
            result.changes().schema().clone(),
            result.changes().operations().to_vec(),
        );
        if let Some((prior_changes, prior_identity, journal_identity)) = &original {
            assert_eq!(&changes, prior_changes);
            assert_eq!(&identity, prior_identity);
            assert_eq!(journal.output_journal_identity(), *journal_identity);
        } else {
            let progress = journal.progress().await?;
            assert_eq!(progress.accepted, 1);
            assert_eq!(progress.processed["sink"], 0);
            original = Some((changes, identity, journal.output_journal_identity()));
        }
        if restart {
            tokio::time::timeout(DEADLINE, async {
                loop {
                    if journal.progress().await?.processed["sink"] == 1 {
                        break Ok::<_, anyhow::Error>(());
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await??;
            assert_eq!(
                journal.progress().await?.accepted,
                1,
                "recovery must not append another output"
            );
            assert!(receive.try_recv().is_err());
        } else {
            assert_unsafe_changes_rejected(&core, directory.path()).await?;
            core.stop().await?;
            assert_unsafe_changes_rejected(&core, directory.path()).await?;
        }
        core.shutdown().await?;
    }
    assert!(!emit.load(Ordering::Acquire));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn managed_shared_drained_changes_reconstruct_actual_owners_and_restore_accepted_removal(
) -> Result<()> {
    managed_drained_changes(true).await
}

#[tokio::test(flavor = "current_thread")]
async fn managed_standalone_query_drained_changes_reconstruct_actual_owners_and_restore_accepted_removal(
) -> Result<()> {
    managed_drained_changes(false).await
}

fn separate_storage(config: &mut ComputationConfig, path: &Path) -> Result<()> {
    config.definition.resource_configurations.insert(
        resource("z-storage"),
        json!({"kind":"rocksdbIndexes","path":path.join("processing")}),
    );
    let journal = config
        .definition
        .resource_configurations
        .get_mut(&resource("a-journal"))
        .context("journal recipe")?;
    journal["kind"] = json!("qos");
    journal["path"] = json!(path.join("journal"));
    config
        .definition
        .resource_dependencies
        .remove(&resource("a-journal"));
    for edge in &mut config.definition.relationships {
        if let DesiredPipe::Qos(pipe) = &mut edge.pipe {
            pipe.shared_storage = None;
        }
    }
    validate_definition(config)?;
    Ok(())
}

async fn quiesce_and_stop(core: &DrasiLib) -> Result<()> {
    let control = core.computation_control()?;
    control
        .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
        .await?;
    core.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn loss_authorized_shared_query_retirement_preserves_pending_output() -> Result<()> {
    loss_authorized_query_retirement(true).await
}

#[tokio::test(flavor = "current_thread")]
async fn loss_authorized_standalone_query_retirement_preserves_pending_output() -> Result<()> {
    loss_authorized_query_retirement(false).await
}

async fn loss_authorized_query_retirement(shared: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let emit = Arc::new(AtomicBool::new(true));
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let (mut config, factories) = pipeline(
        &directory.path().join("processing"),
        &send,
        emit.clone(),
        true,
    )?;
    if !shared {
        separate_storage(&mut config, directory.path())?;
    }
    let resolver = Arc::new(ObservedResolver {
        inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
        journal: Mutex::new(None),
        storage: Mutex::new(None),
    });
    let store = Arc::new(drasi_state_store_redb::RedbConfigurationStore::new(
        directory.path().join("configuration.redb"),
        [67; 32],
    )?);
    let core = DrasiLib::builder()
        .with_id("retired-query")
        .with_component_factories(factories)
        .with_configuration_store(store.clone())
        .with_management_resources(resolver.clone())
        .build()
        .await?;
    let original = DesiredInstance::from(config.definition.clone());
    let created = core
        .apply_desired_state(0, "create", original.clone())
        .await?;
    core.start().await?;
    let output = tokio::time::timeout(DEADLINE, receive.recv())
        .await?
        .context("pending output")?;
    let journal = resolver
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .context("journal")?;
    let identity = journal.output_journal_identity();
    assert_eq!(journal.progress().await?.processed["sink"], 0);
    let mut removed = DesiredInstance {
        retirement: Some(RecoveryRetirementAuthorization {
            from_revision: created.revision,
            allow_data_loss: true,
            resources: original
                .topology
                .resources
                .iter()
                .map(|spec| spec.id.clone())
                .collect(),
            components: original
                .topology
                .components
                .iter()
                .map(|node| node.descriptor.id().clone())
                .collect(),
        }),
        ..Default::default()
    };
    assert!(core
        .apply_desired_state(created.revision, "running", removed.clone())
        .await
        .is_err());
    let control = core.computation_control()?;
    control
        .quiesce_components(
            control.desired_snapshot().revision,
            GraphSelection::Exact(vec![
                ComponentId::try_new("source")?,
                ComponentId::try_new("query")?,
            ]),
        )
        .await?;
    core.stop().await?;
    assert!(core
        .apply_desired_state(created.revision, "lossless", DesiredInstance::default())
        .await
        .is_err());
    let permission = removed.retirement.take().unwrap();
    for missing_resource in [true, false] {
        let mut incomplete = permission.clone();
        if missing_resource {
            incomplete.resources.remove(&resource("a-journal"));
        } else {
            incomplete.components.remove(&ComponentId::try_new("sink")?);
        }
        removed.retirement = Some(incomplete);
        assert!(core
            .apply_desired_state(created.revision, "incomplete", removed.clone())
            .await
            .is_err());
        assert!(core.configuration_receipt("incomplete").await?.is_none());
        assert_eq!(journal.progress().await?.processed["sink"], 0);
    }
    removed.retirement = Some(permission);
    let retired = core
        .apply_desired_state(created.revision, "retire", removed.clone())
        .await?;
    assert!(core.reconcile_desired_state().await?.converged());
    assert!(journal.progress().await.is_err());
    core.shutdown().await?;

    let (_, factories) = pipeline(
        &directory.path().join("processing"),
        &send,
        emit.clone(),
        false,
    )?;
    let restored = DrasiLib::builder()
        .with_id("retired-query")
        .with_component_factories(factories)
        .with_configuration_store(store)
        .with_management_resources(resolver.clone())
        .build()
        .await?;
    assert_eq!(restored.desired_configuration()?.desired, removed);
    assert_eq!(
        restored.configuration_receipt("retire").await?,
        Some(retired.clone())
    );
    assert!(restored.reconcile_desired_state().await?.converged());
    assert!(resolver
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .is_some_and(|old| Arc::ptr_eq(&old, &journal)));
    restored
        .apply_desired_state(retired.revision, "restore-intact-data", original)
        .await?;
    assert!(restored.reconcile_desired_state().await?.converged());
    let reopened = resolver
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .context("reopened journal")?;
    assert_eq!(reopened.output_journal_identity(), identity);
    assert_eq!(reopened.progress().await?.accepted, 1);
    assert_eq!(reopened.progress().await?.processed["sink"], 0);
    restored.start().await?;
    let replay = tokio::time::timeout(DEADLINE, receive.recv())
        .await?
        .context("retirement erased pending output")?;
    assert_eq!(replay.changes().id(), output.changes().id());
    assert_eq!(replay.changes().operations(), output.changes().operations());
    assert_eq!(
        QueryRecoveryIdentity::from_envelope(&replay)?,
        QueryRecoveryIdentity::from_envelope(&output)?
    );
    tokio::time::timeout(DEADLINE, async {
        while reopened.progress().await?.processed["sink"] != 1 {
            tokio::task::yield_now().await;
        }
        anyhow::Ok(())
    })
    .await??;
    quiesce_and_stop(&restored).await?;
    assert_eq!(reopened.progress().await?.accepted, 1);
    assert!(receive.try_recv().is_err());
    assert!(!emit.load(Ordering::Acquire));
    restored.shutdown().await?;
    Ok(())
}

async fn managed_drained_changes(shared: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let (mut config, factories) = pipeline(
        &directory.path().join("processing"),
        &send,
        Arc::new(AtomicBool::new(true)),
        false,
    )?;
    if !shared {
        separate_storage(&mut config, directory.path())?;
    }
    let resolver = Arc::new(ObservedResolver {
        inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
        journal: Mutex::new(None),
        storage: Mutex::new(None),
    });
    let core = DrasiLib::builder()
        .with_id("drained")
        .with_component_factories(factories)
        .with_configuration_store(Arc::new(
            drasi_state_store_redb::RedbConfigurationStore::new(
                directory.path().join("configuration.redb"),
                [31; 32],
            )?,
        ))
        .with_management_resources(resolver.clone())
        .build()
        .await?;
    core.apply_desired_state(0, "pipeline", DesiredInstance::from(config.definition))
        .await?;
    assert!(core.reconcile_desired_state().await?.converged());
    core.start().await?;
    tokio::time::timeout(DEADLINE, receive.recv())
        .await?
        .context("query output")?;
    let old = resolver
        .journal
        .lock()
        .expect("observations")
        .as_ref()
        .and_then(Weak::upgrade)
        .context("journal")?;
    tokio::time::timeout(DEADLINE, async {
        while old.progress().await?.processed["sink"] != 1 {
            tokio::task::yield_now().await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let identity = old.output_journal_identity();
    let original = core.desired_configuration()?;
    let mut changed = original.desired.clone();
    changed
        .topology
        .resources
        .iter_mut()
        .find(|resource| resource.id.as_str() == "z-storage")
        .context("storage declaration")?
        .binding = "replacement-owner".into();
    assert!(
        core.apply_desired_state(original.revision, "rebind", changed.clone())
            .await
            .is_err(),
        "even a currently empty running path must stop before proof"
    );
    assert!(core.configuration_receipt("rebind").await?.is_none());
    quiesce_and_stop(&core).await?;
    let receipt = core
        .apply_desired_state(original.revision, "rebind", changed.clone())
        .await?;
    assert_eq!(receipt.revision, original.revision + 1);
    let status = core.reconcile_desired_state().await?;
    assert!(status.converged(), "{status:?}");
    assert!(
        old.progress().await.is_err(),
        "accepted retirement fences the old owner"
    );
    let current = resolver
        .journal
        .lock()
        .expect("observations")
        .as_ref()
        .and_then(Weak::upgrade)
        .context("replacement journal")?;
    assert!(!Arc::ptr_eq(&old, &current));
    assert_eq!(current.output_journal_identity(), identity);
    assert_eq!(current.progress().await?.accepted, 1);
    assert_eq!(current.progress().await?.processed["sink"], 1);
    core.start().await?;
    quiesce_and_stop(&core).await?;
    changed
        .topology
        .resource_configurations
        .get_mut(&resource("z-storage"))
        .context("storage recipe")?["path"] = json!(directory.path().join("replacement"));
    let receipt = core
        .apply_desired_state(receipt.revision, "redirect", changed)
        .await?;
    let status = core.reconcile_desired_state().await?;
    assert!(status.converged(), "{status:?}");
    core.start().await?;
    quiesce_and_stop(&core).await?;
    let removed = core
        .apply_desired_state(receipt.revision, "remove", DesiredInstance::default())
        .await?;
    let status = core.reconcile_desired_state().await?;
    assert!(status.converged(), "{status:?}");
    core.shutdown().await?;
    let restored = DrasiLib::builder()
        .with_id("drained")
        .with_configuration_store(Arc::new(
            drasi_state_store_redb::RedbConfigurationStore::new(
                directory.path().join("configuration.redb"),
                [31; 32],
            )?,
        ))
        .build()
        .await?;
    assert_eq!(restored.desired_configuration()?.revision, removed.revision);
    assert_eq!(
        restored.desired_configuration()?.desired,
        DesiredInstance::default()
    );
    assert_eq!(
        restored.configuration_receipt("remove").await?,
        Some(removed)
    );
    assert!(restored.reconcile_desired_state().await?.converged());
    restored.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn standalone_query_retirement_preserves_unknown_commit_through_reconcile_or_shutdown(
) -> Result<()> {
    use super::managed_faults::{ConfirmationGate, GatedStore};
    use tokio::sync::{Notify, Semaphore};

    for (reject, close) in [(false, false), (true, false), (false, true), (true, true)] {
        let directory = tempfile::tempdir()?;
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let (mut config, factories) = pipeline(
            &directory.path().join("processing"),
            &send,
            Arc::new(AtomicBool::new(true)),
            false,
        )?;
        separate_storage(&mut config, directory.path())?;
        let gate = Arc::new(ConfirmationGate {
            armed: AtomicBool::new(false),
            loads_blocked: AtomicBool::new(false),
            committed: Notify::new(),
            release: Semaphore::new(0),
            fail: true,
            reject,
        });
        let resolver = Arc::new(ObservedResolver {
            inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
            journal: Mutex::new(None),
            storage: Mutex::new(None),
        });
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("standalone-query")
                .with_component_factories(factories.clone())
                .with_management_resources(resolver.clone())
                .with_configuration_store(Arc::new(GatedStore {
                    inner: drasi_state_store_redb::RedbConfigurationStore::new(
                        directory.path().join("configuration.redb"),
                        [47; 32],
                    )?,
                    gate: gate.clone(),
                }))
                .build()
                .await?,
        );
        core.apply_desired_state(0, "create", DesiredInstance::from(config.definition))
            .await?;
        core.start().await?;
        tokio::time::timeout(DEADLINE, receive.recv())
            .await?
            .context("query output")?;
        let old = resolver
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        tokio::time::timeout(DEADLINE, async {
            while old.progress().await?.processed["sink"] != 1 {
                tokio::task::yield_now().await;
            }
            anyhow::Ok(())
        })
        .await??;
        quiesce_and_stop(&core).await?;
        let original = core.desired_configuration()?;
        let mut desired = original.desired.clone();
        desired
            .topology
            .resources
            .iter_mut()
            .find(|spec| spec.id == resource("z-storage"))
            .context("storage")?
            .binding = "replacement".into();
        gate.armed.store(true, Ordering::SeqCst);
        let next = desired.clone();
        let caller_core = core.clone();
        let mut caller = tokio::spawn(async move {
            caller_core
                .apply_desired_state(original.revision, "transition", next)
                .await
        });
        tokio::select! {
            result = &mut caller => anyhow::bail!("retirement refused before commit (reject={reject}, close={close}): {result:?}"),
            result = tokio::time::timeout(DEADLINE, gate.committed.notified()) => result?,
        }
        gate.release.add_permits(1);
        assert!(caller.await?.is_err());
        assert!(core.desired_configuration().is_err());
        let control = core.computation_control()?;
        assert!(matches!(
            control
                .start_components(control.desired_snapshot().revision, GraphSelection::All)
                .await,
            Err(GraphError::OperationInProgress)
        ));
        let expected = if reject { original.desired } else { desired };
        if close {
            tokio::time::timeout(DEADLINE, core.shutdown()).await??;
            let restored = DrasiLib::builder()
                .with_id("standalone-query")
                .with_component_factories(factories)
                .with_management_resources(resolver.clone())
                .with_configuration_store(Arc::new(
                    drasi_state_store_redb::RedbConfigurationStore::new(
                        directory.path().join("configuration.redb"),
                        [47; 32],
                    )?,
                ))
                .build()
                .await?;
            assert_eq!(restored.desired_configuration()?.desired, expected);
            assert_eq!(
                restored
                    .configuration_receipt("transition")
                    .await?
                    .is_some(),
                !reject
            );
            restored.start().await?;
            assert!(restored.reconcile_desired_state().await?.converged());
            assert!(receive.try_recv().is_err());
            restored.shutdown().await?;
        } else {
            gate.loads_blocked.store(false, Ordering::SeqCst);
            assert!(core.reconcile_desired_state().await?.converged());
            assert_eq!(core.desired_configuration()?.desired, expected);
            assert_eq!(
                core.configuration_receipt("transition").await?.is_some(),
                !reject
            );
            assert_eq!(old.progress().await.is_ok(), reject);
            core.start().await?;
            assert!(receive.try_recv().is_err());
            core.shutdown().await?;
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn shared_retirement_holds_restart_and_storage_until_authoritative_commit_resolution(
) -> Result<()> {
    use super::managed_faults::{ConfirmationGate, GatedStore};
    use tokio::sync::{Notify, Semaphore};

    for (reject, uncertain, close_unconfirmed) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, true, true),
        (true, true, true),
    ] {
        let directory = tempfile::tempdir()?;
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let (config, factories) = pipeline(
            &directory.path().join("processing"),
            &send,
            Arc::new(AtomicBool::new(true)),
            false,
        )?;
        let gate = Arc::new(ConfirmationGate {
            armed: AtomicBool::new(false),
            loads_blocked: AtomicBool::new(false),
            committed: Notify::new(),
            release: Semaphore::new(0),
            fail: uncertain,
            reject,
        });
        let resolver = Arc::new(ObservedResolver {
            inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
            journal: Mutex::new(None),
            storage: Mutex::new(None),
        });
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("retirement-faults")
                .with_component_factories(factories)
                .with_configuration_store(Arc::new(GatedStore {
                    inner: drasi_state_store_redb::RedbConfigurationStore::new(
                        directory.path().join("configuration.redb"),
                        [37; 32],
                    )?,
                    gate: gate.clone(),
                }))
                .with_management_resources(resolver.clone())
                .build()
                .await?,
        );
        core.apply_desired_state(0, "pipeline", DesiredInstance::from(config.definition))
            .await?;
        assert!(core.reconcile_desired_state().await?.converged());
        core.start().await?;
        tokio::time::timeout(DEADLINE, receive.recv())
            .await?
            .context("query output")?;
        let journal = resolver
            .journal
            .lock()
            .expect("observations")
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        tokio::time::timeout(DEADLINE, async {
            while journal.progress().await?.processed["sink"] != 1 {
                tokio::task::yield_now().await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        core.stop().await?;
        let original = core.desired_configuration()?;
        let mut desired = original.desired.clone();
        desired
            .topology
            .resources
            .iter_mut()
            .find(|resource| resource.id.as_str() == "z-storage")
            .context("storage declaration")?
            .binding = "replacement-owner".into();
        let provider = resolver
            .storage
            .lock()
            .expect("observations")
            .as_ref()
            .and_then(Weak::upgrade)
            .context("group")?;
        gate.armed.store(true, Ordering::SeqCst);
        let caller_core = core.clone();
        let next = desired.clone();
        let caller = tokio::spawn(async move {
            caller_core
                .apply_desired_state(original.revision, "transition", next)
                .await
        });
        tokio::time::timeout(DEADLINE, gate.committed.notified()).await?;
        let control = core.computation_control()?;
        assert!(matches!(
            control
                .start_components(control.desired_snapshot().revision, GraphSelection::All)
                .await,
            Err(GraphError::OperationInProgress)
        ));
        let probe = provider
            .0
            .transaction_group()
            .context("actual group")?
            .journal_transaction("retirement-probe")?;
        let executed = AtomicBool::new(false);
        let mut writer = Box::pin(probe.run(async {
            executed.store(true, Ordering::SeqCst);
            Ok(())
        }));
        tokio::select! {
            biased;
            result = &mut writer => panic!("write entered the frozen owner: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        assert!(!executed.load(Ordering::SeqCst));
        if !reject && !uncertain {
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
            gate.release.add_permits(1);
        } else {
            gate.release.add_permits(1);
            assert!(caller.await?.is_err());
        }
        if uncertain {
            assert!(core.desired_configuration().is_err());
            assert!(matches!(
                control
                    .start_components(control.desired_snapshot().revision, GraphSelection::All)
                    .await,
                Err(GraphError::OperationInProgress)
            ));
            tokio::select! {
                biased;
                result = &mut writer => panic!("uncertain commit released storage: {result:?}"),
                _ = tokio::task::yield_now() => {}
            }
            if close_unconfirmed {
                drop(writer);
                probe.shutdown().await?;
                drop(probe);
                drop(provider);
                tokio::time::timeout(DEADLINE, core.shutdown()).await??;
                let (_, factories) = pipeline(
                    &directory.path().join("processing"),
                    &send,
                    Arc::new(AtomicBool::new(false)),
                    false,
                )?;
                let restored = DrasiLib::builder()
                    .with_id("retirement-faults")
                    .with_component_factories(factories)
                    .with_configuration_store(Arc::new(
                        drasi_state_store_redb::RedbConfigurationStore::new(
                            directory.path().join("configuration.redb"),
                            [37; 32],
                        )?,
                    ))
                    .with_management_resources(Arc::new(ServerManagementResources::new(
                        &PluginRegistry::new(),
                        None,
                    )?))
                    .build()
                    .await?;
                assert_eq!(
                    restored.desired_configuration()?.desired,
                    if reject { original.desired } else { desired }
                );
                assert_eq!(
                    restored
                        .configuration_receipt("transition")
                        .await?
                        .is_some(),
                    !reject
                );
                restored.start().await?;
                assert!(receive.try_recv().is_err());
                restored.shutdown().await?;
                continue;
            }
            gate.loads_blocked.store(false, Ordering::SeqCst);
        }
        let (status, write) = tokio::join!(core.reconcile_desired_state(), async {
            let result = tokio::time::timeout(DEADLINE, writer).await?;
            probe.shutdown().await?;
            Ok::<_, anyhow::Error>(result)
        });
        status?;
        assert_eq!(write?.is_ok(), reject);
        assert_eq!(executed.load(Ordering::SeqCst), reject);
        drop(probe);
        drop(provider);
        let status = core.reconcile_desired_state().await?;
        assert!(
            status.converged(),
            "reject={reject}, uncertain={uncertain}: {status:?}"
        );
        let current = core.desired_configuration()?;
        let realized = control.desired_snapshot();
        for resource in &current.desired.topology.resources {
            assert_eq!(
                realized.resources.get(&resource.id),
                Some(resource),
                "retained resource declaration: reject={reject}, uncertain={uncertain}"
            );
            assert_eq!(
                realized.resource_configurations.get(&resource.id),
                current
                    .desired
                    .topology
                    .resource_configurations
                    .get(&resource.id),
                "retained resource recipe: reject={reject}, uncertain={uncertain}"
            );
        }
        assert_eq!(
            current.desired,
            if reject { original.desired } else { desired }
        );
        assert_eq!(current.revision, original.revision + u64::from(!reject));
        assert_eq!(
            core.configuration_receipt("transition").await?.is_some(),
            !reject
        );
        assert_eq!(journal.progress().await.is_ok(), reject);
        core.start().await?;
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "required child of shared_server_resources_recover_after_process_exit"]
async fn shared_server_crash_worker() -> Result<()> {
    use super::managed_faults::{ConfirmationGate, GatedStore};
    use tokio::sync::{Notify, Semaphore};

    let directory = PathBuf::from(std::env::var("DRASI_SHARED_CRASH_DIRECTORY")?);
    let phase = std::env::var("DRASI_SHARED_CRASH_PHASE")?;
    let pending = phase == "pending-output";
    anyhow::ensure!(
        pending || phase == "before-commit" || phase == "after-commit",
        "unknown crash phase"
    );
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    let (config, factories) = pipeline(
        &directory.join("processing"),
        &send,
        Arc::new(AtomicBool::new(true)),
        pending,
    )?;
    let gate = Arc::new(ConfirmationGate {
        armed: AtomicBool::new(false),
        loads_blocked: AtomicBool::new(false),
        committed: Notify::new(),
        release: Semaphore::new(0),
        fail: false,
        reject: phase == "before-commit",
    });
    let resolver = Arc::new(ObservedResolver {
        inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
        journal: Mutex::new(None),
        storage: Mutex::new(None),
    });
    let core = Arc::new(
        DrasiLib::builder()
            .with_id("server-crash")
            .with_component_factories(factories)
            .with_configuration_store(Arc::new(GatedStore {
                inner: drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.join("configuration.redb"),
                    [43; 32],
                )?,
                gate: gate.clone(),
            }))
            .with_management_resources(resolver.clone())
            .build()
            .await?,
    );
    core.apply_desired_state(0, "pipeline", DesiredInstance::from(config.definition))
        .await?;
    assert!(core.reconcile_desired_state().await?.converged());
    core.start().await?;
    let output = tokio::time::timeout(DEADLINE, receive.recv())
        .await?
        .context("query output before crash")?;
    let journal = resolver
        .journal
        .lock()
        .expect("observations")
        .as_ref()
        .and_then(Weak::upgrade)
        .context("journal")?;
    std::fs::write(
        directory.join("output.json"),
        query_output_codec()?.encode(&output)?,
    )?;
    std::fs::write(
        directory.join("journal.json"),
        serde_json::to_vec(&journal.output_journal_identity())?,
    )?;
    if pending {
        assert_eq!(journal.progress().await?.accepted, 1);
        assert_eq!(journal.progress().await?.processed["sink"], 0);
        std::process::exit(81);
    }
    tokio::time::timeout(DEADLINE, async {
        while journal.progress().await?.processed["sink"] != 1 {
            tokio::task::yield_now().await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    core.stop().await?;
    let current = core.desired_configuration()?;
    let mut desired = current.desired;
    desired
        .topology
        .resources
        .iter_mut()
        .find(|resource| resource.id.as_str() == "z-storage")
        .context("storage")?
        .binding = "replacement-owner".into();
    gate.armed.store(true, Ordering::SeqCst);
    let caller = tokio::spawn(async move {
        core.apply_desired_state(current.revision, "transition", desired)
            .await
    });
    tokio::select! {
        result = caller => anyhow::bail!("transition unexpectedly returned: {result:?}"),
        result = tokio::time::timeout(DEADLINE, gate.committed.notified()) => result?,
    }
    std::process::exit(if phase == "before-commit" { 82 } else { 83 });
}

fn query_output_codec() -> Result<EnvelopeCodec> {
    let mut codec = EnvelopeCodec::new(NonZeroUsize::new(64 * 1024).expect("output bound"));
    codec.register_schema(QueryChangeCodec::schema())?;
    Ok(codec)
}

#[tokio::test(flavor = "current_thread")]
async fn shared_server_resources_recover_after_process_exit() -> Result<()> {
    for (phase, code) in [
        ("pending-output", 81),
        ("before-commit", 82),
        ("after-commit", 83),
    ] {
        let directory = tempfile::tempdir()?;
        let mut child = tokio::process::Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "shared_storage::shared_server_crash_worker",
                "--ignored",
            ])
            .env("DRASI_SHARED_CRASH_DIRECTORY", directory.path())
            .env("DRASI_SHARED_CRASH_PHASE", phase)
            .kill_on_drop(true);
        let output = tokio::time::timeout(DEADLINE * 3, child.output()).await??;
        assert_eq!(
            output.status.code(),
            Some(code),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let (config, factories) = pipeline(
            &directory.path().join("processing"),
            &send,
            Arc::new(AtomicBool::new(false)),
            false,
        )?;
        let mut expected = DesiredInstance::from(config.definition);
        if phase == "after-commit" {
            expected
                .topology
                .resources
                .iter_mut()
                .find(|resource| resource.id.as_str() == "z-storage")
                .context("storage")?
                .binding = "replacement-owner".into();
        }
        let resolver = Arc::new(ObservedResolver {
            inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
            journal: Mutex::new(None),
            storage: Mutex::new(None),
        });
        let core = DrasiLib::builder()
            .with_id("server-crash")
            .with_component_factories(factories)
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [43; 32],
                )?,
            ))
            .with_management_resources(resolver.clone())
            .build()
            .await?;
        assert_eq!(
            core.desired_configuration()?.desired,
            expected.normalized()?
        );
        assert_eq!(
            core.desired_configuration()?.revision,
            if phase == "after-commit" { 2 } else { 1 }
        );
        assert_eq!(
            core.configuration_receipt("transition").await?.is_some(),
            phase == "after-commit"
        );
        assert!(core.reconcile_desired_state().await?.converged());
        core.start().await?;
        let journal = resolver
            .journal
            .lock()
            .expect("observations")
            .as_ref()
            .and_then(Weak::upgrade)
            .context("restored journal")?;
        let saved =
            query_output_codec()?.decode(&std::fs::read(directory.path().join("output.json"))?)?;
        let saved_journal: Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("journal.json"))?)?;
        assert_eq!(json!(journal.output_journal_identity()), saved_journal);
        if phase == "pending-output" {
            let output = tokio::time::timeout(DEADLINE, receive.recv())
                .await?
                .context("retained output after process exit")?;
            assert_eq!(output.changes().id(), saved.changes().id());
            assert_eq!(output.changes().operations(), saved.changes().operations());
            assert_eq!(
                QueryRecoveryIdentity::from_envelope(&output)?,
                QueryRecoveryIdentity::from_envelope(&saved)?
            );
        }
        tokio::time::timeout(DEADLINE, async {
            while journal.progress().await?.processed["sink"] != 1 {
                tokio::task::yield_now().await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        assert_eq!(journal.progress().await?.accepted, 1);
        core.shutdown().await?;
        assert!(receive.try_recv().is_err(), "unexpected output: {phase}");
    }
    Ok(())
}

async fn assert_unsafe_changes_rejected(core: &Arc<DrasiLib>, directory: &Path) -> Result<()> {
    let original = core.desired_configuration()?;
    let registry = InstanceRegistry::new();
    registry
        .add("shared-recovery".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(
        registry,
        Arc::new(false),
        None,
        Arc::new(RwLock::new(PluginRegistry::new())),
        None,
    );
    let mut changed_path = original.desired.clone();
    changed_path
        .topology
        .resource_configurations
        .get_mut(&resource("z-storage"))
        .context("storage recipe")?["path"] = json!(directory.join("replacement"));
    let conflict = core
        .apply_desired_state(0, "pipeline", changed_path.clone())
        .await
        .expect_err("changed retry");
    assert!(matches!(
        conflict.downcast_ref::<drasi_lib::management::ManagementError>(),
        Some(drasi_lib::management::ManagementError::RequestConflict)
    ));
    let mut changed_query = original.desired.clone();
    let query = changed_query
        .topology
        .components
        .iter_mut()
        .find(|node| node.descriptor.id().as_str() == "query")
        .context("query")?;
    let ComponentConstruction::Factory(specification) = &mut query.construction else {
        anyhow::bail!("query factory");
    };
    specification.configuration.insert(
        "query".into(),
        ConfigurationValue::Literal("MATCH (n:Item) RETURN 8 AS value".into()),
    );
    let mut changed_member = original.desired.clone();
    let mut definition: QosChannelDefinition = serde_json::from_value(
        changed_member.topology.resource_configurations[&resource("a-journal")]["definition"]
            .clone(),
    )?;
    definition.subscribers = BTreeMap::from([("replacement".into(), SubscriptionStart::Earliest)]);
    changed_member
        .topology
        .resource_configurations
        .get_mut(&resource("a-journal"))
        .context("journal")?["definition"] = serde_json::to_value(&definition)?;
    for edge in &mut changed_member.topology.relationships {
        if let DesiredPipe::Qos(pipe) = &mut edge.pipe {
            pipe.definition = definition.clone();
            pipe.subscriber = "replacement".into();
        }
    }
    let mut downgraded = original.desired.clone();
    let definition =
        downgraded.topology.resource_configurations[&resource("a-journal")]["definition"].clone();
    downgraded.topology.resource_configurations.insert(
        resource("a-journal"),
        json!({"kind":"qos","definition":definition,"path":directory.join("separate")}),
    );
    downgraded
        .topology
        .resource_dependencies
        .remove(&resource("a-journal"));
    for edge in &mut downgraded.topology.relationships {
        if let DesiredPipe::Qos(pipe) = &mut edge.pipe {
            pipe.shared_storage = None;
        }
    }
    for (name, candidate) in [
        ("redirect", changed_path),
        ("query", changed_query),
        ("membership", changed_member),
        ("downgrade", downgraded),
        ("remove", DesiredInstance::default()),
    ] {
        validate_definition(&ComputationConfig {
            definition: candidate.topology.clone(),
        })?;
        let body =
            json!({"expectedRevision":original.revision,"requestId":name,"desired":candidate});
        let (status, response) = request(
            &app,
            "PUT",
            "/instances/shared-recovery/computation/desired",
            &body.to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::CONFLICT, "{response}");
        assert_eq!(
            response["code"], "CONFIGURATION_TRANSITION_REQUIRED",
            "{response}"
        );
        assert!(!response
            .to_string()
            .contains(&directory.display().to_string()));
        assert_eq!(core.desired_configuration()?, original);
        assert!(core.configuration_receipt(name).await?.is_none());
    }
    let receipt = core
        .apply_desired_state(0, "pipeline", original.desired.clone())
        .await?;
    assert_eq!(receipt.revision, original.revision);
    assert_eq!(core.desired_configuration()?, original);
    Ok(())
}
