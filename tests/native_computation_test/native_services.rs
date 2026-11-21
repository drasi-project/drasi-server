use super::*;
use drasi_core::{
    computation::ComputationTransaction,
    interface::{FailureMode, StorageDurability},
    models::{ElementMetadata, ElementPropertyMap, ElementReference},
};
use drasi_host_sdk::computation::{
    NativeBootstrapFactory, NativeBootstrapNodeFactory, NativeConsumerResource,
};
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
    let names = |field: &str| {
        let mut names = metadata[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|factory| factory["implementation"]["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    };
    assert_eq!(
        names("bootstrapFactories"),
        ["fixture/bootstrap", "fixture/bootstrap-progress"]
    );
    assert_eq!(
        names("consumerFactories"),
        [
            "fixture/consumer-external",
            "fixture/consumer-transactional",
            "fixture/input-binding-consumer"
        ]
    );
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
                .join("target/debug/plugins/examples")
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

fn desired(name: &str, spec: ComponentSpecification) -> DesiredComponent {
    DesiredComponent {
        name: ComponentName::try_new(name).unwrap(),
        streams: spec
            .descriptor
            .ports()
            .iter()
            .filter(|port| {
                port.direction() == PortDirection::Output && port.kind() == PortKind::Data
            })
            .map(|port| {
                (
                    port.id().clone(),
                    match spec
                        .configuration
                        .get("stream")
                        .or_else(|| spec.configuration.get("output_stream"))
                    {
                        Some(ConfigurationValue::Literal(Value::String(stream))) => {
                            StreamId::try_new(stream.as_str()).unwrap()
                        }
                        _ => StreamId::try_new(format!("{}/{}", name, port.id())).unwrap(),
                    },
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

#[tokio::test(flavor = "current_thread")]
async fn native_output_snapshot_recipe_is_an_immutable_policy_and_roundtrips() -> Result<()> {
    let mut topology = DesiredInstance::default().topology;
    declare(
        &mut topology,
        "snapshots",
        ResourceRole::OutputCatalogue,
        json!({
            "kind":"nativeOutputSnapshots",
            "outputs":[{"component":id("q"),"port":"out"},{"component":id("source")}],
            "timeoutMs":1000,
            "maxReaders":2
        }),
    );
    let config = ComputationConfig {
        definition: topology,
    };
    validate_definition(&config)?;
    let json = serde_json::to_value(&config)?;
    let json_roundtrip: ComputationConfig = serde_json::from_value(json.clone())?;
    validate_definition(&json_roundtrip)?;
    let yaml_roundtrip: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
    assert_eq!(serde_json::to_value(&yaml_roundtrip)?, json);
    validate_definition(&yaml_roundtrip)?;
    let resolver = ServerManagementResources::new(&PluginRegistry::new(), None)?;
    let snapshot_spec = &config.definition.resources[0];
    let recipe = &config.definition.resource_configurations[&snapshot_spec.id];
    resolver
        .resolve(INSTANCE, &config.definition.graph_id, snapshot_spec, recipe)
        .await?
        .get::<drasi_host_sdk::computation::NativeOutputSnapshotsResource>()?;
    // The policy holds no catalogue, so a dependency is rejected rather than ignored.
    let mut with_dependency = config.clone();
    declare(
        &mut with_dependency.definition,
        "catalog",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    );
    with_dependency.definition.resource_dependencies.insert(
        resource("snapshots"),
        BTreeMap::from([(resource("catalog"), ResourceRole::QueryCatalog)]),
    );
    assert!(validate_definition(&with_dependency).is_err());
    for update in [
        json!({"outputs":[]}),
        json!({"outputs":[{"component":id("q")},{"component":id("q")}]}),
        json!({"outputs":[{"component":id("q"),"port":"out","extra":true}]}),
        json!({"queries":["q"]}),
        json!({"timeoutMs":0}),
        json!({"maxReaders":0}),
        json!({"maxReaders":1025}),
    ] {
        let mut invalid = config.clone();
        invalid
            .definition
            .resource_configurations
            .get_mut(&resource("snapshots"))
            .unwrap()
            .as_object_mut()
            .unwrap()
            .extend(update.as_object().unwrap().clone());
        assert!(validate_definition(&invalid).is_err(), "{update}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn consumer_progress_recipe_retains_actual_members_and_roundtrips() -> Result<()> {
    let mut topology = DesiredInstance::default().topology;
    declare(
        &mut topology,
        "indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"memoryIndexes"}),
    );
    for consumer in ["left", "right"] {
        declare(
            &mut topology,
            consumer,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":id(consumer),"indexes":"indexes"}),
        );
        topology.resource_dependencies.insert(
            resource(consumer),
            BTreeMap::from([(resource("indexes"), ResourceRole::IndexBackend)]),
        );
    }
    declare(
        &mut topology,
        "feedback",
        ResourceRole::Checkpoint,
        json!({
            "kind":"consumerProgress",
            "producer":id("source"),
            "consumers":{id("left").as_str():"left",id("right").as_str():"right"}
        }),
    );
    topology.resource_dependencies.insert(
        resource("feedback"),
        BTreeMap::from([
            (resource("left"), ResourceRole::Checkpoint),
            (resource("right"), ResourceRole::Checkpoint),
        ]),
    );
    let config = ComputationConfig {
        definition: topology,
    };
    validate_definition(&config)?;
    let json = serde_json::to_value(&config)?;
    let decoded: ComputationConfig = serde_json::from_value(json.clone())?;
    validate_definition(&decoded)?;
    let decoded: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
    assert_eq!(serde_json::to_value(decoded)?, json);

    let resolver = ServerManagementResources::new(&PluginRegistry::new(), None)?;
    let graph = &config.definition.graph_id;
    let indexes = BTreeMap::from([(
        resource("indexes"),
        resolver
            .resolve(
                INSTANCE,
                graph,
                &config.definition.resources[0],
                &config.definition.resource_configurations[&resource("indexes")],
            )
            .await?,
    )]);
    let mut dependencies = BTreeMap::new();
    for specification in &config.definition.resources[1..3] {
        let handle = resolver
            .resolve_with_dependencies(
                INSTANCE,
                graph,
                specification,
                &config.definition.resource_configurations[&specification.id],
                &indexes,
            )
            .await?;
        dependencies.insert(specification.id.clone(), handle);
    }
    let specification = &config.definition.resources[3];
    let recipe = &config.definition.resource_configurations[&specification.id];
    let handle = resolver
        .resolve_with_dependencies(INSTANCE, graph, specification, recipe, &dependencies)
        .await?;
    let feedback = handle.get::<ConsumerProgressSet>()?;
    assert_eq!(feedback.producer_id(), &id("source"));
    assert_eq!(feedback.owners().len(), 2);
    for consumer in ["left", "right"] {
        let actual = dependencies[&resource(consumer)].get::<QuerySourceProgressResource>()?;
        assert!(Arc::ptr_eq(&feedback.owners()[&id(consumer)], &actual.0));
    }
    assert!(!feedback.read_only().snapshot()?.ready());
    assert!(resolver
        .resolve_with_dependencies(INSTANCE, "foreign", specification, recipe, &dependencies,)
        .await
        .is_err());
    assert!(resolver
        .resolve(INSTANCE, graph, specification, recipe)
        .await
        .is_err());
    let mut wrong_owner = dependencies.clone();
    wrong_owner.insert(
        resource("left"),
        ResourceHandle::new(
            ResourceRole::Checkpoint,
            Arc::new(QuerySourceProgressResource(Arc::new(
                QuerySourceProgress::new(graph.as_str(), id("another"))?,
            ))),
        ),
    );
    assert!(resolver
        .resolve_with_dependencies(INSTANCE, graph, specification, recipe, &wrong_owner,)
        .await
        .is_err());
    for consumers in [
        json!({}),
        json!({id("source").as_str():"left"}),
        json!({id("left").as_str():"left",id("right").as_str():"left"}),
        json!({id("left").as_str():"feedback"}),
    ] {
        let mut invalid = config.clone();
        invalid
            .definition
            .resource_configurations
            .get_mut(&resource("feedback"))
            .unwrap()["consumers"] = consumers;
        assert!(validate_definition(&invalid).is_err());
    }
    Ok(())
}

fn bootstrap_topology(index_recipe: Value) -> Result<DesiredTopology> {
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
        desired(
            "input",
            factory(COUNTER).specification(
                id("input"),
                json!({"stream":"input/out","count":1,"paused":true}),
            )?,
        ),
        desired(
            "query",
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
                    ("indexes".into(), vec![indexes]),
                    ("source_progress".into(), vec![resource("progress")]),
                    ("catalog".into(), vec![resource("catalog")]),
                ]),
            },
        ),
        desired(
            "outlet",
            ComponentSpecification {
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
            },
        ),
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
        index_recipe,
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
        json!({"kind":"sourceProgress","component":id("query"),"indexes":"indexes"}),
    );
    topology.resource_dependencies.insert(
        resource("progress"),
        BTreeMap::from([(resource("indexes"), ResourceRole::IndexBackend)]),
    );
    Ok(topology)
}

/// Adds the query's exclusive local bootstrap provider node, `bootstrap`, and
/// its bootstrap edge to the query's bootstrap input.
fn add_bootstrap_provider(
    topology: &mut DesiredTopology,
    factory: Arc<NativeBootstrapFactory>,
    configuration: BTreeMap<Arc<str>, ConfigurationValue>,
) -> Result<()> {
    let node = NativeBootstrapNodeFactory::new(factory);
    let specification = ComponentSpecification {
        descriptor: NativeBootstrapNodeFactory::component_descriptor(id("bootstrap"))?,
        role: ComponentRole::BootstrapProvider,
        completion: None,
        implementation: node.descriptor().implementation.clone(),
        configuration_version: node.descriptor().configuration_version,
        configuration,
        dependencies: if node
            .descriptor()
            .dependencies
            .contains_key("source_progress")
        {
            BTreeMap::from([("source_progress".into(), vec![resource("progress")])])
        } else {
            BTreeMap::new()
        },
    };
    topology
        .components
        .push(desired("bootstrap", specification));
    topology.relationships.push(DesiredRelationship {
        definition: EdgeDefinition::new(
            endpoint("bootstrap", "snapshot"),
            endpoint("query", "bootstrap"),
        ),
        policy: RelationshipPolicy {
            activation: ActivationCoupling::RequiresRunning,
            ..Default::default()
        },
        pipe: DesiredPipe::Bounded { capacity: 8 },
    });
    Ok(())
}

fn by_name(components: &[DesiredComponent]) -> BTreeMap<String, &DesiredComponent> {
    components
        .iter()
        .map(|component| (component.name.to_string(), component))
        .collect()
}

fn provider_mut(topology: &mut DesiredTopology) -> &mut ComponentSpecification {
    let component = topology
        .components
        .iter_mut()
        .find(|component| component.descriptor.id() == &id("bootstrap"))
        .expect("bootstrap provider node");
    let ComponentConstruction::Factory(specification) = &mut component.construction else {
        panic!("factory-constructed provider");
    };
    specification
}

fn bootstrap_config(path: &Path) -> Result<ComputationConfig> {
    let mut topology = bootstrap_topology(json!({"kind":"rocksdbIndexes","path":path}))?;
    declare(
        &mut topology,
        "secrets",
        ResourceRole::SecretStore,
        json!({"kind":"configuration"}),
    );
    let factory = recovery_plugin()?.bootstrap_factories()[1].clone();
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
    add_bootstrap_provider(&mut topology, factory, configuration)?;
    Ok(ComputationConfig {
        definition: topology,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn sqlite_file_bootstrap_uses_native_catalogue_and_memory_query_recipes() -> Result<()> {
    let plugin = drasi_host_sdk::computation::load(artifact("drasi_computation_bootstrap_sqlite"))?;
    assert!(plugin.factories().is_empty());
    let file_bootstrap = plugin
        .bootstrap_factories()
        .iter()
        .find(|factory| {
            factory.metadata().implementation.name.as_ref() == "drasi.sqlite/file-bootstrap"
        })
        .cloned()
        .context("SQLite file bootstrap factory")?;
    // The file snapshot serves its source's element namespace, so the source
    // must declare the same SQLite identities (same tables and keys).
    let source_plugin =
        drasi_host_sdk::computation::load(artifact("drasi_computation_source_sqlite"))?;
    let sqlite_source = source_plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == "drasi.sqlite/changes")
        .cloned()
        .context("SQLite changes source factory")?;
    let mut plugins = factories();
    plugins.register_computation_plugin(plugin)?;
    plugins.register_computation_plugin(source_plugin)?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("input.db");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch(
        "CREATE TABLE Item(code TEXT PRIMARY KEY,value TEXT);
         INSERT INTO Item VALUES('one','first');",
    )?;
    for (instance, value) in [
        ("sqlite-memory-first", "first"),
        ("sqlite-memory-new", "fresh"),
    ] {
        connection.execute("UPDATE Item SET value=?1", [value])?;
        let mut topology = bootstrap_topology(json!({"kind":"memoryIndexes"}))?;
        let input = topology
            .components
            .iter_mut()
            .find(|component| component.name.as_str() == "input")
            .context("input component")?;
        *input = desired(
            "input",
            sqlite_source.specification(
                id("input"),
                json!({
                    "stream": "input/out",
                    "tables": ["Item"],
                    "initSql": "CREATE TABLE Item(code TEXT PRIMARY KEY,value TEXT)"
                }),
            )?,
        );
        let configuration = BTreeMap::<Arc<str>, _>::from([
            ("path".into(), ConfigurationValue::Literal(json!(path))),
            (
                "tables".into(),
                ConfigurationValue::Literal(json!(["Item"])),
            ),
            ("maxTables".into(), ConfigurationValue::Literal(json!(8))),
            ("maxBytes".into(), ConfigurationValue::Literal(json!(65536))),
            (
                "maxDurationMs".into(),
                ConfigurationValue::Literal(json!(5000)),
            ),
            (
                "outputCapacity".into(),
                ConfigurationValue::Literal(json!(1)),
            ),
            (
                "shutdownTimeoutMs".into(),
                ConfigurationValue::Literal(json!(5000)),
            ),
        ]);
        add_bootstrap_provider(&mut topology, file_bootstrap.clone(), configuration)?;
        assert!(
            provider_mut(&mut topology).dependencies.is_empty(),
            "file bootstrap needs no fabricated source-progress dependency"
        );
        let config = ComputationConfig {
            definition: topology,
        };
        validate_definition(&config)?;
        let yaml: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
        assert_eq!(yaml.definition, config.definition);
        let core = crate::query_plugin::builder()
            .with_id(instance)
            .build()
            .await?;
        let result = async {
            register_components(&yaml, &core, &plugins).await?;
            core.start().await?;
            tokio::time::timeout(
                DEADLINE,
                core.component_by_name(&ComponentName::try_new("query")?)?
                    .wait_started(),
            )
            .await??;
            assert_eq!(
                core.get_query_results_by_name(&ComponentName::try_new("query")?)
                    .await?,
                vec![json!({"value":value})]
            );
            let saved = core.snapshot_computation_configuration().await?;
            let rebuilt = configuration_from_snapshot(&saved)?
                .context("file bootstrap configuration snapshot")?;
            assert_eq!(
                by_name(&rebuilt.definition.components),
                by_name(&config.definition.components)
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let cleanup = core.shutdown().await;
        result?;
        cleanup?;
    }
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        Arc::new(PluginLifecycleManager::new(Arc::new(RwLock::new(plugins)))),
        directory.path().into(),
    ));
    let app = build_plugin_router(orchestrator, InstanceRegistry::new(), Arc::new(false));
    let (status, catalogue) = request(&app, "GET", "/computation", "", "application/json").await?;
    assert_eq!(status, StatusCode::OK);
    let bootstraps = catalogue["bootstrapFactories"]
        .as_array()
        .context("bootstrap catalogue")?;
    let file_bootstrap = bootstraps
        .iter()
        .find(|factory| factory["implementation"]["name"] == "drasi.sqlite/file-bootstrap")
        .context("SQLite file bootstrap in catalogue")?;
    assert_eq!(file_bootstrap["source_progress"], false);
    Ok(())
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
                crate::query_plugin::builder().with_secret_store_provider(secrets.clone()),
                instance,
                &settings,
                Some(&yaml.clone().into()),
                Arc::new(RwLock::new(registry()?)),
                Some(secrets),
            )
            .await?
            .build()
            .await?;
        core.start().await?;
        assert!(core.reconcile_desired_state().await?.converged());
        assert_eq!(
            core.get_query_results_by_name(&ComponentName::try_new("query")?)
                .await?
                .len(),
            expected
        );
        let saved = core.snapshot_computation_configuration().await?;
        let rebuilt = configuration_from_snapshot(&saved)?.context("native snapshot")?;
        assert_eq!(
            rebuilt.definition.resource_configurations,
            config.definition.resource_configurations
        );
        assert!(!serde_json::to_string(&saved)?.contains("fixture-only"));
        assert_eq!(
            by_name(&rebuilt.definition.components),
            by_name(&config.definition.components)
        );
        let mut bad = core.desired_configuration()?.desired.clone();
        provider_mut(&mut bad.topology).configuration_version = 999;
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
    bad.definition
        .resource_configurations
        .remove(&resource("progress"));
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
        json!({"kind":"qos","owner":"00000000-0000-0000-0000-0000000000a4","definition":channel,"path":directory.path().join("journal"),
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
        let core = crate::query_plugin::builder()
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
        // The provider and durable output start asynchronously; wait for convergence.
        let mut report = core.reconcile_desired_state().await?;
        for _ in 0..20 {
            if report.converged() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            report = core.reconcile_desired_state().await?;
        }
        assert!(report.converged(), "{report:?}");
        assert_eq!(
            core.get_query_results_by_name(&ComponentName::try_new("query")?)
                .await?
                .len(),
            3
        );
        let control = core.computation_control()?;
        control
            .quiesce_components(control.desired_snapshot().revision, GraphSelection::All)
            .await?;
        core.stop().await?;
        if !restart {
            let mut desired = core.desired_configuration()?.desired.clone();
            desired
                .topology
                .components
                .retain(|component| component.descriptor.id() != &id("bootstrap"));
            desired
                .topology
                .relationships
                .retain(|edge| edge.definition.from.component != id("bootstrap"));
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
            crate::query_plugin::builder()
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
        assert_eq!(
            core.get_query_results_by_name(&ComponentName::try_new("query")?)
                .await?
                .len(),
            count
        );
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
    fn plan_update(
        &self,
        resource: &ResourceId,
        previous: &DesiredTopology,
        desired: &DesiredTopology,
    ) -> Result<ComponentUpdatePlan> {
        self.inner.plan_update(resource, previous, desired)
    }

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
        json!({"kind":"qos","owner":"00000000-0000-0000-0000-0000000000a5","definition":journal_definition()?,"path":path,
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
    let core = crate::query_plugin::builder()
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
        .2
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
            crate::query_plugin::builder()
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
        if !allow_loss && native_consumer {
            desired
                .topology
                .resource_configurations
                .get_mut(&resource("delivery"))
                .context("consumer delivery recipe")?["maxStreams"] = json!(3);
        } else if !allow_loss {
            desired
                .topology
                .resources
                .iter_mut()
                .find(|spec| spec.id == resource("journal"))
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
                let restored = crate::query_plugin::builder()
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
        let mut builder = crate::query_plugin::builder()
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

        let restored = crate::query_plugin::builder()
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
        let core = crate::query_plugin::builder()
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
                    "input",
                    factory(COUNTER)
                        .specification(id("input"), json!({"stream":"input/out","count":4}))?,
                ),
                desired(
                    "transaction",
                    transaction.specification(
                        &registry,
                        resource("transformers"),
                        resource("indexes"),
                    )?,
                ),
                desired(
                    "capture",
                    factory(CAPTURE).specification(id("capture"), json!({"path":output}))?,
                ),
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
                json!({"kind":"qos","owner":"00000000-0000-0000-0000-0000000000a6","definition":channel,"path":directory.path().join("journal"),
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
    // An exactly-once consumer requires a restart-stable producer identity;
    // the tests inject envelopes under that identity.
    let source = plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == "fixture/durable-source")
        .context("durable source fixture")?;
    topology.components = vec![
        desired(
            "input",
            source.specification(id("input"), json!({"stream":"input/out"}))?,
        ),
        desired("consumer", sink),
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
        json!({"kind":"qos","owner":"00000000-0000-0000-0000-0000000000a7","definition":channel,"path":path.join("journal"),
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
        "construction_scope":INSTANCE, "graph_id":graph, "component_id":id("input"), "stream":"input/out",
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
        consumer
            .provider
            .create_indexes(graph, &id("consumer"))
            .await?,
    )?;
    let prefix = id("consumer")
        .as_str()
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let reference = ElementReference::new(&format!("transaction-step/{prefix}/values"), "count");
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
        crate::query_plugin::builder()
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
        let core = crate::query_plugin::builder()
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
        let core = crate::query_plugin::builder()
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
