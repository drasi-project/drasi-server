// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::computation::v1::*;
use drasi_server::{
    computation::{ComputationConfig, ComputationConfigInput},
    DrasiServerConfig,
};
use serde_json::{json, Value};

fn input() -> Value {
    json!({"graph": {
        "allowIncomplete": true,
        "resources": {
            "indexes": {"kind": "memoryIndexes"},
            "results": {"kind": "queryCatalog"}
        },
        "components": [
            {
                "id": "query", "kind": "drasi/continuous-query", "autoStart": false,
                "resources": {"indexes": "indexes", "catalog": "results"},
                "config": {"query": "MATCH (n) RETURN n.value AS value"}
            },
            {
                "id": "results-outlet", "kind": "drasi/query-results-outlet", "autoStart": false,
                "resources": {"catalog": ["results"]}
            }
        ],
        "connections": [{"from": "query", "to": "results-outlet"}]
    }})
}

fn factories() -> Result<FactoryRegistry> {
    let mut registry = FactoryRegistry::default();
    registry.register(Arc::new(ContinuousQueryFactory::default()))?;
    registry.register(Arc::new(QueryResultsOutletFactory::default()))?;
    Ok(registry)
}

fn resolve(value: Value, factories: &FactoryRegistry) -> Result<ComputationConfig> {
    let input: ComputationConfigInput = serde_json::from_value(value)?;
    Ok(input.resolve(factories)?.into_owned())
}

#[test]
fn readable_graph_uses_actual_contracts_and_round_trips_as_an_expanded_definition() -> Result<()> {
    let factories = factories()?;
    let config = resolve(input(), &factories)?;
    assert_eq!(config.definition.components.len(), 2);
    assert_eq!(config.definition.resources.len(), 2);
    assert_eq!(config.definition.graph_id, "__drasi_lib_runtime__");
    let query = &config.definition.components[0];
    let ComponentConstruction::Factory(spec) = &query.construction else {
        panic!("factory recipe");
    };
    assert_eq!(
        spec.configuration["stream"],
        ConfigurationValue::Literal(json!("query/out"))
    );
    assert_eq!(
        query.descriptor.ports()[1].schema(),
        QueryChangeCodec::schema().descriptor()
    );
    assert_eq!(
        query.streams[&PortId::try_new("out")?].as_str(),
        "query/out"
    );
    assert!(!query.lifecycle.auto_start);
    assert!(matches!(
        config.definition.relationships[0].pipe,
        DesiredPipe::Bounded { capacity: 32 }
    ));
    let outlet = &config.definition.components[1];
    assert_eq!(outlet.completion, Some(SinkCompletion::Handled));
    assert_eq!(
        outlet.descriptor.ports()[0].schema(),
        QueryChangeCodec::schema().descriptor()
    );
    config.definition.build(TopologyBindings {
        factories: factories.clone(),
        ..Default::default()
    })?;

    let yaml = serde_yaml::to_string(&ComputationConfigInput::from(config.clone()))?;
    assert!(yaml.starts_with("definition:"));
    assert!(!yaml.contains("!Definition"));
    let restored: ComputationConfigInput = serde_yaml::from_str(&yaml)?;
    assert_eq!(restored.resolve(&factories)?.definition, config.definition);
    let old: ComputationConfig = serde_yaml::from_str(&yaml)?;
    assert_eq!(old.definition, config.definition);
    Ok(())
}

#[test]
fn root_and_per_instance_input_are_supported_without_runtime_selectors() -> Result<()> {
    let root: DrasiServerConfig = serde_json::from_value(json!({
        "id": "readable", "computation": input()
    }))?;
    root.validate()?;
    let instances: DrasiServerConfig = serde_json::from_value(json!({
        "instances": [
            {"id": "first", "computation": input()},
            {"id": "second", "computation": input()}
        ]
    }))?;
    instances.validate()?;
    let mixed: DrasiServerConfig = serde_json::from_value(json!({
        "computation": input(), "instances": [{"id": "first"}]
    }))?;
    assert!(mixed.validate().is_err());
    let validation = drasi_server::config::validate_with_plugins(&root, None);
    assert!(
        validation.config_errors.is_empty(),
        "{:?}",
        validation.config_errors
    );
    Ok(())
}

#[test]
fn validator_reports_computation_components_for_both_input_formats() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plugins = directory.path().join("plugins");
    std::fs::create_dir(&plugins)?;
    for (name, computation) in [
        ("graph", input()),
        (
            "definition",
            serde_json::to_value(resolve(input(), &factories()?)?)?,
        ),
    ] {
        let path = directory.path().join(format!("{name}.yaml"));
        let config: DrasiServerConfig =
            serde_json::from_value(json!({"id": "readable", "computation": computation}))?;
        std::fs::write(&path, serde_yaml::to_string(&config)?)?;
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"))
            .arg("--config")
            .arg(&path)
            .arg("--plugins-dir")
            .arg(&plugins)
            .arg("validate")
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            output.status.success(),
            "{name}: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("computation graph for instance 'readable' (2 component(s))"));
        assert!(stdout.contains("Summary: 1 instance(s), 2 computation component(s)"));
        assert!(!stdout.contains("no components to validate"));
    }
    Ok(())
}

#[test]
fn unknown_mixed_and_invalid_fields_do_not_silently_default() -> Result<()> {
    let expanded = resolve(input(), &factories()?)?;
    let mut mixed = input();
    mixed["definition"] = serde_json::to_value(&expanded.definition)?;
    assert!(serde_json::from_value::<ComputationConfigInput>(mixed).is_err());
    assert!(serde_json::from_value::<ComputationConfigInput>(json!({})).is_err());
    for bad in [
        json!({"graph": null}),
        json!({"graph": {"components": [], "typo": true}}),
        json!({"graph": {"components": [{"id": "x", "kind": "x", "typo": true}]}}),
    ] {
        assert!(
            serde_json::from_value::<ComputationConfigInput>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    let mut bad = input();
    bad["graph"]["connections"][0]["capacity"] = json!(0);
    let bad: ComputationConfigInput = serde_json::from_value(bad)?;
    assert!(bad.validate().is_err());
    let mut bad = input();
    bad["graph"]["defaultPipeCapacity"] = json!(0);
    let bad: ComputationConfigInput = serde_json::from_value(bad)?;
    assert!(bad.validate().is_err());
    for kind in [
        "memoryIndexes",
        "queryCatalog",
        "middleware",
        "queryMiddleware",
        "transactionalTransformers",
        "bridgeIngressControl",
        "bridgeEgressControl",
        "configuration",
    ] {
        let mut bad = input();
        bad["graph"]["resources"]["indexes"] = json!({"kind": kind, "typo": true});
        let bad: ComputationConfigInput = serde_json::from_value(bad)?;
        assert!(
            format!("{:#}", bad.validate().err().context("invalid recipe")?)
                .contains("unknown field"),
            "{kind} must not silently accept unknown settings"
        );
    }
    let mut bad = input();
    bad["graph"]["components"][0]["config"] = json!([]);
    let bad: ComputationConfigInput = serde_json::from_value(bad)?;
    assert!(bad.validate().is_err());
    let repeated = "graph:\n  components: []\n  resources:\n    indexes: {kind: memoryIndexes}\n    indexes: {kind: queryCatalog}\n";
    assert!(serde_yaml::from_str::<ComputationConfigInput>(repeated).is_err());
    Ok(())
}

#[test]
fn invalid_components_resources_connections_and_catalogs_are_rejected() -> Result<()> {
    let factories = factories()?;
    for bad in [
        {
            let mut bad = input();
            bad["graph"]["components"][1]["id"] = json!("query");
            bad
        },
        {
            let mut bad = input();
            bad["graph"]["components"][0]["resources"]["indexes"] = json!("absent");
            bad
        },
        {
            let mut bad = input();
            bad["graph"]["connections"][0]["from"] = json!("absent");
            bad
        },
        {
            let mut bad = input();
            bad["graph"]["connections"][0]["from"] = json!({"component": "query", "port": "in"});
            bad
        },
        {
            let mut bad = input();
            bad["graph"]["resources"]["other-results"] = json!({"kind": "queryCatalog"});
            bad["graph"]["components"][1]["resources"]["catalog"] = json!("other-results");
            bad
        },
    ] {
        assert!(resolve(bad.clone(), &factories).is_err(), "{bad}");
    }
    let mut bad = input();
    bad["graph"]["components"][0]["config"]["unexpected"] = json!(true);
    let root: DrasiServerConfig = serde_json::from_value(json!({"computation": bad}))?;
    assert!(drasi_server::config::validate_with_plugins(&root, None).has_errors());
    Ok(())
}

struct DescribingFactory {
    descriptor: FactoryDescriptor,
    ports: Vec<PortDescriptor>,
    creates: Arc<AtomicUsize>,
}

impl DescribingFactory {
    fn new(version: &str, outputs: &[&str], creates: Arc<AtomicUsize>) -> Result<Self> {
        Ok(Self {
            descriptor: FactoryDescriptor {
                implementation: ImplementationIdentity::try_new("test/source", version)?,
                role: ComponentRole::Source,
                configuration_version: 1,
                configuration: ConfigurationSchema::default(),
                dependencies: BTreeMap::new(),
            },
            ports: outputs
                .iter()
                .map(|port| {
                    Ok(PortDescriptor::new(
                        PortId::try_new(*port)?,
                        PortDirection::Output,
                        GraphChangeCodec::schema().descriptor().clone(),
                        PipeRequirements::default(),
                    ))
                })
                .collect::<Result<_>>()?,
            creates,
        })
    }
}

#[async_trait]
impl ComponentFactory for DescribingFactory {
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }
    fn specification_from_config(
        &self,
        id: ComponentId,
        _: Value,
    ) -> Result<ComponentSpecification> {
        Ok(ComponentSpecification {
            descriptor: ComponentDescriptor::try_new(id, self.ports.clone())?,
            role: self.descriptor.role,
            completion: None,
            implementation: self.descriptor.implementation.clone(),
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        })
    }
    fn validate(&self, _: &ComponentSpecification) -> Result<()> {
        Ok(())
    }
    async fn create(
        &self,
        _: ConstructionContext,
    ) -> std::result::Result<ConstructedComponent, ComponentCreationError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Err(ComponentCreationError::terminal(anyhow::anyhow!(
            "metadata discovery must not create components"
        )))
    }
}

#[test]
fn factory_versions_and_multi_port_endpoints_must_be_unambiguous() -> Result<()> {
    let creates = Arc::new(AtomicUsize::new(0));
    let mut factories = factories()?;
    factories.register(Arc::new(DescribingFactory::new(
        "1",
        &["left", "right"],
        creates.clone(),
    )?))?;
    factories.register(Arc::new(DescribingFactory::new(
        "2",
        &["left", "right"],
        creates.clone(),
    )?))?;
    assert!(factories.get_by_name("test/source", None).is_err());
    assert!(factories
        .get_by_name("test/source", Some("absent"))
        .is_err());
    assert_eq!(
        factories
            .get_by_name("test/source", Some("1"))?
            .descriptor()
            .implementation
            .version
            .as_ref(),
        "1"
    );
    let mut value = input();
    value["graph"]["components"]
        .as_array_mut()
        .context("component array")?
        .push(json!({
            "id": "sensor/group", "kind": "test/source", "version": "1"
        }));
    value["graph"]["connections"]
        .as_array_mut()
        .context("connection array")?
        .push(json!({
            "from": "sensor/group", "to": "query", "capacity": 7
        }));
    assert!(resolve(value.clone(), &factories)
        .err()
        .context("ambiguous output port")?
        .to_string()
        .contains("multiple Output ports"));
    value["graph"]["connections"][1]["from"] =
        json!({"component": "sensor/group", "port": "right"});
    let config = resolve(value, &factories)?;
    let edge = &config.definition.relationships[1];
    assert_eq!(edge.definition.from.component.as_str(), "sensor/group");
    assert_eq!(edge.definition.from.port.as_str(), "right");
    assert!(matches!(edge.pipe, DesiredPipe::Bounded { capacity: 7 }));
    assert_eq!(creates.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn transaction_query_description_uses_the_transaction_input_schema() -> Result<()> {
    let factory = ContinuousQueryFactory::default();
    let execution = QueryExecutionSettings {
        source_transactions: Some(SourceTransactionLimits {
            max_changes: std::num::NonZeroUsize::new(100).context("change limit")?,
            max_bytes: std::num::NonZeroUsize::new(1024 * 1024).context("byte limit")?,
            max_duration_ms: std::num::NonZeroU64::new(1000).context("duration limit")?,
        }),
        ..Default::default()
    };
    let specification = factory.specification_from_config(
        ComponentId::try_new("transaction-query")?,
        json!({"query": "MATCH (n) RETURN n", "execution": execution}),
    )?;
    assert_eq!(
        specification.descriptor.ports()[0].schema(),
        SourceTransactionCodec::schema().descriptor()
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn readable_component_api_preserves_canonical_snapshots_and_duplicate_checks() -> Result<()> {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use drasi_server::{
        api::v1::routes::build_v1_router, computation::configuration_from_snapshot,
        instance_registry::InstanceRegistry, plugin_registry::PluginRegistry,
    };
    use tower::ServiceExt;

    let core = Arc::new(
        drasi_lib::DrasiLib::builder()
            .with_id("readable-api")
            .build()
            .await?,
    );
    let instances = InstanceRegistry::new();
    instances
        .add("readable-api".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(
        instances,
        Arc::new(false),
        None,
        Arc::new(tokio::sync::RwLock::new(PluginRegistry::new())),
        None,
    );
    let body = serde_yaml::to_string(&input())?;
    for expected in [StatusCode::OK, StatusCode::CONFLICT] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/instances/readable-api/computation/components")
                    .header("content-type", "application/yaml")
                    .body(Body::from(body.clone()))?,
            )
            .await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&bytes));
    }
    let snapshot = core.snapshot_computation_configuration().await?;
    let canonical = configuration_from_snapshot(&snapshot)?.context("native snapshot")?;
    assert_eq!(canonical.definition.components.len(), 2);
    assert!(canonical
        .definition
        .components
        .iter()
        .all(|component| !component.lifecycle.auto_start));
    let exported = serde_json::to_value(canonical)?;
    assert!(exported.get("definition").is_some());
    assert!(exported.get("graph").is_none());
    core.shutdown().await?;
    Ok(())
}
