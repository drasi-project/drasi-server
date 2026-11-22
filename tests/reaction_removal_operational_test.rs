// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

#![cfg(unix)]
#![allow(
    clippy::print_stdout,
    reason = "Server output is printed to diagnose operational qualification failures"
)]

use anyhow::{Context, Result};
use drasi_lib::{ComponentId, StateStoreProvider, StorePartition};
use drasi_state_store_redb::RedbStateStoreProvider;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::process::{Child, Command};

const DEADLINE: Duration = Duration::from_secs(20);

fn port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

async fn launch(config: &Path, log: &Path, api: u16) -> Result<Child> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join("target/debug/drasi-server"))
        .arg("--config")
        .arg(config)
        .arg("--plugins-dir")
        .arg(
            config
                .parent()
                .context("private plugin directory")?
                .join("plugins"),
        )
        .current_dir(config.parent().context("private configuration directory")?)
        .stdout(Stdio::from(std::fs::File::create(log)?))
        .stderr(Stdio::from(std::fs::File::create(
            log.with_extension("stderr"),
        )?))
        .kill_on_drop(true)
        .spawn()?;
    let client = Client::new();
    let readiness = tokio::time::timeout(DEADLINE, async {
        loop {
            anyhow::ensure!(
                child.try_wait()?.is_none(),
                "Server exited before readiness"
            );
            if client
                .get(format!("http://127.0.0.1:{api}/health"))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if !matches!(&readiness, Ok(Ok(()))) {
        println!("Readiness stdout:\n{}", std::fs::read_to_string(log)?);
        println!(
            "Readiness stderr:\n{}",
            std::fs::read_to_string(log.with_extension("stderr"))?
        );
    }
    readiness??;
    Ok(child)
}

async fn shutdown(child: &mut Child, log: &Path) -> Result<()> {
    let pid = child.id().context("owned Server process")?;
    anyhow::ensure!(
        Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status()
            .await?
            .success(),
        "SIGINT delivery failed"
    );
    let status = tokio::time::timeout(DEADLINE, child.wait()).await??;
    println!("Server stdout:\n{}", std::fs::read_to_string(log)?);
    println!(
        "Server stderr:\n{}",
        std::fs::read_to_string(log.with_extension("stderr"))?
    );
    anyhow::ensure!(status.success(), "Server shutdown failed: {status}");
    Ok(())
}

async fn request(
    client: &Client,
    api: u16,
    method: Method,
    path: &str,
    body: Value,
) -> Result<Value> {
    let response = client
        .request(
            method.clone(),
            format!("http://127.0.0.1:{api}/api/v1/instances/qualification/{path}"),
        )
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await?;
    println!("{method} {path}: {status} {body}");
    anyhow::ensure!(status.is_success(), "{method} {path}: {status} {body}");
    Ok(body)
}

async fn reaction(
    client: &Client,
    api: u16,
    name: &str,
    kind: &str,
    sse: u16,
) -> Result<ComponentId> {
    let mut body = json!({"kind":kind, "id":name, "queries":["values"], "autoStart":false});
    if kind == "sse" {
        body["host"] = json!("127.0.0.1");
        body["port"] = json!(sse);
        body["defaultTemplate"] = json!({
            "added":{"template":"QUALIFIED {{after.value}}"},
            "updated":{"template":"QUALIFIED {{after.value}}"}
        });
    } else {
        body["routes"] = json!({"values":{"added":{"template":"QUALIFIED {{after.value}}"},
            "updated":{"template":"QUALIFIED {{after.value}}"}}});
    }
    request(client, api, Method::POST, "reactions", body).await?;
    let full = request(
        client,
        api,
        Method::GET,
        &format!("reactions/{name}?view=full"),
        Value::Null,
    )
    .await?;
    Ok(ComponentId::try_new(
        full["data"]["id"].as_str().context("reaction UUID")?,
    )?)
}

async fn lifecycle(client: &Client, api: u16, action: &str) -> Result<()> {
    let paths = [
        "sources/input",
        "queries/values",
        "reactions/stream",
        "reactions/logger",
        "reactions/unrelated",
    ];
    for path in paths {
        request(
            client,
            api,
            Method::POST,
            &format!("{path}/{action}"),
            Value::Null,
        )
        .await?;
    }
    Ok(())
}

async fn stream_event(response: &mut reqwest::Response) -> Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let mut bytes = Vec::new();
        loop {
            bytes.extend_from_slice(
                &response
                    .chunk()
                    .await?
                    .context("SSE closed before an update")?,
            );
            if String::from_utf8_lossy(&bytes).lines().any(|line| {
                line.strip_prefix("data: QUALIFIED ")
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|value| value > 0)
            }) {
                println!(
                    "Observed actual SSE update: {}",
                    String::from_utf8_lossy(&bytes)
                );
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await?
}

#[tokio::test]
async fn matched_server_removes_sse_and_log_checkpoints_and_recreates_public_names() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(120), async {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("server.json");
        let plugins = directory.path().join("plugins");
        std::fs::create_dir(&plugins)?;
        for library in [
            "drasi_source_mock",
            "drasi_reaction_log",
            "drasi_reaction_sse",
            "drasi_computation_transformer_query",
        ] {
            let file = format!(
                "{}{library}{}",
                std::env::consts::DLL_PREFIX,
                std::env::consts::DLL_SUFFIX
            );
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/debug/plugins")
                    .join(&file),
                plugins.join(file),
            )?;
        }
        let state = directory.path().join("state.redb");
        let log = directory.path().join("run.log");
        let api = port()?;
        let sse = port()?;
        std::fs::write(
            &config,
            serde_json::to_vec_pretty(&json!({
                "apiVersion":"drasi.io/v1", "host":"127.0.0.1", "port":api,
                "logLevel":"info", "persistConfig":true, "verifyPlugins":false,
                "autoInstallPlugins":false, "enableUi":false,
                "instances":[{"id":"qualification",
                    "stateStore":{"kind":"redb","path":state},
                    "sources":[],"queries":[],"reactions":[]}]
            }))?,
        )?;
        let client = Client::builder().timeout(DEADLINE).build()?;
        let mut server = launch(&config, &log, api).await?;
        request(
            &client,
            api,
            Method::POST,
            "sources",
            json!({"kind":"mock","id":"input","autoStart":false,
                "dataType":{"type":"counter"},"intervalMs":100}),
        )
        .await?;
        request(
            &client,
            api,
            Method::POST,
            "queries",
            json!({
                "name":"values", "query":"MATCH (n:Counter) RETURN n.value AS value",
                "queryLanguage":"Cypher", "inputs":["input"], "enableBootstrap":false,
                "autoStart":false
            }),
        )
        .await?;
        let stream = reaction(&client, api, "stream", "sse", sse).await?;
        let logger = reaction(&client, api, "logger", "log", sse).await?;
        let unrelated = reaction(&client, api, "unrelated", "log", sse).await?;
        // Independent never-started destruction must not depend on worker startup.
        for kind in ["sse", "log"] {
            reaction(&client, api, "never-started", kind, sse).await?;
            request(
                &client,
                api,
                Method::DELETE,
                "reactions/never-started",
                Value::Null,
            )
            .await?;
        }
        lifecycle(&client, api, "start").await?;
        let mut connection = client
            .get(format!("http://127.0.0.1:{sse}/events"))
            .send()
            .await?;
        anyhow::ensure!(connection.status().is_success(), "SSE endpoint not ready");
        stream_event(&mut connection).await?;
        drop(connection);
        tokio::time::timeout(DEADLINE, async {
            while !std::fs::read_to_string(&log)?.lines().any(|line| {
                line.split_once("QUALIFIED ")
                    .and_then(|(_, value)| value.trim().parse::<u64>().ok())
                    .is_some_and(|value| value > 0)
            }) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        lifecycle(&client, api, "stop").await?;
        lifecycle(&client, api, "start").await?;
        let mut connection = client
            .get(format!("http://127.0.0.1:{sse}/events"))
            .send()
            .await?;
        stream_event(&mut connection).await?;
        request(
            &client,
            api,
            Method::DELETE,
            "reactions/stream",
            Value::Null,
        )
        .await?;
        tokio::time::timeout(DEADLINE, async {
            while connection.chunk().await?.is_some() {}
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        drop(connection);
        request(
            &client,
            api,
            Method::POST,
            "reactions/logger/stop",
            Value::Null,
        )
        .await?;
        request(
            &client,
            api,
            Method::DELETE,
            "reactions/logger",
            Value::Null,
        )
        .await?;
        let listed = request(&client, api, Method::GET, "reactions", Value::Null).await?;
        let items = listed["data"].as_array().context("reaction list")?;
        assert!(!items
            .iter()
            .any(|item| item["name"] == "stream" || item["name"] == "logger"));
        let saved = std::fs::read_to_string(&config)?;
        anyhow::ensure!(
            !saved.contains(&stream.to_string()) && !saved.contains(&logger.to_string()),
            "removed identities persisted"
        );
        let recreated_stream = reaction(&client, api, "stream", "sse", sse).await?;
        let recreated_logger = reaction(&client, api, "logger", "log", sse).await?;
        assert_ne!(recreated_stream, stream);
        assert_ne!(recreated_logger, logger);
        request(
            &client,
            api,
            Method::POST,
            "sources/input/stop",
            Value::Null,
        )
        .await?;
        request(
            &client,
            api,
            Method::POST,
            "queries/values/stop",
            Value::Null,
        )
        .await?;
        request(
            &client,
            api,
            Method::POST,
            "reactions/unrelated/stop",
            Value::Null,
        )
        .await?;
        shutdown(&mut server, &log).await?;
        let store = RedbStateStoreProvider::new(&state)?;
        for id in [&stream, &logger] {
            assert!(store
                .list_keys(&StorePartition::component(id.clone()))
                .await?
                .is_empty());
        }
        let mut checkpoints = Vec::new();
        for id in [&unrelated] {
            let partition = StorePartition::component(id.clone());
            let keys = store.list_keys(&partition).await?;
            anyhow::ensure!(
                !keys.is_empty(),
                "actual persistent checkpoint missing for {id}"
            );
            for key in keys {
                checkpoints.push((
                    partition.clone(),
                    key.clone(),
                    store.get(&partition, &key).await?,
                ));
            }
        }
        let sentinel = StorePartition::component(unrelated.clone());
        store
            .set(&sentinel, "unrelated-sentinel", vec![7, 8, 9])
            .await?;
        store.sync().await?;
        drop(store);
        server = launch(&config, &log, api).await?;
        // Restoring stopped configurations must preserve checkpoint values.
        shutdown(&mut server, &log).await?;
        let store = RedbStateStoreProvider::new(&state)?;
        for (partition, key, value) in checkpoints {
            assert_eq!(store.get(&partition, &key).await?, value);
        }
        drop(store);
        server = launch(&config, &log, api).await?;
        lifecycle(&client, api, "start").await?;
        let mut connection = client
            .get(format!("http://127.0.0.1:{sse}/events"))
            .send()
            .await?;
        anyhow::ensure!(
            connection.status().is_success(),
            "SSE endpoint not ready after restart"
        );
        stream_event(&mut connection).await?;
        request(
            &client,
            api,
            Method::DELETE,
            "reactions/stream",
            Value::Null,
        )
        .await?;
        tokio::time::timeout(DEADLINE, async {
            while connection.chunk().await?.is_some() {}
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        drop(connection);
        request(
            &client,
            api,
            Method::POST,
            "reactions/logger/stop",
            Value::Null,
        )
        .await?;
        request(
            &client,
            api,
            Method::DELETE,
            "reactions/logger",
            Value::Null,
        )
        .await?;
        let listed = request(&client, api, Method::GET, "reactions", Value::Null).await?;
        let items = listed["data"].as_array().context("reaction list")?;
        assert!(!items
            .iter()
            .any(|item| item["name"] == "stream" || item["name"] == "logger"));
        let saved = std::fs::read_to_string(&config)?;
        anyhow::ensure!(
            !saved.contains(&stream.to_string()) && !saved.contains(&logger.to_string()),
            "removed identities persisted in configuration"
        );
        let new_stream = reaction(&client, api, "stream", "sse", sse).await?;
        let new_logger = reaction(&client, api, "logger", "log", sse).await?;
        assert_ne!(new_stream, stream);
        assert_ne!(new_logger, logger);
        shutdown(&mut server, &log).await?;
        let store = RedbStateStoreProvider::new(&state)?;
        for id in [&stream, &logger] {
            assert!(store
                .list_keys(&StorePartition::component(id.clone()))
                .await?
                .is_empty());
        }
        assert_eq!(
            store.get(&sentinel, "unrelated-sentinel").await?,
            Some(vec![7, 8, 9])
        );
        store.sync().await?;
        drop(store);
        server = launch(&config, &log, api).await?;
        for (name, id) in [("stream", new_stream), ("logger", new_logger)] {
            let restored = request(
                &client,
                api,
                Method::GET,
                &format!("reactions/{name}?view=full"),
                Value::Null,
            )
            .await?;
            assert_eq!(restored["data"]["id"], id.to_string());
        }
        shutdown(&mut server, &log).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await?
}
