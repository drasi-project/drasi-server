// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#[path = "../../drasi-core/components/host-sdk/tests/fixtures/native_mssql/database.rs"]
#[allow(dead_code)]
mod database;
#[allow(dead_code)]
#[path = "../examples/native-mssql.rs"]
mod generator;

use anyhow::{ensure, Context, Result};
use drasi_lib::{
    computation::v1::FactoryRegistry, secret_store::MemorySecretStoreProvider, DrasiLib,
};
use drasi_server::{
    computation::register_components, plugin_registry::PluginRegistry, DrasiServerConfig,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn libraries() -> PathBuf {
    std::env::var_os("DRASI_NATIVE_PLUGIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
}

#[test]
fn canonical_server_configuration_uses_three_native_roles_and_secret_references() -> Result<()> {
    let config = generator::generate(&libraries())?;
    let yaml = serde_yaml::to_string(&config)?;
    let decoded: DrasiServerConfig = serde_yaml::from_str(&yaml)?;
    decoded.validate()?;
    ensure!(
        yaml.contains("drasi.mssql/changes")
            && yaml.contains("drasi.mssql/bootstrap")
            && yaml.contains("drasi.mssql/storedproc"),
        "SQL Server native family missing"
    );
    ensure!(
        yaml.contains("env-json:DRASI_MSSQL_SETTINGS")
            && yaml.contains("env-json:DRASI_MSSQL_SINK_SETTINGS"),
        "SQL Server secret references lost"
    );
    ensure!(
        !yaml.contains("password:"),
        "literal credential field leaked into Server configuration"
    );
    Ok(())
}

async fn real_server() -> Result<()> {
    let mut database = database::Database::new().await?;
    let config = generator::generate(&libraries())?;
    let json =
        serde_json::to_string(&config)?.replace("env-json:DRASI_MSSQL", "secret-json:DRASI_MSSQL");
    let config: DrasiServerConfig = serde_json::from_str(&json)?;
    let source = database.settings();
    let sink = json!({"connection":source["connection"],"outputs":{"all":{"component":"all","port":"out"}},
        "default_template":{"added":{"template":database.scoped_sql("EXEC {{schema}}.ApplyEffect {{param operation}},{{param after.id}},{{param after.value}}")}}});
    let secrets = Arc::new(
        MemorySecretStoreProvider::new()
            .with_secret("DRASI_MSSQL_SETTINGS", source.to_string())
            .with_secret("DRASI_MSSQL_SINK_SETTINGS", sink.to_string()),
    );
    let mut registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut registry);
    for plugin in generator::plugins(&libraries())? {
        registry.register_computation_plugin(plugin)?;
    }
    let core = DrasiLib::builder()
        .with_id("native-mssql")
        .with_secret_store_provider(secrets)
        .build()
        .await?;
    let definition = config.instances[0]
        .computation
        .as_ref()
        .context("Server computation config missing")?
        .resolve(&FactoryRegistry::default())?
        .into_owned();
    let proof = async {
        register_components(&definition, &core, &registry)
            .await?
            .ensure_committed()?;
        core.start().await?;
        for query in ["all", "high"] {
            tokio::time::timeout(
                Duration::from_secs(30),
                core.computation_component(query)?.wait_started(),
            )
            .await??;
        }
        let schema = core
            .get_source_schema("items")
            .await?
            .context("Server/public source schema missing")?;
        ensure!(
            schema.nodes.iter().any(|node| node.label == "Items"),
            "Server source inspection did not reach native catalog"
        );
        ensure!(
            core.get_query_results("all").await?.len() == 2,
            "Server snapshot missing"
        );
        database.sql("INSERT INTO {{schema}}.Items VALUES(3,30,N'Server');")?;
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                if core.get_query_results("all").await?.len() == 3
                    && core.get_query_results("high").await?.len() == 2
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await??;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = core.shutdown().await;
    let removed = database.remove();
    proof?;
    stopped?;
    removed?;
    Ok(())
}

#[tokio::test]
async fn current_thread_server_consumes_real_sql_server() -> Result<()> {
    real_server().await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_thread_server_consumes_real_sql_server() -> Result<()> {
    real_server().await
}
