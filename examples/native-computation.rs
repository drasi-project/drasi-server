// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate a server YAML configuration from the real native factory contracts.
//! Usage: cargo run --example native-computation -- <library> [capture.jsonl]

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use drasi_host_sdk::computation::{NativeFactory, NativePlugin};
use drasi_lib::computation::v1::{
    BoundedPipeConfig, ComponentId, ComputationGraph, EdgeDefinition, Endpoint, PortId, StreamId,
};
use drasi_server::{
    api::models::ConfigValue, computation::ComputationGraphConfig, config::DrasiServerConfig,
};
use serde_json::json;

fn factory(plugin: &NativePlugin, name: &str) -> Result<Arc<NativeFactory>> {
    plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == name)
        .cloned()
        .with_context(|| format!("native factory '{name}' is not provided by this library"))
}

fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(
        ComponentId::try_new(component)?,
        PortId::try_new(port)?,
    ))
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let library = args
        .next()
        .context("usage: native-computation <native-standard-library> [capture.jsonl]")?;
    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("native-output.jsonl"));
    anyhow::ensure!(args.next().is_none(), "too many arguments");
    let plugin = drasi_host_sdk::computation::load(PathBuf::from(library))?;
    let counter = factory(&plugin, "drasi.standard/volatile-counter")?;
    let middleware = factory(&plugin, "drasi.standard/middleware")?;
    let arithmetic = factory(&plugin, "drasi.standard/arithmetic")?;
    let capture = factory(&plugin, "drasi.standard/capture")?;
    let mut builder = ComputationGraph::builder("native-pipeline")
        .component(counter.specification(ComponentId::try_new("counter")?,
            json!({"stream":"counter/out","count":4,"start":2,"step":3}))?, counter)
        .component(middleware.specification(ComponentId::try_new("middleware")?, json!({
            "stream":"middleware/out",
            "middleware":[{"name":"relabel","kind":"relabel","config":{"labelMappings":{"Counter":"Projected"}}}],
            "pipeline":["relabel"],
        }))?, middleware)
        .component(arithmetic.specification(ComponentId::try_new("arithmetic")?,
            json!({"stream":"arithmetic/out","add":10,"multiply":2}))?, arithmetic)
        .component(capture.specification(ComponentId::try_new("capture")?, json!({"path":path}))?, capture);
    for (from, to) in [
        ("counter", "middleware"),
        ("middleware", "arithmetic"),
        ("arithmetic", "capture"),
    ] {
        builder = builder
            .connect(
                EdgeDefinition::new(endpoint(from, "out")?, endpoint(to, "in")?),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            )
            .bind_stream(
                endpoint(from, "out")?,
                StreamId::try_new(format!("{from}/out"))?,
            );
    }
    let graph = builder.build()?;
    let config = DrasiServerConfig {
        id: ConfigValue::Static("native-example".into()),
        host: ConfigValue::Static("127.0.0.1".into()),
        verify_plugins: false,
        enable_ui: false,
        persist_config: false,
        computation_graphs: vec![ComputationGraphConfig {
            auto_start: true,
            definition: graph.configuration_snapshot()?.topology,
        }],
        ..Default::default()
    };
    config.validate()?;
    println!("{}", serde_yaml::to_string(&config)?);
    Ok(())
}
