// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate the canonical Server configuration from rebuilt native metadata.

use anyhow::{Context, Result};
use drasi_host_sdk::computation::{load, NativeBootstrapNodeFactory, NativePlugin};
use drasi_lib::computation::v1::*;
use drasi_server::{computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn id(value: &str) -> ComponentId {
    let value = match value {
        "items" => 1,
        "all" => 2,
        "high" => 3,
        "effect" => 4,
        "query-results" => 5,
        "items-bootstrap" => 6,
        other => panic!("missing explicit component ID for {other}"),
    };
    ComponentId::from_uuid(uuid::Uuid::from_u128(value))
}
fn resource(value: &str) -> ResourceId {
    ResourceId::try_new(value).expect("constant resource")
}
fn stream(value: &str) -> StreamId {
    StreamId::try_new(format!("{value}/out")).expect("constant stream")
}
fn endpoint(value: &str, port: &str) -> Endpoint {
    Endpoint::new(id(value), PortId::try_new(port).expect("constant port"))
}
/// Source and bootstrap fields supplied per deployment, each as one JSON
/// environment value named `DRASI_MSSQL_<FIELD>`.
pub const SOURCE_FIELDS: &[&str] = &[
    "host",
    "port",
    "database",
    "user",
    "password",
    "encryption",
    "trustServerCertificate",
    "tables",
    "barrierTable",
];
/// Sink fields supplied per deployment as `DRASI_MSSQL_SINK_<FIELD>`.
pub const SINK_FIELDS: &[&str] = &[
    "host",
    "port",
    "database",
    "user",
    "password",
    "encryption",
    "trustServerCertificate",
    "defaultTemplate",
];
/// The environment variable that supplies `field`, for example
/// `trustServerCertificate` under `DRASI_MSSQL` is
/// `DRASI_MSSQL_TRUST_SERVER_CERTIFICATE`.
pub fn variable(prefix: &str, field: &str) -> String {
    let mut name = format!("{prefix}_");
    for character in field.chars() {
        if character.is_ascii_uppercase() {
            name.push('_');
        }
        name.push(character.to_ascii_uppercase());
    }
    name
}
fn references(
    prefix: &str,
    fields: &'static [&'static str],
) -> impl Iterator<Item = (std::sync::Arc<str>, ConfigurationValue)> {
    let prefix = prefix.to_owned();
    fields.iter().map(move |field| {
        (
            (*field).into(),
            ConfigurationValue::Reference {
                resource: resource("mssql-settings"),
                key: format!("env-json:{}", variable(&prefix, field)).into(),
                secret: *field == "password",
            },
        )
    })
}
/// Valid stand-ins for the referenced connection fields, used only while the
/// factory validates the specification; every one is replaced by a reference.
fn placeholders() -> Value {
    json!({
        "host":"localhost", "port":1433, "database":"placeholder", "user":"placeholder",
        "password":"placeholder", "encryption":"required", "trustServerCertificate":false,
    })
}
fn bootstrap_edge(
    builder: ComponentBatchBuilder,
    from: Endpoint,
    to: Endpoint,
) -> ComponentBatchBuilder {
    builder.connect(
        EdgeDefinition::new(from, to),
        Box::new(BoundedPipeConfig { capacity: 8 }),
    )
}
fn declare(
    builder: ComponentBatchBuilder,
    name: &str,
    role: ResourceRole,
    config: Value,
) -> Result<ComponentBatchBuilder> {
    Ok(builder
        .declare_resource(ResourceSpecification {
            id: resource(name),
            role,
            ownership: if role == ResourceRole::SecretStore {
                ResourceOwnership::Borrowed
            } else {
                ResourceOwnership::Graph
            },
            binding: name.into(),
        })?
        .resource_configuration(resource(name), config)?)
}
pub fn plugins(directory: &Path) -> Result<Vec<Arc<NativePlugin>>> {
    ["source_mssql", "bootstrap_mssql", "sink_storedproc_mssql"]
        .into_iter()
        .map(|package| {
            let path = directory.join(format!(
                "{}drasi_computation_{package}{}",
                std::env::consts::DLL_PREFIX,
                std::env::consts::DLL_SUFFIX
            ));
            load(&path)
                .with_context(|| format!("load rebuilt native SQL Server {}", path.display()))
        })
        .collect()
}
pub fn generate(libraries: &Path) -> Result<DrasiServerConfig> {
    let plugins = plugins(libraries)?;
    let source_factory = plugins[0]
        .factories()
        .iter()
        .find(|f| f.metadata().implementation.name.as_ref() == "drasi.mssql/changes")
        .context("SQL Server source")?
        .clone();
    let bootstrap = plugins[1]
        .bootstrap_factories()
        .iter()
        .find(|f| f.metadata().implementation.name.as_ref() == "drasi.mssql/bootstrap")
        .cloned()
        .context("SQL Server bootstrap")?;
    let bootstrap = Arc::new(NativeBootstrapNodeFactory::new(bootstrap));
    let sink_factory = plugins[2]
        .factories()
        .iter()
        .find(|f| f.metadata().implementation.name.as_ref() == "drasi.mssql/storedproc")
        .context("SQL Server sink")?
        .clone();
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    let mut builder = declare(
        ComponentBatch::builder(),
        "mssql-settings",
        ResourceRole::SecretStore,
        json!({"kind":"configuration"}),
    )?;
    builder = declare(
        builder,
        "indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"memoryIndexes"}),
    )?;
    builder = declare(
        builder,
        "results",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    )?;
    for name in ["all", "high"] {
        let progress = format!("{name}-progress");
        builder = declare(
            builder,
            &progress,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":id(name),"indexes":"indexes"}),
        )?
        .resource_dependency(
            resource(&progress),
            resource("indexes"),
            ResourceRole::IndexBackend,
        )?;
        let mut query = query_factory.specification_from_config(id(name),json!({
            "query":format!("MATCH (n:Items) {} RETURN n.id AS id,n.value AS value",if name=="high" {"WHERE n.value >= 20"} else {""}),
            "stream":stream(name),"language":"cypher","outbox_capacity":32,
        }))?;
        query.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("indexes")]),
            ("catalog".into(), vec![resource("results")]),
            ("source_progress".into(), vec![resource(&progress)]),
        ]);
        builder = builder
            .component(ComponentName::try_new(name)?, query, query_factory.clone())
            .bind_stream(endpoint(name, "out"), stream(name))
            .connect(
                EdgeDefinition::new(endpoint("items", "out"), endpoint(name, "in")),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            );
        builder = bootstrap_edge(
            builder,
            endpoint("items", "snapshot"),
            endpoint(name, "bootstrap"),
        );
    }
    // One visible provider node serves the source, which relays its queries'
    // bootstrap sessions; it reads each query's progress owner.
    let provider = ComponentSpecification {
        descriptor: NativeBootstrapNodeFactory::component_descriptor(id("items-bootstrap"))?,
        role: ComponentRole::BootstrapProvider,
        completion: None,
        implementation: bootstrap.descriptor().implementation.clone(),
        configuration_version: bootstrap.descriptor().configuration_version,
        configuration: BTreeMap::from([
            (
                "source".into(),
                ConfigurationValue::Literal(json!(id("items"))),
            ),
            (
                "stream".into(),
                ConfigurationValue::Literal(json!(stream("items"))),
            ),
            (
                "labels".into(),
                ConfigurationValue::Literal(json!(["Items"])),
            ),
        ])
        .into_iter()
        .chain(references("DRASI_MSSQL", SOURCE_FIELDS))
        .collect(),
        dependencies: if bootstrap
            .descriptor()
            .dependencies
            .contains_key("source_progress")
        {
            BTreeMap::from([(
                "source_progress".into(),
                vec![resource("all-progress"), resource("high-progress")],
            )])
        } else {
            BTreeMap::new()
        },
    };
    builder = builder.component(
        ComponentName::try_new("items-bootstrap")?,
        provider,
        bootstrap,
    );
    builder = bootstrap_edge(
        builder,
        endpoint("items-bootstrap", "snapshot"),
        endpoint("items", "bootstrap"),
    );
    builder = declare(builder,"consumers",ResourceRole::Checkpoint,json!({
        "kind":"consumerProgress","producer":id("items"),"consumers":{id("all").as_str():"all-progress",id("high").as_str():"high-progress"},
    }))?.resource_dependency(resource("consumers"),resource("all-progress"),ResourceRole::Checkpoint)?
        .resource_dependency(resource("consumers"),resource("high-progress"),ResourceRole::Checkpoint)?;
    let mut configuration = placeholders();
    configuration["stream"] = json!(stream("items"));
    configuration["tables"] = json!(["dbo.Items"]);
    configuration["barrierTable"] = json!("dbo.DrasiBarrier");
    let mut source = source_factory.specification(id("items"), configuration)?;
    source
        .configuration
        .extend(references("DRASI_MSSQL", SOURCE_FIELDS));
    source
        .dependencies
        .insert("consumer_progress".into(), vec![resource("consumers")]);
    let mut configuration = placeholders();
    configuration["defaultTemplate"] = json!({"added":{"template":"SELECT 1"}});
    let mut sink = sink_factory.specification(id("effect"), configuration)?;
    sink.configuration
        .extend(references("DRASI_MSSQL_SINK", SINK_FIELDS));
    builder = declare(
        builder,
        "delivery-indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"memoryIndexes"}),
    )?;
    builder = declare(
        builder,
        "delivery",
        ResourceRole::IndexBackend,
        json!({"kind":"nativeConsumer","failureScope":"processRestart","maxStreams":2,"receiptsPerStream":64}),
    )?
    .resource_dependency(
        resource("delivery"),
        resource("delivery-indexes"),
        ResourceRole::IndexBackend,
    )?;
    sink.dependencies
        .insert("consumer".into(), vec![resource("delivery")]);
    let mut outlet = outlet_factory.specification_from_config(id("query-results"), json!({}))?;
    outlet
        .dependencies
        .insert("catalog".into(), vec![resource("results")]);
    builder = builder
        .component(ComponentName::try_new("items")?, source, source_factory)
        .component(ComponentName::try_new("effect")?, sink, sink_factory)
        .component(
            ComponentName::try_new("query-results")?,
            outlet,
            outlet_factory,
        )
        .bind_stream(endpoint("items", "out"), stream("items"))
        .connect(
            EdgeDefinition::new(endpoint("all", "out"), endpoint("effect", "in")),
            Box::new(BoundedPipeConfig { capacity: 8 }),
        );
    for name in ["all", "high"] {
        builder = builder.connect(
            EdgeDefinition::new(endpoint(name, "out"), endpoint("query-results", "in")),
            Box::new(BoundedPipeConfig { capacity: 8 }),
        );
    }
    let mut config: DrasiServerConfig = serde_json::from_value(json!({
        "apiVersion":"drasi.io/v1","host":"127.0.0.1","port":8080,"persistConfig":false,
        "autoInstallPlugins":false,"instances":[{"id":"native-mssql","persistIndex":false}],
    }))?;
    config.instances[0].computation = Some(
        ComputationConfig {
            definition: builder.build()?.definition,
        }
        .into(),
    );
    config.validate()?;
    Ok(config)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "usage: native-mssql <rebuilt-native-library-directory> <output.yaml>"
    );
    let config = generate(Path::new(&args[0]))?;
    std::fs::write(&args[1], serde_yaml::to_string(&config)?)?;
    Ok(())
}
