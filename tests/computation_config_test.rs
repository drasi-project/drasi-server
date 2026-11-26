// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

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

fn component_id(value: u128) -> ComponentId {
    ComponentId::from_uuid(uuid::Uuid::from_u128(value))
}

fn input() -> Value {
    let query = component_id(1);
    let outlet = component_id(2);
    json!({"graph": {
        "resources": {
            "indexes": {"kind": "memoryIndexes"},
            "results": {"kind": "queryCatalog"}
        },
        "components": [
            {
                "id": query.clone(), "name": "query", "kind": "drasi/continuous-query", "autoStart": false,
                "resources": {"indexes": "indexes", "catalog": "results"},
                "config": {"query": "MATCH (n) RETURN n.value AS value"}
            },
            {
                "id": outlet.clone(), "name": "results-outlet", "kind": "drasi/query-results-outlet", "autoStart": false,
                "resources": {"catalog": ["results"]}
            }
        ],
        "connections": [{"from": query, "to": outlet}]
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
fn removed_completeness_switch_is_rejected() {
    for field in ["allowIncomplete", "allow_incomplete"] {
        let mut value = input();
        value["graph"][field] = true.into();
        let error = serde_json::from_value::<ComputationConfigInput>(value).unwrap_err();
        assert!(error.to_string().contains(field), "{error}");
    }
}

#[test]
fn readable_graph_uses_actual_contracts_and_round_trips_as_an_expanded_definition() -> Result<()> {
    let factories = factories()?;
    let config = resolve(input(), &factories)?;
    assert_eq!(config.definition.components.len(), 2);
    assert_eq!(config.definition.resources.len(), 2);
    assert_eq!(
        config.definition.graph_id.as_str(),
        ComponentBatch::GRAPH_ID
    );
    let query = &config.definition.components[0];
    let ComponentConstruction::Factory(spec) = &query.construction else {
        panic!("factory recipe");
    };
    assert_eq!(
        spec.configuration["stream"],
        ConfigurationValue::Literal(json!(format!("{}/out", component_id(1))))
    );
    assert_eq!(
        query.descriptor.ports()[1].schema(),
        QueryChangeCodec::schema().descriptor()
    );
    assert_eq!(
        query.streams[&PortId::try_new("out")?].as_str(),
        format!("{}/out", component_id(1))
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
    let mapped_yaml = serde_yaml::to_string(&serde_json::to_value(ComputationConfigInput::from(
        config.clone(),
    ))?)?;
    assert!(!mapped_yaml.contains("!Factory"));
    let mapped: ComputationConfigInput = serde_yaml::from_str(&mapped_yaml)?;
    assert_eq!(mapped.resolve(&factories)?.definition, config.definition);
    Ok(())
}

#[test]
fn explicit_instances_accept_computation_without_runtime_selectors() -> Result<()> {
    let single: DrasiServerConfig = serde_json::from_value(json!({
        "instances": [{"id": "readable", "computation": input()}]
    }))?;
    single.validate()?;
    let instances: DrasiServerConfig = serde_json::from_value(json!({
        "instances": [
            {"id": "first", "computation": input()},
            {"id": "second", "computation": input()}
        ]
    }))?;
    instances.validate()?;
    let mixed = serde_json::from_value::<DrasiServerConfig>(json!({
        "computation": input(), "instances": [{"id": "first"}]
    }));
    assert!(mixed
        .unwrap_err()
        .to_string()
        .contains("unknown field `computation`"));
    let validation = drasi_server::config::validate_with_plugins(&single, None);
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
        let config: DrasiServerConfig = serde_json::from_value(
            json!({"instances": [{"id": "readable", "computation": computation}]}),
        )?;
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
            bad["graph"]["components"][1]["id"] = json!(component_id(1));
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
            bad["graph"]["connections"][0]["from"] =
                json!({"component": component_id(1), "port": "in"});
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
    let config: DrasiServerConfig =
        serde_json::from_value(json!({"instances":[{"computation": bad}]}))?;
    assert!(drasi_server::config::validate_with_plugins(&config, None).has_errors());
    Ok(())
}

struct DescribingFactory {
    descriptor: FactoryDescriptor,
    ports: Vec<PortDescriptor>,
    creates: Arc<AtomicUsize>,
    primary: PortPrimaries,
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
            primary: PortPrimaries::default(),
        })
    }
}

#[async_trait]
impl ComponentFactory for DescribingFactory {
    fn describe(
        &self,
        _context: &drasi_lib::computation::v1::ConstructionContext,
    ) -> anyhow::Result<drasi_lib::computation::v1::ComponentDescription> {
        Ok(drasi_lib::computation::v1::ComponentDescription::undeclared())
    }
    fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }
    fn primary_ports(&self) -> PortPrimaries {
        self.primary.clone()
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
            "id": component_id(3), "name": "sensor/group", "kind": "test/source", "version": "1"
        }));
    value["graph"]["connections"]
        .as_array_mut()
        .context("connection array")?
        .push(json!({
            "from": component_id(3), "to": component_id(1), "capacity": 7
        }));
    let error = resolve(value.clone(), &factories)
        .err()
        .context("ambiguous output port")?;
    assert!(
        format!("{error:#}").contains("ambiguous Output port"),
        "{error:#}"
    );
    value["graph"]["connections"][1]["from"] =
        json!({"component": component_id(3), "port": "right"});
    let config = resolve(value, &factories)?;
    let edge = &config.definition.relationships[1];
    assert_eq!(edge.definition.from.component, component_id(3));
    assert_eq!(edge.definition.from.port.as_str(), "right");
    assert!(matches!(edge.pipe, DesiredPipe::Bounded { capacity: 7 }));
    assert_eq!(creates.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn readable_graph_uses_the_declared_primary_without_constructing_components() -> Result<()> {
    let creates = Arc::new(AtomicUsize::new(0));
    let mut factories = factories()?;
    let mut factory = DescribingFactory::new("1", &["audit", "data"], creates.clone())?;
    factory.primary = PortPrimaries::new(None, Some(PortId::try_new("data")?));
    factories.register(Arc::new(factory))?;
    let mut value = input();
    value["graph"]["components"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":component_id(4), "name":"sensor", "kind":"test/source", "version":"1"}));
    value["graph"]["connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":component_id(4), "to":component_id(1)}));
    let config = resolve(value, &factories)?;
    assert_eq!(
        config.definition.relationships[1]
            .definition
            .from
            .port
            .as_str(),
        "data"
    );
    assert_eq!(creates.load(Ordering::SeqCst), 0);
    Ok(())
}

fn graph_port(name: &str, direction: PortDirection, kind: PortKind) -> Result<PortDescriptor> {
    Ok(PortDescriptor::new(
        PortId::try_new(name)?,
        direction,
        GraphChangeCodec::schema().descriptor().clone(),
        PipeRequirements::default(),
    )
    .with_kind(kind))
}

/// A source with bootstrap ports and a bootstrap provider factory.
fn bootstrap_factories() -> Result<FactoryRegistry> {
    let creates = Arc::new(AtomicUsize::new(0));
    let mut registry = factories()?;
    let mut source = DescribingFactory::new("1", &[], creates.clone())?;
    source.ports = vec![
        graph_port("out", PortDirection::Output, PortKind::Data)?,
        graph_port("bootstrap", PortDirection::Input, PortKind::Bootstrap)?,
        graph_port("snapshot", PortDirection::Output, PortKind::Bootstrap)?,
    ];
    registry.register(Arc::new(source))?;
    let mut provider = DescribingFactory::new("1", &[], creates)?;
    provider.descriptor.implementation = ImplementationIdentity::try_new("test/provider", "1")?;
    provider.descriptor.role = ComponentRole::BootstrapProvider;
    provider.ports = vec![graph_port(
        "snapshot",
        PortDirection::Output,
        PortKind::Bootstrap,
    )?];
    registry.register(Arc::new(provider))?;
    Ok(registry)
}

/// `input()` plus a bootstrapping source feeding the query.
fn bootstrap_input() -> Value {
    let mut value = input();
    value["graph"]["components"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": component_id(10), "name": "sensor", "kind": "test/source", "version": "1",
            "autoStart": false,
            "bootstrapProvider": {"kind": "test/provider", "version": "1"}
        }));
    value["graph"]["connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from": component_id(10), "to": component_id(1)}));
    value
}

fn edges(config: &ComputationConfig) -> Vec<(ComponentId, String, ComponentId, String)> {
    config
        .definition
        .relationships
        .iter()
        .map(|relationship| {
            let edge = &relationship.definition;
            (
                edge.from.component.clone(),
                edge.from.port.to_string(),
                edge.to.component.clone(),
                edge.to.port.to_string(),
            )
        })
        .collect()
}

#[test]
fn source_bootstrap_providers_are_graph_nodes_and_edges() -> Result<()> {
    let factories = bootstrap_factories()?;
    let config = resolve(bootstrap_input(), &factories)?;
    let provider_id = ComponentId::derive_owned(&component_id(10), "bootstrap-provider")?;
    let provider = config
        .definition
        .components
        .iter()
        .find(|component| component.descriptor.id() == &provider_id)
        .context("generated provider node")?;
    assert_eq!(provider.role, ComponentRole::BootstrapProvider);
    assert_eq!(provider.name.as_str(), "sensor-bootstrap");
    assert!(!provider.lifecycle.auto_start);
    let found = edges(&config);
    for expected in [
        (
            provider_id.clone(),
            "snapshot",
            component_id(10),
            "bootstrap",
        ),
        (component_id(10), "out", component_id(1), "in"),
    ] {
        let expected = (
            expected.0,
            expected.1.to_owned(),
            expected.2,
            expected.3.to_owned(),
        );
        assert!(found.contains(&expected), "{expected:?} in {found:?}");
    }
    // DrasiLib, not the server, decides whether the query's data connection
    // carries its initial state.
    assert!(
        !found
            .iter()
            .any(|edge| edge.2 == component_id(1) && edge.3 == "bootstrap"),
        "{found:?}"
    );
    config.definition.build(TopologyBindings {
        factories: factories.clone(),
        ..Default::default()
    })?;

    // The expanded definition round-trips with the generated node and edges.
    let yaml = serde_yaml::to_string(&ComputationConfigInput::from(config.clone()))?;
    let restored: ComputationConfigInput = serde_yaml::from_str(&yaml)?;
    assert_eq!(restored.resolve(&factories)?.definition, config.definition);

    // The removed per-component switch is refused with guidance.
    let mut removed = bootstrap_input();
    removed["graph"]["components"][0]["enableBootstrap"] = json!(false);
    let error = resolve(removed.clone(), &factories)
        .err()
        .context("enableBootstrap is no longer a component field")?;
    assert!(
        format!("{error:#}").contains("unknown field `enableBootstrap`"),
        "{error:#}"
    );
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    std::fs::write(
        &path,
        serde_yaml::to_string(&json!({"instances": [{"id": "removed", "computation": removed}]}))?,
    )?;
    let error = drasi_server::load_config_file(&path)
        .err()
        .context("enableBootstrap in a configuration file")?;
    assert!(
        error
            .to_string()
            .contains("enableBootstrap has been removed"),
        "{error}"
    );
    Ok(())
}

#[test]
fn explicit_provider_components_and_bootstrap_connections_are_authored_like_other_nodes(
) -> Result<()> {
    let factories = bootstrap_factories()?;
    let mut value = bootstrap_input();
    value["graph"]["components"][2]
        .as_object_mut()
        .unwrap()
        .remove("bootstrapProvider");
    value["graph"]["components"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": component_id(11), "name": "sensor-snapshot", "kind": "test/provider",
            "version": "1", "autoStart": false
        }));
    // The provider has only a bootstrap output, so the connection kind is inferred.
    value["graph"]["connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from": component_id(11), "to": component_id(10)}));
    let config = resolve(value.clone(), &factories)?;
    assert!(edges(&config).contains(&(
        component_id(11),
        "snapshot".into(),
        component_id(10),
        "bootstrap".into()
    )));

    // A data connection cannot use bootstrap ports.
    value["graph"]["connections"][2]["kind"] = json!("data");
    let error = resolve(value.clone(), &factories)
        .err()
        .context("data connection from a provider")?;
    assert!(
        format!("{error:#}").contains("use a bootstrap connection"),
        "{error:#}"
    );

    // A component referenced as a provider must be a bootstrap provider.
    value["graph"]["connections"][2]["kind"] = json!("bootstrap");
    value["graph"]["components"][0]["bootstrapProvider"] = json!(component_id(2));
    let error = resolve(value, &factories)
        .err()
        .context("non-provider bootstrapProvider")?;
    assert!(
        format!("{error:#}").contains("not a bootstrap provider"),
        "{error:#}"
    );
    Ok(())
}

#[test]
fn local_query_bootstrap_provider_is_exclusive() -> Result<()> {
    let factories = bootstrap_factories()?;
    let mut value = bootstrap_input();
    value["graph"]["components"][0]["bootstrapProvider"] =
        json!({"id": component_id(12), "kind": "test/provider", "version": "1"});
    let config = resolve(value.clone(), &factories)?;
    let query_bootstrap: Vec<_> = edges(&config)
        .into_iter()
        .filter(|edge| edge.2 == component_id(1) && edge.3 == "bootstrap")
        .collect();
    assert_eq!(
        query_bootstrap,
        vec![(
            component_id(12),
            "snapshot".into(),
            component_id(1),
            "bootstrap".into()
        )],
        "the query never asks its inputs for bootstrap data"
    );

    value["graph"]["connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from": component_id(10), "to": component_id(1), "kind": "bootstrap"}));
    let error = resolve(value, &factories)
        .err()
        .context("input bootstrap with a local provider")?;
    assert!(format!("{error:#}").contains("exclusive"), "{error:#}");
    Ok(())
}

#[test]
fn invalid_bootstrap_authoring_has_clear_errors() -> Result<()> {
    let factories = bootstrap_factories()?;
    let mut no_input = bootstrap_input();
    no_input["graph"]["components"][1]["bootstrapProvider"] =
        json!({"kind": "test/provider", "version": "1"});
    let error = resolve(no_input, &factories)
        .err()
        .context("provider on a sink")?;
    assert!(
        format!("{error:#}").contains("has no bootstrap input"),
        "{error:#}"
    );

    let mut unknown = bootstrap_input();
    unknown["graph"]["components"][2]["bootstrapProvider"] = json!(component_id(99));
    let error = resolve(unknown, &factories)
        .err()
        .context("unknown provider")?;
    assert!(
        format!("{error:#}").contains("unknown bootstrapProvider"),
        "{error:#}"
    );

    let mut port = bootstrap_input();
    port["graph"]["connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "from": {"component": component_id(10), "port": "out"},
            "to": component_id(1), "kind": "bootstrap"
        }));
    let error = resolve(port, &factories)
        .err()
        .context("data port in a bootstrap connection")?;
    assert!(format!("{error:#}").contains("bootstrap"), "{error:#}");
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
        component_id(5),
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
        crate::query_plugin::builder()
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

#[path = "query_plugin/mod.rs"]
mod query_plugin;
