// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate the canonical Server configuration from rebuilt native metadata.

use anyhow::{Context, Result};
use drasi_host_sdk::{
    computation::{load, NativePlugin},
    management::NativeBootstrapConfig,
};
use drasi_lib::computation::v1::*;
use drasi_server::{computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn id(value: &str) -> ComponentId {
    ComponentId::try_new(value).expect("constant id")
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
fn settings(key: &str) -> ConfigurationValue {
    ConfigurationValue::Reference {
        resource: resource("mssql-settings"),
        key: format!("env-json:{key}").into(),
        secret: true,
    }
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
        .context("SQL Server bootstrap")?
        .metadata();
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
        let bootstrap_id = format!("{name}-bootstrap");
        builder = declare(
            builder,
            &progress,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":name}),
        )?;
        let bootstrap_config = NativeBootstrapConfig {
            component: id(name),
            implementation: bootstrap.implementation.clone(),
            configuration_version: bootstrap.configuration_version,
            configuration: BTreeMap::from([
                ("source".into(), ConfigurationValue::Literal(json!("items"))),
                (
                    "stream".into(),
                    ConfigurationValue::Literal(json!(stream("items"))),
                ),
                ("settings".into(), settings("DRASI_MSSQL_SETTINGS")),
                (
                    "labels".into(),
                    ConfigurationValue::Literal(json!(["Items"])),
                ),
            ]),
            source_progress: Some(resource(&progress)),
            source_bootstrap: None,
        };
        let mut recipe = serde_json::to_value(bootstrap_config)?;
        recipe["kind"] = json!("nativeBootstrap");
        builder = declare(builder, &bootstrap_id, ResourceRole::Bootstrap, recipe)?
            .resource_dependency(
                resource(&bootstrap_id),
                resource(&progress),
                ResourceRole::Checkpoint,
            )?
            .resource_dependency(
                resource(&bootstrap_id),
                resource("mssql-settings"),
                ResourceRole::SecretStore,
            )?;
        let mut query = query_factory.specification_from_config(id(name),json!({
            "query":format!("MATCH (n:Items) {} RETURN n.id AS id,n.value AS value",if name=="high" {"WHERE n.value >= 20"} else {""}),
            "stream":stream(name),"language":"cypher","outbox_capacity":32,
        }))?;
        query.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("indexes")]),
            ("catalog".into(), vec![resource("results")]),
            ("source_progress".into(), vec![resource(&progress)]),
            ("bootstrap".into(), vec![resource(&bootstrap_id)]),
        ]);
        builder = builder
            .component(query, query_factory.clone())
            .bind_stream(endpoint(name, "out"), stream(name))
            .connect(
                EdgeDefinition::new(endpoint("items", "out"), endpoint(name, "in")),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            );
    }
    builder = declare(builder,"consumers",ResourceRole::Checkpoint,json!({
        "kind":"consumerProgress","producer":"items","consumers":{"all":"all-progress","high":"high-progress"},
    }))?.resource_dependency(resource("consumers"),resource("all-progress"),ResourceRole::Checkpoint)?
        .resource_dependency(resource("consumers"),resource("high-progress"),ResourceRole::Checkpoint)?;
    let mut source = source_factory
        .specification(id("items"), json!({"stream":stream("items"),"settings":{}}))?;
    source
        .configuration
        .insert("settings".into(), settings("DRASI_MSSQL_SETTINGS"));
    source
        .dependencies
        .insert("consumer_progress".into(), vec![resource("consumers")]);
    let mut sink = sink_factory.specification(id("effect"), json!({"settings":{}}))?;
    sink.configuration
        .insert("settings".into(), settings("DRASI_MSSQL_SINK_SETTINGS"));
    let mut outlet = outlet_factory.specification_from_config(id("query-results"), json!({}))?;
    outlet
        .dependencies
        .insert("catalog".into(), vec![resource("results")]);
    builder = builder
        .component(source, source_factory)
        .component(sink, sink_factory)
        .component(outlet, outlet_factory)
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
