// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate a server YAML configuration from the real native factory contracts.
//! Usage: cargo run --example native-computation -- <plugin-directory> [capture.jsonl]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use drasi_host_sdk::computation::NativeFactory;
use drasi_lib::computation::v1::{
    BoundedPipeConfig, ComponentBatch, ComponentId, EdgeDefinition, Endpoint, PortId, StreamId,
};
use drasi_server::{
    api::models::ConfigValue, computation::ComputationConfig, config::DrasiServerConfig,
};
use serde_json::json;

fn factory(directory: &Path, library: &str, name: &str) -> Result<Arc<NativeFactory>> {
    let path = directory.join(format!(
        "{}{library}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX,
    ));
    let plugin = drasi_host_sdk::computation::load(&path)
        .with_context(|| format!("load native component {}", path.display()))?;
    anyhow::ensure!(
        plugin.factories().len() == 1,
        "expected one factory in {}",
        path.display()
    );
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
    let directory = PathBuf::from(
        args.next()
            .context("usage: native-computation <plugin-directory> [capture.jsonl]")?,
    );
    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("native-output.jsonl"));
    anyhow::ensure!(args.next().is_none(), "too many arguments");
    let counter = factory(
        &directory,
        "drasi_computation_source_counter",
        "drasi.standard/volatile-counter",
    )?;
    let middleware = factory(
        &directory,
        "drasi_computation_transformer_middleware",
        "drasi.standard/middleware",
    )?;
    let arithmetic = factory(
        &directory,
        "drasi_computation_transformer_arithmetic",
        "drasi.standard/arithmetic",
    )?;
    let capture = factory(
        &directory,
        "drasi_computation_sink_capture",
        "drasi.standard/capture",
    )?;
    let mut builder = ComponentBatch::builder()
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
        host: ConfigValue::Static("127.0.0.1".into()),
        verify_plugins: false,
        enable_ui: false,
        persist_config: false,

        instances: vec![drasi_server::config::DrasiLibInstanceConfig {
            id: ConfigValue::Static("native-example".into()).into(),
            computation: Some(
                ComputationConfig {
                    definition: graph.definition,
                }
                .into(),
            ),
            ..Default::default()
        }],
        ..Default::default()
    };
    config.validate()?;
    println!("{}", serde_yaml::to_string(&config)?);
    Ok(())
}
