// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::{
    computation::v1::{ConfigurationValue, ResourceId},
    DrasiLib,
};
use drasi_server::{
    api::v1::routes::build_v1_router,
    computation::{configuration_from_snapshot, register_components, ComputationConfig},
    instance_registry::InstanceRegistry,
    plugin_registry::PluginRegistry,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{oneshot, RwLock};

#[allow(dead_code)]
#[path = "../examples/native-sqlite.rs"]
mod recipe;

const DEADLINE: Duration = Duration::from_secs(10);

async fn reject_mismatched_pairings(
    config: &ComputationConfig,
    plugins: &PluginRegistry,
    database: &std::path::Path,
) -> Result<()> {
    let coordinator = ResourceId::try_new("source-bootstrap")?;
    let bootstrap = ResourceId::try_new("first-snapshot")?;
    let mut different_config = config.clone();
    different_config
        .definition
        .resource_configurations
        .get_mut(&coordinator)
        .context("coordinator recipe")?["configuration"]["stream"] =
        serde_json::to_value(ConfigurationValue::Literal(json!("different/out")))?;
    let mut two_modes = config.clone();
    two_modes
        .definition
        .resource_configurations
        .get_mut(&coordinator)
        .context("coordinator recipe")?["sourceProgress"] = json!("first-progress");
    let mut different_owner = config.clone();
    different_owner
        .definition
        .resource_configurations
        .get_mut(&bootstrap)
        .context("bootstrap recipe")?["sourceProgress"] = json!("second-progress");
    let mut unpaired = config.clone();
    unpaired
        .definition
        .resource_configurations
        .get_mut(&bootstrap)
        .context("bootstrap recipe")?
        .as_object_mut()
        .context("bootstrap object")?
        .remove("sourceBootstrap");
    for invalid in [different_config, two_modes, different_owner, unpaired] {
        let core = DrasiLib::builder()
            .with_id(recipe::INSTANCE)
            .build()
            .await?;
        let constructed =
            drasi_server::computation::build_components_with_registry(&invalid, &core, plugins)
                .await;
        let cleanup = core.shutdown().await;
        anyhow::ensure!(
            constructed.is_err(),
            "invalid paired Server definition was accepted"
        );
        cleanup?;
        let tables: u64 = rusqlite::Connection::open(database)?.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name LIKE '__drasi_%'",
            [],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            tables == 0,
            "rejected configuration initialized the SQLite journal"
        );
    }
    Ok(())
}

async fn results(
    client: &reqwest::Client,
    base: &str,
    queries: &[&str],
    expected: &[Value],
) -> Result<()> {
    tokio::time::timeout(DEADLINE, async {
        loop {
            let mut complete = true;
            for query in queries {
                let body: Value = client
                    .get(format!(
                        "{base}/instances/{}/queries/{query}/results",
                        recipe::INSTANCE,
                    ))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                let mut rows = body["data"]
                    .as_array()
                    .context("Server query result data")?
                    .clone();
                rows.sort_by_key(|row| row["id"].as_i64());
                complete &= rows == expected;
            }
            if complete {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("Server queries did not observe SQLite changes")?
}

async fn native_sqlite_server(mode: recipe::Mode) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("source.db");
    rusqlite::Connection::open(&database)?.execute_batch(
        "CREATE TABLE items(id INTEGER PRIMARY KEY,value TEXT NOT NULL); INSERT INTO items VALUES(1,'snapshot');",
    )?;
    let libraries = std::env::var_os("DRASI_SERVER_TEST_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let source_port = std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port();
    let indexes = directory.path().join("indexes");
    let config = recipe::definition_with_mode(
        &libraries,
        &database,
        source_port,
        mode,
        (mode != recipe::Mode::File).then_some(indexes.as_path()),
    )?;
    let roundtrip: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
    anyhow::ensure!(
        roundtrip.definition == config.definition,
        "canonical SQLite definition changed in YAML"
    );
    let mut plugins = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugins);
    for package in [
        "drasi-computation-source-sqlite",
        "drasi-computation-bootstrap-sqlite",
    ] {
        plugins.register_computation_plugin(recipe::load(&libraries, package)?)?;
    }
    if mode == recipe::Mode::Shared {
        reject_mismatched_pairings(&config, &plugins, &database).await?;
    }
    let plugins = Arc::new(RwLock::new(plugins));
    let mut current = roundtrip;
    for round in 0..2 {
        let core = Arc::new(
            DrasiLib::builder()
                .with_id(recipe::INSTANCE)
                .build()
                .await?,
        );
        let setup = async {
            let registry = plugins.read().await;
            register_components(&current, &core, &registry).await?;
            drop(registry);
            core.start().await?;
            let components = std::iter::once("source".to_owned()).chain(
                mode.queries()
                    .iter()
                    .flat_map(|name| [(*name).to_owned(), format!("{name}-results")]),
            );
            for component in components {
                tokio::time::timeout(
                    DEADLINE,
                    core.computation_component(&component)?.wait_started(),
                )
                .await??;
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = setup {
            core.shutdown().await?;
            return Err(error);
        }
        let instances = InstanceRegistry::new();
        instances
            .add(recipe::INSTANCE.into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let router = build_v1_router(
            instances.clone(),
            Arc::new(false),
            None,
            plugins.clone(),
            None,
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let base = format!("http://{}", listener.local_addr()?);
        let source = format!("http://127.0.0.1:{source_port}");
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        let client = reqwest::Client::builder().timeout(DEADLINE).build()?;
        let result = async {
            let schema = core
                .get_source_schema("source")
                .await?
                .context("Server native source schema")?;
            anyhow::ensure!(
                schema.nodes.len() == 1 && schema.nodes[0].label == "items",
                "Server-configured source did not expose actual SQLite schema"
            );
            let graph_schema = core.get_graph_schema().await?;
            anyhow::ensure!(
                graph_schema
                    .nodes
                    .get("items")
                    .is_some_and(|node| node.sources.contains("source")),
                "Server instance inspection omitted native SQLite"
            );
            if round == 0 {
                results(
                    &client,
                    &base,
                    mode.queries(),
                    &[json!({"id":1,"value":"snapshot"})],
                )
                .await?;
                client
                    .post(format!("{source}/api/tables/items"))
                    .json(&json!({"id":2,"value":"live"}))
                    .send()
                    .await?
                    .error_for_status()?;
                results(
                    &client,
                    &base,
                    mode.queries(),
                    &[
                        json!({"id":1,"value":"snapshot"}),
                        json!({"id":2,"value":"live"}),
                    ],
                )
                .await?;
                client
                    .put(format!("{source}/api/tables/items/2"))
                    .json(&json!({"id":20,"value":"rekey"}))
                    .send()
                    .await?
                    .error_for_status()?;
                client
                    .delete(format!("{source}/api/tables/items/1"))
                    .send()
                    .await?
                    .error_for_status()?;
                results(
                    &client,
                    &base,
                    mode.queries(),
                    &[json!({"id":20,"value":"rekey"})],
                )
                .await?;
                let failed = client
                    .post(format!("{source}/api/batch"))
                    .json(&json!({"operations":[
                        {"op":"insert","table":"items","data":{"id":3,"value":"must-rollback"}},
                        {"op":"insert","table":"items","data":{"id":20,"value":"duplicate"}}
                    ]}))
                    .send()
                    .await?;
                anyhow::ensure!(
                    failed.status() == reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                    "failed SQLite batch was acknowledged"
                );
                anyhow::ensure!(
                    client
                        .get(format!("{source}/api/tables/items/3"))
                        .send()
                        .await?
                        .status()
                        == reqwest::StatusCode::NOT_FOUND,
                    "rolled-back batch left a database prefix"
                );
            } else {
                results(
                    &client,
                    &base,
                    mode.queries(),
                    &[json!({"id":20,"value":"rekey"})],
                )
                .await?;
                client
                    .post(format!("{source}/api/tables/items"))
                    .json(&json!({"id":30,"value":"after-restart"}))
                    .send()
                    .await?
                    .error_for_status()?;
                results(
                    &client,
                    &base,
                    mode.queries(),
                    &[
                        json!({"id":20,"value":"rekey"}),
                        json!({"id":30,"value":"after-restart"}),
                    ],
                )
                .await?;
            }
            let saved = core.snapshot_computation_configuration().await?;
            let saved =
                configuration_from_snapshot(&saved)?.context("SQLite configuration snapshot")?;
            anyhow::ensure!(
                saved.definition.resource_configurations
                    == config.definition.resource_configurations,
                "Server changed native bootstrap resource configuration"
            );
            Ok::<_, anyhow::Error>(saved)
        }
        .await;
        let _ = stop.send(());
        let http_cleanup = tokio::time::timeout(DEADLINE, server).await;
        let cleanup = core.shutdown().await;
        instances.remove(recipe::INSTANCE).await;
        current = result?;
        http_cleanup???;
        cleanup?;
        anyhow::ensure!(
            std::net::TcpListener::bind(("127.0.0.1", source_port)).is_ok(),
            "SQLite REST listener survived Server shutdown"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_sqlite_server_current_thread() -> Result<()> {
    native_sqlite_server(recipe::Mode::File).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sqlite_server_multi_thread() -> Result<()> {
    native_sqlite_server(recipe::Mode::File).await
}

#[tokio::test(flavor = "current_thread")]
async fn native_sqlite_coordinated_server_current_thread() -> Result<()> {
    for mode in [recipe::Mode::Single, recipe::Mode::Shared] {
        native_sqlite_server(mode).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sqlite_coordinated_server_multi_thread() -> Result<()> {
    for mode in [recipe::Mode::Single, recipe::Mode::Shared] {
        native_sqlite_server(mode).await?;
    }
    Ok(())
}
