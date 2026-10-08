use super::*;
use drasi_core::{
    computation::ComputationTransaction,
    interface::{FailureMode, StorageDurability},
    models::{ElementMetadata, ElementPropertyMap, ElementReference},
};
use drasi_host_sdk::computation::NativeConsumerResource;
use drasi_lib::management::{
    DesiredInstance, ManagementResourceResolver, RecoveryRetirementAuthorization,
};
use drasi_server::computation::{validate_definition, ServerManagementResources};
use std::{
    collections::BTreeMap,
    sync::{Mutex, Weak},
};

const INSTANCE: &str = "native-services";

#[tokio::test(flavor = "current_thread")]
async fn native_service_discovery_reports_negotiated_factories_without_configuration() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        Arc::new(PluginLifecycleManager::new(Arc::new(RwLock::new(
            registry()?,
        )))),
        directory.path().into(),
    ));
    let app = build_plugin_router(orchestrator, InstanceRegistry::new(), Arc::new(false));
    let (status, metadata) = request(&app, "GET", "/computation", "", "application/json").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metadata["bootstrapFactories"].as_array().unwrap().len(), 2);
    assert_eq!(metadata["consumerFactories"].as_array().unwrap().len(), 2);
    assert!(metadata["bootstrapFactories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|factory| factory["source_progress"] == true));
    assert!(metadata["consumerFactories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|factory| factory["mode"] == "transactional"));
    assert!(!metadata.to_string().contains("fixture-only"));
    Ok(())
}

fn recovery_plugin() -> Result<Arc<NativePlugin>> {
    let path = std::env::var_os("DRASI_NATIVE_RECOVERY_PLUGIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../drasi-core/target/debug/examples")
                .join(format!(
                    "{}native_recovery{}",
                    std::env::consts::DLL_PREFIX,
                    std::env::consts::DLL_SUFFIX
                ))
        });
    drasi_host_sdk::computation::load(path)
}

fn registry() -> Result<PluginRegistry> {
    let mut registry = factories();
    registry.register_computation_plugin(recovery_plugin()?)?;
    Ok(registry)
}

fn desired(spec: ComponentSpecification) -> DesiredComponent {
    DesiredComponent {
        streams: spec
            .descriptor
            .ports()
            .iter()
            .filter(|port| port.direction() == PortDirection::Output)
            .map(|port| {
                (
                    port.id().clone(),
                    StreamId::try_new(format!("{}/out", spec.descriptor.id())).unwrap(),
                )
            })
            .collect(),
        descriptor: spec.descriptor.clone(),
        role: spec.role,
        completion: spec.completion,
        lifecycle: LifecyclePolicy { auto_start: true },
        input_merge: InputMergePolicy::default(),
        construction: ComponentConstruction::Factory(spec),
    }
}

fn declare(topology: &mut DesiredTopology, name: &str, role: ResourceRole, recipe: Value) {
    topology.resources.push(ResourceSpecification {
        id: resource(name),
        role,
        ownership: ResourceOwnership::Graph,
        binding: name.into(),
    });
    topology
        .resource_configurations
        .insert(resource(name), recipe);
}

fn bootstrap_config(path: &Path) -> Result<ComputationConfig> {
    let mut topology = DesiredInstance::default().topology;
    let indexes = resource("indexes");
    let query = ContinuousQueryDefinition {
        graph_id: topology.graph_id.clone(),
        id: id("query"),
        query: "MATCH (n:Item) RETURN n.value AS value".into(),
        language: ComputationQueryLanguage::Cypher,
        output_stream: StreamId::try_new("query/out")?,
        outbox_capacity: NonZeroUsize::new(16).unwrap(),
    };
    topology.components = vec![
        desired(factory(COUNTER).specification(
            id("input"),
            json!({"stream":"input/out","count":1,"paused":true}),
        )?),
        desired(ComponentSpecification {
            descriptor: query.descriptor(),
            role: ComponentRole::Query,
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
                ("indexes".into(), vec![indexes]),
                ("bootstrap".into(), vec![resource("bootstrap")]),
                ("source_progress".into(), vec![resource("progress")]),
                ("catalog".into(), vec![resource("catalog")]),
            ]),
        }),
        desired(ComponentSpecification {
            descriptor: QueryResultsOutlet::new(
                id("outlet"),
                QueryResultsCatalog::new(topology.graph_id.as_str())?,
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
            dependencies: BTreeMap::from([("catalog".into(), vec![resource("catalog")])]),
        }),
    ];
    topology.relationships = [("input", "query"), ("query", "outlet")]
        .into_iter()
        .map(|(from, to)| DesiredRelationship {
            definition: edge(from, to),
            policy: RelationshipPolicy::default(),
            pipe: DesiredPipe::Bounded { capacity: 8 },
        })
        .collect();
    declare(
        &mut topology,
        "indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"rocksdbIndexes","path":path}),
    );
    declare(
        &mut topology,
        "catalog",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    );
    declare(
        &mut topology,
        "progress",
        ResourceRole::Checkpoint,
        json!({"kind":"sourceProgress","component":"query"}),
    );
    declare(
        &mut topology,
        "secrets",
        ResourceRole::SecretStore,
        json!({"kind":"configuration"}),
    );
    let metadata = recovery_plugin()?.bootstrap_factories()[1]
        .metadata()
        .clone();
    let configuration = BTreeMap::<Arc<str>, _>::from([
        (
            "durability".into(),
            ConfigurationValue::Literal(json!(StorageDurability::LOCAL_PROCESS_RESTART)),
        ),
        (
            "count".into(),
            ConfigurationValue::Reference {
                resource: resource("secrets"),
                key: "secret-json:COUNT".into(),
                secret: true,
            },
        ),
        (
            "testSecret".into(),
            ConfigurationValue::Reference {
                resource: resource("secrets"),
                key: "secret:BOOTSTRAP".into(),
                secret: true,
            },
        ),
    ]);
    declare(
        &mut topology,
        "bootstrap",
        ResourceRole::Bootstrap,
        json!({"kind":"nativeBootstrap","component":"query","implementation":metadata.implementation,
            "configurationVersion":metadata.configuration_version,"configuration":configuration,"sourceProgress":"progress"}),
    );
    topology.resource_dependencies.insert(
        resource("bootstrap"),
        BTreeMap::from([
            (resource("progress"), ResourceRole::Checkpoint),
            (resource("secrets"), ResourceRole::SecretStore),
        ]),
    );
    Ok(ComputationConfig {
        definition: topology,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn native_bootstrap_server_recipes_restore_snapshot_and_preserve_secret_references(
) -> Result<()> {
    use drasi_server::managed_configuration::{ConfigurationStoreConfig, ConfigurationStores};
    let directory = tempfile::tempdir()?;
    let config = bootstrap_config(&directory.path().join("indexes"))?;
    validate_definition(&config)?;
    let yaml: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
    assert_eq!(yaml.definition, config.definition);
    let key = directory.path().join("key");
    std::fs::write(&key, [19; 32])?;
    let settings = ConfigurationStoreConfig::Redb {
        path: directory.path().join("configuration.redb"),
        key_file: key,
    };
    for (instance, count, expected) in
        [(INSTANCE, "3", 3), (INSTANCE, "9", 3), ("isolated", "5", 5)]
    {
        let stores = ConfigurationStores::default();
        let secrets = Arc::new(
            drasi_lib::secret_store::MemorySecretStoreProvider::new()
                .with_secret("COUNT", count)
                .with_secret("BOOTSTRAP", "fixture-only"),
        );
        let core = stores
            .configure(
                DrasiLib::builder().with_secret_store_provider(secrets.clone()),
                instance,
                &settings,
                Some(&yaml),
                Arc::new(RwLock::new(registry()?)),
                Some(secrets),
            )
            .await?
            .build()
            .await?;
        core.start().await?;
        assert!(core.reconcile_desired_state().await?.converged());
        assert_eq!(core.get_query_results("query").await?.len(), expected);
        let saved = core.snapshot_computation_configuration().await?;
        let rebuilt = configuration_from_snapshot(&saved)?.context("native snapshot")?;
        assert_eq!(
            rebuilt.definition.resource_configurations,
            config.definition.resource_configurations
        );
        assert!(!serde_json::to_string(&saved)?.contains("fixture-only"));
        let mut bad = core.desired_configuration()?.desired.clone();
        bad.topology
            .resource_configurations
            .get_mut(&resource("bootstrap"))
            .unwrap()["configurationVersion"] = json!(999);
        assert!(core
            .apply_desired_state(
                core.desired_configuration()?.revision,
                "invalid-bootstrap",
                bad
            )
            .await
            .is_err());
        assert!(core
            .configuration_receipt("invalid-bootstrap")
            .await?
            .is_none());
        core.shutdown().await?;
    }
    let mut bad = config;
    bad.definition.resource_dependencies.clear();
    assert!(validate_definition(&bad).is_err());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_bootstrap_domain_retires_actual_progress_and_metadata_resources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = bootstrap_config(&directory.path().join("indexes"))?;
    let channel = QosChannelDefinition {
        stream: StreamId::try_new("query/out")?,
        capacity: NonZeroUsize::new(16).unwrap(),
        durable: true,
        retention: RetentionPolicy::Backpressure,
        subscribers: BTreeMap::from([("outlet".into(), SubscriptionStart::Earliest)]),
    };
    config.definition.relationships[1].pipe =
        DesiredPipe::Qos(channel.pipe(resource("journal"), "outlet"));
    declare(
        &mut config.definition,
        "journal",
        ResourceRole::StateStore,
        json!({"kind":"qos","definition":channel,"path":directory.path().join("journal"),
            "recovery":{"kind":"replay","failureScope":FailureMode::ProcessRestart,"receiptCapacity":16}}),
    );
    let mut accepted = None;
    for restart in [false, true] {
        let plugins = registry()?;
        let secrets = Arc::new(
            drasi_lib::secret_store::MemorySecretStoreProvider::new()
                .with_secret("COUNT", if restart { "9" } else { "3" })
                .with_secret("BOOTSTRAP", "fixture-only"),
        );
        let core = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_secret_store_provider(secrets.clone())
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(Arc::new(ServerManagementResources::new(
                &plugins,
                Some(secrets),
            )?))
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [59; 32],
                )?,
            ))
            .build()
            .await?;
        if !restart {
            core.apply_desired_state(
                0,
                "create",
                DesiredInstance::from(config.definition.clone()),
            )
            .await?;
        }
        core.start().await?;
        assert!(core.reconcile_desired_state().await?.converged());
        assert_eq!(core.get_query_results("query").await?.len(), 3);
        let control = core.computation_control()?;
        control
            .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
            .await?;
        core.stop().await?;
        if !restart {
            let mut desired = core.desired_configuration()?.desired.clone();
            desired
                .topology
                .resources
                .iter_mut()
                .find(|spec| spec.id == resource("bootstrap"))
                .unwrap()
                .binding = "replacement".into();
            core.apply_desired_state(
                core.desired_configuration()?.revision,
                "retire",
                desired.clone(),
            )
            .await?;
            assert!(core.reconcile_desired_state().await?.converged());
            assert!(
                !serde_json::to_string(&core.desired_configuration()?)?.contains("fixture-only")
            );
            accepted = Some(desired);
        } else {
            assert_eq!(
                Some(core.desired_configuration()?.desired.clone()),
                accepted
            );
            assert!(core.configuration_receipt("retire").await?.is_some());
        }
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_bootstrap_imperative_api_reconstructs_snapshot_in_another_instance() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = bootstrap_config(&directory.path().join("indexes"))?;
    for (instance, count) in [("first", 3), ("clone", 5)] {
        let secrets = Arc::new(
            drasi_lib::secret_store::MemorySecretStoreProvider::new()
                .with_secret("COUNT", count.to_string())
                .with_secret("BOOTSTRAP", "fixture-only"),
        );
        let core = Arc::new(
            DrasiLib::builder()
                .with_id(instance)
                .with_secret_store_provider(secrets)
                .build()
                .await?,
        );
        let instances = InstanceRegistry::new();
        instances
            .add(instance.into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let app = build_v1_router(
            instances,
            Arc::new(false),
            None,
            Arc::new(RwLock::new(registry()?)),
            None,
        );
        let (status, body) = request(
            &app,
            "POST",
            &format!("/instances/{instance}/computation/components"),
            &serde_yaml::to_string(&config)?,
            "application/yaml",
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        core.start().await?;
        assert_eq!(core.get_query_results("query").await?.len(), count);
        config = configuration_from_snapshot(&core.snapshot_computation_configuration().await?)?
            .context("native clone")?;
        assert!(!serde_json::to_string(&config)?.contains("fixture-only"));
        core.shutdown().await?;
    }
    Ok(())
}

struct ObservedResources {
    inner: ServerManagementResources,
    journal: Mutex<Option<Weak<QosChannel>>>,
}
#[async_trait::async_trait]
impl ManagementResourceResolver for ObservedResources {
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
        spec: &ResourceSpecification,
        config: &Value,
    ) -> Result<ResourceHandle> {
        self.resolve_with_dependencies(instance, graph, spec, config, &BTreeMap::new())
            .await
    }
    async fn resolve_with_dependencies(
        &self,
        instance: &str,
        graph: &str,
        spec: &ResourceSpecification,
        config: &Value,
        dependencies: &BTreeMap<ResourceId, ResourceHandle>,
    ) -> Result<ResourceHandle> {
        let handle = self
            .inner
            .resolve_with_dependencies(instance, graph, spec, config, dependencies)
            .await?;
        if spec.id == resource("journal") {
            *self.journal.lock().unwrap() = Some(Arc::downgrade(&handle.get::<QosChannel>()?));
        }
        Ok(handle)
    }
}

fn journal_definition() -> Result<QosChannelDefinition> {
    Ok(QosChannelDefinition {
        stream: StreamId::try_new("input/out")?,
        capacity: NonZeroUsize::new(4).unwrap(),
        durable: true,
        retention: RetentionPolicy::Backpressure,
        subscribers: BTreeMap::from([("consumer".into(), SubscriptionStart::Earliest)]),
    })
}

fn standalone_journal_config(path: &Path) -> Result<ComputationConfig> {
    let mut topology = DesiredInstance::default().topology;
    declare(
        &mut topology,
        "journal",
        ResourceRole::StateStore,
        json!({"kind":"qos","definition":journal_definition()?,"path":path,
            "recovery":{"kind":"replay","failureScope":FailureMode::ProcessRestart,"receiptCapacity":4}}),
    );
    Ok(ComputationConfig {
        definition: topology,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn standalone_journal_transition_refuses_pending_delivery_and_preserves_drained_state(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = standalone_journal_config(&directory.path().join("journal"))?;
    declare(
        &mut config.definition,
        "unused-indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"rocksdbIndexes","path":directory.path().join("unused")}),
    );
    validate_definition(&config)?;
    let resources = Arc::new(ObservedResources {
        inner: ServerManagementResources::new(&PluginRegistry::new(), None)?,
        journal: Mutex::new(None),
    });
    let core = DrasiLib::builder()
        .with_id(INSTANCE)
        .with_management_resources(resources.clone())
        .with_configuration_store(Arc::new(
            drasi_state_store_redb::RedbConfigurationStore::new(
                directory.path().join("configuration.redb"),
                [43; 32],
            )?,
        ))
        .build()
        .await?;
    let receipt = core
        .apply_desired_state(
            0,
            "create",
            DesiredInstance::from(config.definition.clone()),
        )
        .await?;
    assert!(core.reconcile_desired_state().await?.converged());
    let old = resources
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .context("journal")?;
    let identity = old.output_journal_identity();
    old.publish(&input(&config.definition.graph_id)?).await?;
    let mut desired = core.desired_configuration()?.desired.clone();
    desired.topology.resources[0].binding = "replacement-owner".into();
    assert!(core
        .apply_desired_state(receipt.revision, "transition", desired.clone())
        .await
        .is_err());
    assert!(core.configuration_receipt("transition").await?.is_none());
    assert_eq!(old.progress().await?.processed["consumer"], 0);

    let mut pipe = journal_definition()?
        .pipe(resource("journal"), "consumer")
        .create_with_resources(&BTreeMap::from([(resource("journal"), old.resource())]))?;
    let mut receiver = pipe.pipe.take_receiver()?;
    receiver
        .receive()
        .await?
        .context("pending delivery")?
        .into_parts()
        .1
        .context("completion")?
        .complete(HandlingOutcome::Handled)
        .await?;
    pipe.control.cancel();
    drop(receiver);
    drop(pipe);
    let mut unknown = desired.clone();
    unknown.topology.resources[1].binding = "unverified-owner".into();
    assert!(core
        .apply_desired_state(receipt.revision, "unknown", unknown)
        .await
        .is_err());
    assert!(core.configuration_receipt("unknown").await?.is_none());
    core.apply_desired_state(receipt.revision, "transition", desired.clone())
        .await?;
    assert!(core.reconcile_desired_state().await?.converged());
    assert!(old
        .publish(&input(&config.definition.graph_id)?)
        .await
        .is_err());
    let current = resources
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .context("replacement")?;
    assert!(!Arc::ptr_eq(&old, &current));
    assert_eq!(current.output_journal_identity(), identity);
    assert_eq!(current.progress().await?.processed["consumer"], 1);
    assert_eq!(core.desired_configuration()?.desired, desired);
    core.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn standalone_journal_retirement_holds_writers_through_commit_and_shutdown() -> Result<()> {
    retirement_holds_writers_through_commit_and_shutdown(false, false).await
}

#[tokio::test(flavor = "current_thread")]
async fn native_consumer_retirement_holds_configuration_through_commit_and_shutdown() -> Result<()>
{
    retirement_holds_writers_through_commit_and_shutdown(true, false).await
}

#[tokio::test(flavor = "current_thread")]
async fn loss_authorized_consumer_retirement_preserves_storage_through_uncertain_acceptance(
) -> Result<()> {
    retirement_holds_writers_through_commit_and_shutdown(true, true).await
}

#[tokio::test(flavor = "current_thread")]
async fn loss_authorized_journal_retirement_preserves_storage_through_uncertain_acceptance(
) -> Result<()> {
    retirement_holds_writers_through_commit_and_shutdown(false, true).await
}

fn retire_definition(previous: &DesiredInstance, revision: u64) -> DesiredInstance {
    DesiredInstance {
        retirement: Some(RecoveryRetirementAuthorization {
            from_revision: revision,
            allow_data_loss: true,
            resources: previous
                .topology
                .resources
                .iter()
                .map(|spec| spec.id.clone())
                .collect(),
            components: previous
                .topology
                .components
                .iter()
                .map(|node| node.descriptor.id().clone())
                .collect(),
        }),
        ..Default::default()
    }
}

async fn preserved_retired_storage(
    config: &ComputationConfig,
    accepted: u64,
    native_consumer: bool,
) -> Result<()> {
    if native_consumer {
        assert_eq!(
            consumer_count(config).await?,
            Some(ElementValue::Integer(1))
        );
    }
    let resolver = ServerManagementResources::new(&registry()?, None)?;
    let spec = config
        .definition
        .resources
        .iter()
        .find(|spec| spec.id == resource("journal"))
        .unwrap();
    let handle = resolver
        .resolve(
            INSTANCE,
            &config.definition.graph_id,
            spec,
            &config.definition.resource_configurations[&spec.id],
        )
        .await?;
    let journal = handle.get::<QosChannel>()?;
    let progress = journal.progress().await?;
    assert_eq!(progress.accepted, accepted);
    assert_eq!(
        progress.processed["consumer"], 0,
        "retirement must not acknowledge abandoned input"
    );
    let mut pipe = journal
        .definition()
        .pipe(resource("journal"), "consumer")
        .create_with_resources(&BTreeMap::from([(resource("journal"), handle)]))?;
    let mut receiver = pipe.pipe.take_receiver().context("receiver")?;
    let retained = receiver
        .receive()
        .await?
        .context("abandoned data was erased")?;
    assert_eq!(retained.into_parts().0.changes().operations().len(), 3);
    pipe.control.cancel();
    drop(receiver);
    journal.shutdown().await?;
    Ok(())
}

async fn retirement_holds_writers_through_commit_and_shutdown(
    native_consumer: bool,
    allow_loss: bool,
) -> Result<()> {
    use super::managed_faults::{ConfirmationGate, GatedStore};
    use std::sync::atomic::{AtomicBool, Ordering};
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
        let config = if native_consumer {
            consumer_config(directory.path())?
        } else {
            standalone_journal_config(&directory.path().join("journal"))?
        };
        let plugins = if native_consumer {
            registry()?
        } else {
            PluginRegistry::new()
        };
        let gate = Arc::new(ConfirmationGate {
            armed: AtomicBool::new(false),
            loads_blocked: AtomicBool::new(false),
            committed: Notify::new(),
            release: Semaphore::new(0),
            fail: uncertain,
            reject,
        });
        let resources = Arc::new(ObservedResources {
            inner: ServerManagementResources::new(&plugins, None)?,
            journal: Mutex::new(None),
        });
        let core = Arc::new(
            DrasiLib::builder()
                .with_id(INSTANCE)
                .with_component_factories(plugins.computation_factory_registry()?)
                .with_management_resources(resources.clone())
                .with_configuration_store(Arc::new(GatedStore {
                    inner: drasi_state_store_redb::RedbConfigurationStore::new(
                        directory.path().join("configuration.redb"),
                        [43; 32],
                    )?,
                    gate: gate.clone(),
                }))
                .build()
                .await?,
        );
        core.apply_desired_state(
            0,
            "create",
            DesiredInstance::from(config.definition.clone()),
        )
        .await?;
        if native_consumer {
            core.start().await?;
        }
        assert!(core.reconcile_desired_state().await?.converged());
        if native_consumer && !allow_loss {
            let control = core.computation_control()?;
            control
                .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
                .await?;
            core.stop().await?;
        }
        let old = resources
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        let identity = old.output_journal_identity();
        if allow_loss {
            old.publish(&input(&config.definition.graph_id)?).await?;
            if native_consumer {
                tokio::time::timeout(DEADLINE, async {
                    while !directory.path().join("failed").exists() {
                        tokio::task::yield_now().await;
                    }
                })
                .await?;
                core.stop().await?;
            }
            assert_eq!(old.progress().await?.accepted, 1);
            assert_eq!(old.progress().await?.processed["consumer"], 0);
        }
        let original = core.desired_configuration()?;
        let mut desired = if allow_loss {
            retire_definition(&original.desired, original.revision)
        } else {
            original.desired.clone()
        };
        if !allow_loss {
            desired
                .topology
                .resources
                .iter_mut()
                .find(|spec| {
                    spec.id
                        == resource(if native_consumer {
                            "delivery"
                        } else {
                            "journal"
                        })
                })
                .context("retirement resource")?
                .binding = "replacement-owner".into();
        }
        gate.armed.store(true, Ordering::SeqCst);
        let caller_core = core.clone();
        let next = desired.clone();
        let mut caller = tokio::spawn(async move {
            caller_core
                .apply_desired_state(original.revision, "transition", next)
                .await
        });
        tokio::time::timeout(DEADLINE, async {
            tokio::select! {
                () = gate.committed.notified() => Ok(()),
                result = &mut caller => anyhow::bail!("retirement refused before commit: {result:?}"),
            }
        }).await??;
        let event = input_sequence(&config.definition.graph_id, if allow_loss { 2 } else { 1 })?;
        let mut writer = Box::pin(old.publish(&event));
        assert!(futures::poll!(&mut writer).is_pending());
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
            assert!(futures::poll!(&mut writer).is_pending());
            if close_unconfirmed {
                let (stopped, write) =
                    tokio::time::timeout(DEADLINE, async { tokio::join!(core.shutdown(), writer) })
                        .await?;
                stopped?;
                assert!(write.is_err());
                assert!(old.progress().await.is_err());
                let restored = DrasiLib::builder()
                    .with_id(INSTANCE)
                    .with_component_factories(plugins.computation_factory_registry()?)
                    .with_management_resources(resources.clone())
                    .with_configuration_store(Arc::new(
                        drasi_state_store_redb::RedbConfigurationStore::new(
                            directory.path().join("configuration.redb"),
                            [43; 32],
                        )?,
                    ))
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
                assert!(restored.reconcile_desired_state().await?.converged());
                if allow_loss && !reject {
                    assert!(restored
                        .desired_configuration()?
                        .desired
                        .topology
                        .resources
                        .is_empty());
                } else {
                    let current = resources
                        .journal
                        .lock()
                        .unwrap()
                        .as_ref()
                        .and_then(Weak::upgrade)
                        .context("restored")?;
                    assert_eq!(current.output_journal_identity(), identity);
                    assert_eq!(current.progress().await?.accepted, u64::from(allow_loss));
                }
                restored.shutdown().await?;
                if allow_loss {
                    preserved_retired_storage(&config, 1, native_consumer).await?;
                }
                continue;
            }
            gate.loads_blocked.store(false, Ordering::SeqCst);
        }
        let (status, write) = tokio::time::timeout(DEADLINE, async {
            tokio::join!(core.reconcile_desired_state(), writer)
        })
        .await?;
        assert!(status?.converged());
        assert_eq!(write.is_ok(), reject);
        assert_eq!(old.progress().await.is_ok(), reject);
        assert_eq!(
            core.desired_configuration()?.desired,
            if reject { original.desired } else { desired }
        );
        assert_eq!(
            core.configuration_receipt("transition").await?.is_some(),
            !reject
        );
        core.shutdown().await?;
        if allow_loss {
            preserved_retired_storage(&config, 1 + u64::from(reject), native_consumer).await?;
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn retirement_api_requires_explicit_scoped_permission_and_persists_it_with_receipts(
) -> Result<()> {
    for persistent in [false, true] {
        let directory = tempfile::tempdir()?;
        let config = standalone_journal_config(&directory.path().join("journal"))?;
        let plugins = factories();
        let resources = Arc::new(ObservedResources {
            inner: ServerManagementResources::new(&plugins, None)?,
            journal: Mutex::new(None),
        });
        let store = Arc::new(drasi_state_store_redb::RedbConfigurationStore::new(
            directory.path().join("configuration.redb"),
            [61; 32],
        )?);
        let mut builder = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(resources.clone());
        if persistent {
            builder = builder.with_configuration_store(store.clone());
        }
        let core = Arc::new(builder.build().await?);
        core.apply_desired_state(
            0,
            "create",
            DesiredInstance::from(config.definition.clone()),
        )
        .await?;
        assert!(core.reconcile_desired_state().await?.converged());
        let journal = resources
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        journal
            .publish(&input(&config.definition.graph_id)?)
            .await?;
        let previous = core.desired_configuration()?;
        let desired = retire_definition(&previous.desired, previous.revision);
        let instances = InstanceRegistry::new();
        instances
            .add(INSTANCE.into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let app = build_v1_router(
            instances,
            Arc::new(false),
            None,
            Arc::new(RwLock::new(plugins)),
            None,
        );
        let uri = format!("/instances/{INSTANCE}/computation/desired");
        for case in 0..5 {
            let mut invalid = desired.clone();
            let expected = match case {
                0 => {
                    invalid.retirement = None;
                    StatusCode::CONFLICT
                }
                1 => {
                    invalid.retirement.as_mut().unwrap().allow_data_loss = false;
                    StatusCode::BAD_REQUEST
                }
                2 => {
                    invalid.retirement.as_mut().unwrap().from_revision = 0;
                    StatusCode::CONFLICT
                }
                3 => {
                    invalid
                        .retirement
                        .as_mut()
                        .unwrap()
                        .resources
                        .insert(resource("foreign"));
                    StatusCode::CONFLICT
                }
                _ => {
                    invalid.topology = previous.desired.topology.clone();
                    StatusCode::BAD_REQUEST
                }
            };
            let key = format!("invalid-{case}");
            let payload =
                json!({"expectedRevision":previous.revision,"requestId":key,"desired":invalid});
            let (status, body) =
                request(&app, "PUT", &uri, &payload.to_string(), "application/json").await?;
            assert_eq!(status, expected, "{body}");
            assert!(core.configuration_receipt(key).await?.is_none());
            assert_eq!(core.desired_configuration()?, previous);
            assert_eq!(journal.progress().await?.processed["consumer"], 0);
        }
        let payload =
            json!({"expectedRevision":previous.revision,"requestId":"retire","desired":desired});
        let (status, receipt) = request(
            &app,
            "PUT",
            &uri,
            &serde_yaml::to_string(&payload)?,
            "application/yaml",
        )
        .await?;
        if !persistent {
            assert_eq!(status, StatusCode::CONFLICT, "{receipt}");
            assert_eq!(core.desired_configuration()?, previous);
            assert!(core.configuration_receipt("retire").await?.is_none());
            core.shutdown().await?;
            preserved_retired_storage(&config, 1, false).await?;
            continue;
        }
        assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
        assert_eq!(receipt["data"]["durable"], true);
        assert!(core.reconcile_desired_state().await?.converged());
        assert_eq!(core.desired_configuration()?.desired, desired);
        assert!(journal.progress().await.is_err());
        let (status, retried) =
            request(&app, "PUT", &uri, &payload.to_string(), "application/json").await?;
        assert_eq!(status, StatusCode::ACCEPTED, "{retried}");
        assert_eq!(retried["data"], receipt["data"]);
        let mut conflicting = payload.clone();
        conflicting["desired"]["retirement"]["from_revision"] = json!(previous.revision + 1);
        let (status, body) = request(
            &app,
            "PUT",
            &uri,
            &conflicting.to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        core.shutdown().await?;
        preserved_retired_storage(&config, 1, false).await?;

        let restored = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(factories().computation_factory_registry()?)
            .with_management_resources(resources.clone())
            .with_configuration_store(store)
            .build()
            .await?;
        assert_eq!(restored.desired_configuration()?.desired, desired);
        assert!(restored
            .desired_configuration()?
            .desired
            .topology
            .resources
            .is_empty());
        assert!(restored.configuration_receipt("retire").await?.is_some());
        let retired_revision = restored.desired_configuration()?.revision;
        restored
            .apply_desired_state(retired_revision, "explicit-restore", previous.desired)
            .await?;
        assert!(restored.reconcile_desired_state().await?.converged());
        let restored_revision = restored.desired_configuration()?.revision;
        assert!(restored
            .apply_desired_state(restored_revision, "stale-permission", desired)
            .await
            .is_err());
        assert!(restored
            .configuration_receipt("stale-permission")
            .await?
            .is_none());
        assert_eq!(
            restored.desired_configuration()?.revision,
            restored_revision
        );
        restored.shutdown().await?;
        preserved_retired_storage(&config, 1, false).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_transaction_sequence_retires_its_standalone_owner_and_restores_configuration(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("output.jsonl");
    for restart in [false, true] {
        let plugins = factories();
        let resources = Arc::new(ObservedResources {
            inner: ServerManagementResources::new(&plugins, None)?,
            journal: Mutex::new(None),
        });
        let core = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(resources.clone())
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [53; 32],
                )?,
            ))
            .build()
            .await?;
        if !restart {
            let registry =
                plugins.transactional_transformer_registry(core.middleware_registry())?;
            let mut topology = DesiredInstance::default().topology;
            let transaction = TransactionTransformerDefinition {
                graph_id: topology.graph_id.clone(),
                id: id("transaction"),
                output_stream: StreamId::try_new("transaction/out")?,
                steps: vec![TransactionStepDefinition {
                    id: id("arithmetic"),
                    implementation: factory(ARITHMETIC).metadata().implementation.clone(),
                    configuration_version: 1,
                    configuration: json!({"stream":"step/out","add":1}),
                }],
                outbox_capacity: NonZeroUsize::new(8).unwrap(),
            };
            topology.components = vec![
                desired(
                    factory(COUNTER)
                        .specification(id("input"), json!({"stream":"input/out","count":4}))?,
                ),
                desired(transaction.specification(
                    &registry,
                    resource("transformers"),
                    resource("indexes"),
                )?),
                desired(factory(CAPTURE).specification(id("capture"), json!({"path":output}))?),
            ];
            let channel = QosChannelDefinition {
                stream: StreamId::try_new("transaction/out")?,
                capacity: NonZeroUsize::new(8).unwrap(),
                durable: true,
                retention: RetentionPolicy::Backpressure,
                subscribers: BTreeMap::from([("capture".into(), SubscriptionStart::Earliest)]),
            };
            topology.relationships = vec![
                DesiredRelationship {
                    definition: edge("input", "transaction"),
                    policy: RelationshipPolicy::default(),
                    pipe: DesiredPipe::Bounded { capacity: 4 },
                },
                DesiredRelationship {
                    definition: edge("transaction", "capture"),
                    policy: RelationshipPolicy::default(),
                    pipe: DesiredPipe::Qos(channel.pipe(resource("journal"), "capture")),
                },
            ];
            declare(
                &mut topology,
                "indexes",
                ResourceRole::IndexBackend,
                json!({"kind":"rocksdbIndexes","path":directory.path().join("indexes")}),
            );
            declare(
                &mut topology,
                "transformers",
                ResourceRole::Component,
                json!({"kind":"transactionalTransformers"}),
            );
            topology
                .resources
                .iter_mut()
                .find(|spec| spec.id == resource("transformers"))
                .unwrap()
                .ownership = ResourceOwnership::Borrowed;
            declare(
                &mut topology,
                "journal",
                ResourceRole::StateStore,
                json!({"kind":"qos","definition":channel,"path":directory.path().join("journal"),
                    "recovery":{"kind":"replay","failureScope":FailureMode::ProcessRestart,"receiptCapacity":8}}),
            );
            validate_definition(&ComputationConfig {
                definition: topology.clone(),
            })?;
            core.apply_desired_state(0, "create", DesiredInstance::from(topology))
                .await?;
        }
        core.start().await?;
        assert!(core.reconcile_desired_state().await?.converged());
        let journal = resources
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        tokio::time::timeout(DEADLINE, async {
            while journal.progress().await?.processed["capture"] != 4 {
                tokio::task::yield_now().await;
            }
            anyhow::Ok(())
        })
        .await??;
        let control = core.computation_control()?;
        control
            .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
            .await?;
        core.stop().await?;
        // The capture fixture truncates on reconstruction; completed input must not replay.
        assert_eq!(
            tokio::fs::read_to_string(&output).await?.lines().count(),
            if restart { 0 } else { 4 }
        );
        if !restart {
            let saved = core.desired_configuration()?;
            let mut next = saved.desired.clone();
            next.topology
                .resources
                .iter_mut()
                .find(|spec| spec.id == resource("indexes"))
                .context("indexes")?
                .binding = "replacement".into();
            let ComponentConstruction::Factory(source) = &mut next
                .topology
                .components
                .iter_mut()
                .find(|node| node.descriptor.id() == &id("input"))
                .context("input")?
                .construction
            else {
                anyhow::bail!("expected native source recipe");
            };
            source
                .configuration
                .insert("count".into(), ConfigurationValue::Literal(json!(0)));
            core.apply_desired_state(saved.revision, "retire", next)
                .await?;
            assert!(core.reconcile_desired_state().await?.converged());
            assert!(journal.progress().await.is_err());
        } else {
            assert!(core.configuration_receipt("retire").await?.is_some());
        }
        core.shutdown().await?;
    }
    Ok(())
}

fn consumer_config(path: &Path) -> Result<ComputationConfig> {
    let mut topology = DesiredInstance::default().topology;
    let plugin = recovery_plugin()?;
    let factory = plugin
        .factories()
        .iter()
        .find(|factory| {
            factory.metadata().implementation.name.as_ref() == "fixture/consumer-transactional"
        })
        .unwrap();
    let mut sink = factory.specification(
        id("consumer"),
        json!({"mode":"fail-second-once","signal":path.join("failed")}),
    )?;
    sink.dependencies
        .insert("consumer".into(), vec![resource("delivery")]);
    topology.components = vec![
        desired(super::factory(COUNTER).specification(
            id("input"),
            json!({"stream":"input/out","count":1,"paused":true}),
        )?),
        desired(sink),
    ];
    let channel = journal_definition()?;
    topology.relationships = vec![DesiredRelationship {
        definition: edge("input", "consumer"),
        policy: RelationshipPolicy::default(),
        pipe: DesiredPipe::Qos(channel.pipe(resource("journal"), "consumer")),
    }];
    declare(
        &mut topology,
        "indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"rocksdbIndexes","path":path.join("indexes")}),
    );
    declare(
        &mut topology,
        "delivery",
        ResourceRole::IndexBackend,
        json!({"kind":"nativeConsumer","failureScope":FailureMode::ProcessRestart,"maxStreams":2,"receiptsPerStream":4}),
    );
    declare(
        &mut topology,
        "journal",
        ResourceRole::StateStore,
        json!({"kind":"qos","definition":channel,"path":path.join("journal"),
            "recovery":{"kind":"replay","failureScope":FailureMode::ProcessRestart,"receiptCapacity":4}}),
    );
    topology.resource_dependencies.insert(
        resource("delivery"),
        BTreeMap::from([(resource("indexes"), ResourceRole::IndexBackend)]),
    );
    Ok(ComputationConfig {
        definition: topology,
    })
}

fn input(graph: &str) -> Result<ChangeEnvelope> {
    input_sequence(graph, 1)
}

fn input_sequence(graph: &str, sequence: u64) -> Result<ChangeEnvelope> {
    let producer: GraphProducerIdentity = serde_json::from_value(json!({
        "construction_scope":INSTANCE, "graph_id":graph, "component_id":"input", "stream":"input/out",
        "incarnation":"c218b0c6-8803-48df-aa4c-d5350f0e6453", "persistent":true
    }))?;
    let changes = (0..3)
        .map(|value| SourceChange::Insert {
            element: Element::Node {
                metadata: ElementMetadata {
                    reference: ElementReference::new("input", &value.to_string()),
                    labels: Arc::from([Arc::from("Item")]),
                    effective_from: 1,
                },
                properties: ElementPropertyMap::from(json!({"value":value})),
            },
        })
        .collect::<Vec<_>>();
    let seed =
        GraphChangeCodec::encode_change(changes[0].clone(), producer.stream().clone(), 1, None)?;
    let mut batch =
        GraphChangeCodec::derive_changes(&seed, &changes, producer.stream().clone(), sequence)?;
    GraphProducerProgress::annotate(&mut batch, &producer, sequence)?;
    Ok(batch)
}

async fn consumer_count(config: &ComputationConfig) -> Result<Option<ElementValue>> {
    let resolver = ServerManagementResources::new(&registry()?, None)?;
    let graph = &config.definition.graph_id;
    let indexes = config
        .definition
        .resources
        .iter()
        .find(|spec| spec.id == resource("indexes"))
        .unwrap();
    let delivery = config
        .definition
        .resources
        .iter()
        .find(|spec| spec.id == resource("delivery"))
        .unwrap();
    let provider = resolver
        .resolve(
            INSTANCE,
            graph,
            indexes,
            &config.definition.resource_configurations[&indexes.id],
        )
        .await?;
    let resource = resolver
        .resolve_with_dependencies(
            INSTANCE,
            graph,
            delivery,
            &config.definition.resource_configurations[&delivery.id],
            &BTreeMap::from([(indexes.id.clone(), provider)]),
        )
        .await?;
    let consumer = resource.get::<NativeConsumerResource>()?;
    let transaction = ComputationTransaction::try_new(
        consumer.provider.create_indexes(graph, "consumer").await?,
    )?;
    let reference = ElementReference::new("transaction-step/636f6e73756d6572/values", "count");
    let value = transaction
        .run(async {
            Ok(transaction
                .resources()
                .indexes()
                .element_index
                .get_element(&reference)
                .await?)
        })
        .await?;
    let count = match value.as_deref() {
        Some(Element::Node { properties, .. }) => properties.get("value").cloned(),
        None => None,
        _ => anyhow::bail!("invalid consumer state"),
    };
    transaction.shutdown().await?;
    Ok(count)
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "required child of native_consumer_server_recovers_processing_and_retirement_after_process_exit"]
async fn native_consumer_server_crash_worker() -> Result<()> {
    use super::managed_faults::{ConfirmationGate, GatedStore};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::{Notify, Semaphore};

    let directory = PathBuf::from(std::env::var("DRASI_NATIVE_SERVER_CRASH_DIRECTORY")?);
    let phase = std::env::var("DRASI_NATIVE_SERVER_CRASH_PHASE")?;
    anyhow::ensure!(
        matches!(
            phase.as_str(),
            "partial" | "before-retirement" | "after-retirement"
        ),
        "unknown process-exit phase"
    );
    let config = consumer_config(&directory)?;
    let plugins = registry()?;
    let resources = Arc::new(ObservedResources {
        inner: ServerManagementResources::new(&plugins, None)?,
        journal: Mutex::new(None),
    });
    let gate = Arc::new(ConfirmationGate {
        armed: AtomicBool::new(false),
        loads_blocked: AtomicBool::new(false),
        committed: Notify::new(),
        release: Semaphore::new(0),
        fail: false,
        reject: phase == "before-retirement",
    });
    let core = Arc::new(
        DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(resources.clone())
            .with_configuration_store(Arc::new(GatedStore {
                inner: drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.join("configuration.redb"),
                    [71; 32],
                )?,
                gate: gate.clone(),
            }))
            .build()
            .await?,
    );
    core.apply_desired_state(
        0,
        "create",
        DesiredInstance::from(config.definition.clone()),
    )
    .await?;
    core.start().await?;
    let journal = resources
        .journal
        .lock()
        .unwrap()
        .as_ref()
        .and_then(Weak::upgrade)
        .context("journal")?;
    journal
        .publish(&input(&config.definition.graph_id)?)
        .await?;
    tokio::time::timeout(DEADLINE, async {
        while !directory.join("failed").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(journal.progress().await?.accepted, 1);
    assert_eq!(journal.progress().await?.processed["consumer"], 0);
    if phase == "partial" {
        std::process::exit(91);
    }
    core.stop().await?;
    let current = core.desired_configuration()?;
    let removed = retire_definition(&current.desired, current.revision);
    gate.armed.store(true, Ordering::SeqCst);
    let caller = tokio::spawn(async move {
        core.apply_desired_state(current.revision, "retire", removed)
            .await
    });
    tokio::select! {
        result = caller => anyhow::bail!("retirement did not pause: {result:?}"),
        result = tokio::time::timeout(DEADLINE, gate.committed.notified()) => result?,
    }
    std::process::exit(if phase == "before-retirement" { 92 } else { 93 });
}

#[tokio::test(flavor = "current_thread")]
async fn native_consumer_server_recovers_processing_and_retirement_after_process_exit() -> Result<()>
{
    for (phase, code) in [
        ("partial", 91),
        ("before-retirement", 92),
        ("after-retirement", 93),
    ] {
        let directory = tempfile::tempdir()?;
        let mut child = tokio::process::Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "native_services::native_consumer_server_crash_worker",
                "--ignored",
            ])
            .env("DRASI_NATIVE_SERVER_CRASH_DIRECTORY", directory.path())
            .env("DRASI_NATIVE_SERVER_CRASH_PHASE", phase)
            .kill_on_drop(true);
        let output = tokio::time::timeout(DEADLINE * 3, child.output()).await??;
        assert_eq!(
            output.status.code(),
            Some(code),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let config = consumer_config(directory.path())?;
        preserved_retired_storage(&config, 1, true).await?;
        let plugins = registry()?;
        let resources = Arc::new(ObservedResources {
            inner: ServerManagementResources::new(&plugins, None)?,
            journal: Mutex::new(None),
        });
        let core = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(resources.clone())
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [71; 32],
                )?,
            ))
            .build()
            .await?;
        let original = DesiredInstance::from(config.definition.clone()).normalized()?;
        let retired = phase == "after-retirement";
        let current = core.desired_configuration()?;
        assert_eq!(
            current.desired,
            if retired {
                retire_definition(&original, 1)
            } else {
                original.clone()
            }
        );
        assert_eq!(
            core.configuration_receipt("retire").await?.is_some(),
            retired
        );
        if retired {
            assert!(resources.journal.lock().unwrap().is_none());
            core.apply_desired_state(current.revision, "restore-intact-data", original)
                .await?;
        }
        core.start().await?;
        let journal = resources
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("restored journal")?;
        tokio::time::timeout(DEADLINE, async {
            while journal.progress().await?.processed["consumer"] != 1 {
                tokio::task::yield_now().await;
            }
            anyhow::Ok(())
        })
        .await??;
        assert_eq!(journal.progress().await?.accepted, 1);
        let control = core.computation_control()?;
        control
            .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
            .await?;
        core.shutdown().await?;
        assert_eq!(
            consumer_count(&config).await?,
            Some(ElementValue::Integer(3))
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_consumer_server_recipe_restores_partial_completion_and_retains_upstream_obligation(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = consumer_config(directory.path())?;
    validate_definition(&config)?;
    for lifetime in 0..3 {
        let restart = lifetime > 0;
        let plugins = registry()?;
        let resources = Arc::new(ObservedResources {
            inner: ServerManagementResources::new(&plugins, None)?,
            journal: Mutex::new(None),
        });
        let core = DrasiLib::builder()
            .with_id(INSTANCE)
            .with_component_factories(plugins.computation_factory_registry()?)
            .with_management_resources(resources.clone())
            .with_configuration_store(Arc::new(
                drasi_state_store_redb::RedbConfigurationStore::new(
                    directory.path().join("configuration.redb"),
                    [41; 32],
                )?,
            ))
            .build()
            .await?;
        if !restart {
            core.apply_desired_state(
                0,
                "create",
                DesiredInstance::from(config.definition.clone()),
            )
            .await?;
        }
        core.start().await?;
        assert!(core.reconcile_desired_state().await?.converged());
        let channel = resources
            .journal
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
            .context("journal")?;
        channel
            .publish(&input(&config.definition.graph_id)?)
            .await?;
        tokio::time::timeout(DEADLINE, async {
            loop {
                let ready = if restart {
                    channel.progress().await?.processed["consumer"] == 1
                } else {
                    directory.path().join("failed").exists()
                };
                if ready {
                    return anyhow::Ok(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        let progress = channel.progress().await?;
        assert_eq!(progress.accepted, 1);
        assert_eq!(progress.processed["consumer"], u64::from(restart));
        let mut invalid = core.desired_configuration()?.desired.clone();
        invalid
            .topology
            .resource_configurations
            .get_mut(&resource("delivery"))
            .unwrap()["maxStreams"] = json!(3);
        assert!(core
            .apply_desired_state(
                core.desired_configuration()?.revision,
                "unsafe-rebind",
                invalid
            )
            .await
            .is_err());
        assert!(core.configuration_receipt("unsafe-rebind").await?.is_none());
        let control = core.computation_control()?;
        if restart {
            control
                .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
                .await?;
        }
        core.stop().await?;
        let mut replacement = core.desired_configuration()?.desired.clone();
        replacement
            .topology
            .resource_configurations
            .get_mut(&resource("delivery"))
            .unwrap()["maxStreams"] = json!(3 + lifetime);
        let result = core
            .apply_desired_state(
                core.desired_configuration()?.revision,
                "drained-rebind",
                replacement.clone(),
            )
            .await;
        if lifetime == 1 {
            result?;
            assert_eq!(core.desired_configuration()?.desired, replacement);
            assert!(core
                .configuration_receipt("drained-rebind")
                .await?
                .is_some());
        } else if !restart {
            assert!(
                result.is_err(),
                "partial completion must not authorize a change after stop"
            );
            assert!(core
                .configuration_receipt("drained-rebind")
                .await?
                .is_none());
        } else {
            assert!(
                result.is_err(),
                "idempotency key cannot authorize a different definition"
            );
            assert_eq!(
                core.desired_configuration()?
                    .desired
                    .topology
                    .resource_configurations[&resource("delivery")]["maxStreams"],
                json!(4)
            );
            assert!(core
                .configuration_receipt("drained-rebind")
                .await?
                .is_some());
        }
        core.shutdown().await?;
        drop(core);
        assert_eq!(
            consumer_count(&config).await?,
            Some(ElementValue::Integer(if restart { 3 } else { 1 }))
        );
    }
    Ok(())
}
