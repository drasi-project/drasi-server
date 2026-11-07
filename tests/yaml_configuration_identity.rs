// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{path::PathBuf, sync::Arc};

use drasi_lib::{
    config::{ComponentId, ComponentName},
    management::{
        ConfigurationStore, DesiredInstance, ManagementError, OrdinaryComponent,
        OrdinaryComponentDefinition,
    },
    DrasiLib,
};
use drasi_server::persistence::configuration_store::YamlConfigurationStore;

fn definition(name: &str) -> OrdinaryComponentDefinition {
    OrdinaryComponentDefinition {
        id: None,
        name: ComponentName::try_new(name).unwrap(),
        auto_start: false,
        component: OrdinaryComponent::Source {
            kind: "unavailable-fixture".into(),
            configuration: serde_json::json!({"setting": "retained-before-construction"}),
        },
    }
}

#[tokio::test]
async fn accepted_updates_are_durable_and_acknowledgment_keeps_receipts_and_snapshots(
) -> anyhow::Result<()> {
    use drasi_lib::{
        computation::v1::{ComponentUpdateData, ComponentUpdateMode, ComponentUpdatePlan},
        management::{
            AcceptedComponentUpdate, ComponentUpdateCompletion, ComponentUpdateDefinition,
        },
    };
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("updates.yaml");
    let before = definition("source").with_assigned_id(ComponentId::generate());
    let mut after = before.clone();
    after.component = OrdinaryComponent::Source {
        kind: "unavailable-fixture".into(),
        configuration: serde_json::json!({"setting": "updated"}),
    };
    let desired = DesiredInstance {
        ordinary: vec![after.clone()],
        ..Default::default()
    };
    let update = AcceptedComponentUpdate {
        component_id: before.id.clone(),
        operation_id: "accepted-update".into(),
        plan: ComponentUpdatePlan {
            mode: ComponentUpdateMode::Restart,
            data: ComponentUpdateData::Preserve,
        },
        current: ComponentUpdateDefinition::Ordinary(before.clone()),
        proposed: ComponentUpdateDefinition::Ordinary(after),
    };
    let receipt = {
        let store = YamlConfigurationStore::new(path.clone(), true)?;
        let session = store.open("instance").await?;
        let initial = session
            .commit(
                0,
                "initial",
                &DesiredInstance {
                    ordinary: vec![before],
                    ..Default::default()
                },
                None,
                &[],
            )
            .await?;
        let receipt = session
            .commit(
                initial.revision,
                "update",
                &desired,
                None,
                &[update.clone()],
            )
            .await?;
        assert_eq!(
            session.snapshot("pending").await?.pending_updates,
            vec![update.clone()]
        );
        session.close().await?;
        receipt
    };
    {
        let store = YamlConfigurationStore::new(path.clone(), false)?;
        let session = store.open("instance").await?;
        assert_eq!(session.load().await?.pending_updates, vec![update.clone()]);
        let mut conflicting = update.clone();
        conflicting.operation_id = "different-operation".into();
        assert!(session
            .commit(1, "update", &desired, None, &[conflicting.clone()])
            .await
            .is_err());
        assert!(session
            .commit(
                receipt.revision,
                "second-update",
                &desired,
                None,
                &[conflicting]
            )
            .await
            .is_err());
        let completion = ComponentUpdateCompletion {
            component_id: update.component_id.clone(),
            operation_id: update.operation_id.clone(),
        };
        let wrong = ComponentUpdateCompletion {
            operation_id: "wrong".into(),
            ..completion.clone()
        };
        assert!(session
            .complete_updates(receipt.revision, &[completion.clone(), wrong])
            .await
            .is_err());
        assert_eq!(session.load().await?.pending_updates, vec![update.clone()]);
        assert!(session
            .complete_updates(0, &[completion.clone()])
            .await
            .is_err());
        let completed = session
            .complete_updates(receipt.revision, &[completion.clone()])
            .await?;
        assert_eq!(completed.desired, desired);
        assert_eq!(completed.revision, receipt.revision);
        assert!(completed.pending_updates.is_empty());
        assert_eq!(
            session
                .complete_updates(receipt.revision, &[completion])
                .await?,
            completed
        );
        assert_eq!(session.receipt("update").await?, Some(receipt.clone()));
        assert_eq!(
            session
                .load_snapshot("pending")
                .await?
                .unwrap()
                .pending_updates,
            vec![update.clone()]
        );
        session.close().await?;
    }
    let store = YamlConfigurationStore::new(path, false)?;
    let session = store.open("instance").await?;
    assert!(session.load().await?.pending_updates.is_empty());
    assert_eq!(
        session
            .commit(1, "update", &desired, None, &[update])
            .await?,
        receipt
    );
    assert!(session.load().await?.pending_updates.is_empty());
    session.close().await?;
    Ok(())
}

#[tokio::test]
async fn configured_file_restores_ids_and_only_real_snapshots_authorize_reuse() -> anyhow::Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("configuration.yaml");
    let provider = Arc::new(YamlConfigurationStore::new(path.clone(), true)?);
    let core = DrasiLib::builder()
        .with_id("instance")
        .with_configuration_store(provider.clone())
        .build()
        .await?;
    assert!(!core.allow_external_component_ids());
    let original = definition("orders");
    let receipt = core
        .create_managed_ordinary_component(0, "create-orders", original.clone())
        .await?;
    let id = receipt.creation.as_ref().unwrap().components[0].id.clone();
    assert!(receipt.durable);
    assert_eq!(core.desired_configuration()?.desired.ordinary[0].id, id);
    core.snapshot_desired_configuration("original").await?;

    let mut renamed = core.desired_configuration()?;
    renamed.desired.ordinary[0].name = ComponentName::try_new("renamed")?;
    core.apply_desired_state(renamed.revision, "rename", renamed.desired)
        .await?;
    core.shutdown().await?;
    drop(core);
    drop(provider);

    let provider = Arc::new(YamlConfigurationStore::new(path, false)?);
    let core = DrasiLib::builder()
        .with_id("instance")
        .with_configuration_store(provider.clone())
        .build()
        .await?;
    let current = core.desired_configuration()?;
    assert_eq!(current.desired.ordinary[0].id, id);
    assert_eq!(current.desired.ordinary[0].name.as_str(), "renamed");
    assert_eq!(
        core.configuration_receipt("create-orders").await?.unwrap(),
        receipt
    );
    let snapshot = core.load_configuration_snapshot("original").await?.unwrap();
    core.apply_desired_state(current.revision, "remove", DesiredInstance::default())
        .await?;

    let retried = core
        .create_managed_ordinary_component(0, "create-orders", original)
        .await?;
    assert_eq!(retried, receipt);
    assert!(core.desired_configuration()?.desired.ordinary.is_empty());
    let revision = core.desired_configuration()?.revision;
    assert!(core
        .apply_desired_state(revision, "copied-record", snapshot.desired)
        .await
        .is_err());
    core.restore_configuration_snapshot("original", revision, "restore")
        .await?;
    assert_eq!(core.desired_configuration()?.desired.ordinary[0].id, id);
    assert_eq!(
        core.desired_configuration()?.desired.ordinary[0]
            .name
            .as_str(),
        "orders"
    );
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn ownership_close_and_receipt_conflicts_survive_reopen() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("configuration.yaml");
    let provider = YamlConfigurationStore::new(path.clone(), true)?;
    let session = provider.open("instance").await?;
    assert!(provider.open("instance").await.is_err());
    assert!(YamlConfigurationStore::new(path.clone(), false).is_err());
    let mut desired = DesiredInstance::default();
    desired
        .ordinary
        .push(definition("first").with_assigned_id(ComponentId::generate()));
    let receipt = session.commit(0, "request", &desired, None, &[]).await?;
    assert_eq!(
        session.commit(0, "request", &desired, None, &[]).await?,
        receipt
    );
    let mut changed = desired.clone();
    changed.ordinary[0].name = ComponentName::try_new("second")?;
    let conflict = session
        .commit(0, "request", &changed, None, &[])
        .await
        .unwrap_err();
    assert!(matches!(
        conflict.downcast_ref::<ManagementError>(),
        Some(ManagementError::RequestConflict)
    ));
    session.snapshot("saved").await?;
    session.close().await?;
    let replacement = provider.open("instance").await?;
    session.close().await?;
    assert!(
        provider.open("instance").await.is_err(),
        "closing an old session must not release its replacement"
    );
    assert!(session.load().await.is_err());
    assert_eq!(
        replacement.load_snapshot("saved").await?.unwrap().desired,
        desired
    );
    replacement.close().await?;
    drop(replacement);
    drop(session);
    drop(provider);
    let reopened = YamlConfigurationStore::new(path, false)?;
    let session = reopened.open("instance").await?;
    assert_eq!(session.receipt("request").await?, Some(receipt));
    session.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_or_incompatible_storage_never_becomes_empty_state() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("configuration.yaml");
    assert!(YamlConfigurationStore::new(path.clone(), false).is_err());
    assert!(!path.exists());
    let store = YamlConfigurationStore::new(path.clone(), true)?;
    drop(store);
    for version in [0, 1, 2] {
        let old = format!("format: {version}\ninstances: {{}}\n");
        std::fs::write(&path, &old)?;
        assert!(YamlConfigurationStore::new(path.clone(), false).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, old);
    }
    Ok(())
}

#[tokio::test]
async fn an_existing_owner_does_not_evaluate_replacement_seeds() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let store = YamlConfigurationStore::new(directory.path().join("configuration.yaml"), true)?;
    assert!(
        store
            .initialize_if_absent_with("instance", "initial", || Ok(DesiredInstance::default()))
            .await?
    );
    assert!(
        !store
            .initialize_if_absent_with("instance", "different", || {
                anyhow::bail!("an obsolete startup seed was evaluated")
            })
            .await?
    );
    assert_eq!(store.configured_instance_ids().await?, ["instance"]);
    Ok(())
}

#[tokio::test]
async fn independent_process_reopens_the_same_uuid_record() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("configuration.yaml");
    let id = ComponentId::generate();
    let store = YamlConfigurationStore::new(path.clone(), true)?;
    let session = store.open("instance").await?;
    let mut desired = DesiredInstance::default();
    desired
        .ordinary
        .push(definition("orders").with_assigned_id(id.clone()));
    session.commit(0, "initial", &desired, None, &[]).await?;
    session.close().await?;
    drop(session);
    drop(store);
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "file_restart_child", "--ignored", "--nocapture"])
        .env("DRASI_UUID_TEST_STORE", &path)
        .env("DRASI_UUID_TEST_ID", id.as_str())
        .output()?;
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let store = YamlConfigurationStore::new(path, false)?;
    let session = store.open("instance").await?;
    assert_eq!(
        session.load().await?.desired.ordinary[0].name.as_str(),
        "renamed-in-child"
    );
    assert_eq!(session.load().await?.desired.ordinary[0].id, id);
    session.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "launched by independent_process_reopens_the_same_uuid_record"]
async fn file_restart_child() -> anyhow::Result<()> {
    let path = PathBuf::from(std::env::var("DRASI_UUID_TEST_STORE")?);
    let id = ComponentId::try_new(std::env::var("DRASI_UUID_TEST_ID")?)?;
    let store = YamlConfigurationStore::new(path, false)?;
    let session = store.open("instance").await?;
    let mut current = session.load().await?;
    assert_eq!(current.desired.ordinary[0].id, id);
    current.desired.ordinary[0].name = ComponentName::try_new("renamed-in-child")?;
    session
        .commit(
            current.revision,
            "child-rename",
            &current.desired,
            None,
            &[],
        )
        .await?;
    session.close().await?;
    Ok(())
}

#[cfg(unix)]
struct RunningServer {
    child: std::process::Child,
    log: PathBuf,
    base: String,
}

#[cfg(unix)]
impl RunningServer {
    async fn start(directory: &std::path::Path) -> anyhow::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let log = directory.join("server.log");
        let output = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)?;
        let child = std::process::Command::new(env!("CARGO_BIN_EXE_drasi-server"))
            .args([
                "--config",
                "server.yaml",
                "--plugins-dir",
                "plugins",
                "--port",
                &port.to_string(),
            ])
            .current_dir(directory)
            .stdout(output.try_clone()?)
            .stderr(output)
            .spawn()?;
        let mut server = Self {
            child,
            log,
            base: format!("http://127.0.0.1:{port}"),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()?;
        for _ in 0..200 {
            if let Some(status) = server.child.try_wait()? {
                anyhow::bail!(
                    "Server exited {status}: {}",
                    std::fs::read_to_string(&server.log)?
                );
            }
            if client
                .get(format!("{}/health", server.base))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                return Ok(server);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        anyhow::bail!(
            "Server readiness timed out: {}",
            std::fs::read_to_string(&server.log)?
        )
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        let signal = std::process::Command::new("/bin/kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status()?;
        anyhow::ensure!(signal.success(), "could not signal owned server");
        for _ in 0..200 {
            if let Some(status) = self.child.try_wait()? {
                anyhow::ensure!(
                    status.success(),
                    "Server shutdown failed: {}",
                    std::fs::read_to_string(&self.log)?
                );
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        anyhow::bail!("Server shutdown timed out")
    }
}

#[cfg(unix)]
impl Drop for RunningServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
async fn request(
    server: &RunningServer,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let mut request = client.request(
        method,
        format!("{}/api/v1/instances/instance{path}", server.base),
    );
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let status = response.status();
    let value: serde_json::Value = response.json().await?;
    anyhow::ensure!(
        status.is_success(),
        "Request {path} failed with {status}: {value}"
    );
    Ok(value["data"].clone())
}

#[cfg(unix)]
#[tokio::test]
async fn server_file_restart_preserves_failed_declarations_and_exposes_snapshot_restore(
) -> anyhow::Result<()> {
    use reqwest::Method;
    use serde_json::json;
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("plugins"))?;
    std::fs::write(
        directory.path().join("server.yaml"),
        r#"
apiVersion: drasi.io/v1
host: 127.0.0.1
persistConfig: true
enableUi: false
autoInstallPlugins: false
instances:
  - id: instance
    sources:
      - name: orders
        kind: unavailable-fixture
        autoStart: false
        setting: retained-before-construction
"#,
    )?;
    let mut server = RunningServer::start(directory.path()).await?;
    let current = request(&server, Method::GET, "/computation/desired", None).await?;
    assert_eq!(current["persistent"], true);
    let id = current["desired"]["ordinary"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    ComponentId::try_new(id.as_str())?;
    let snapshot = request(
        &server,
        Method::POST,
        "/computation/snapshots/original",
        None,
    )
    .await?;
    assert_eq!(snapshot["desired"]["ordinary"][0]["id"], id);
    request(
        &server,
        Method::POST,
        &format!("/components/{id}/rename"),
        Some(json!({
            "name": "renamed", "expectedRevision": current["revision"], "requestId": "rename-http",
        })),
    )
    .await?;
    server.stop().await?;

    let stored = std::fs::read_to_string(directory.path().join("server.yaml"))?;
    assert!(stored.contains("initializeIfMissing: false"));
    let mut server = RunningServer::start(directory.path()).await?;
    let current = request(&server, Method::GET, "/computation/desired", None).await?;
    assert_eq!(current["desired"]["ordinary"][0]["id"], id);
    assert_eq!(current["desired"]["ordinary"][0]["name"], "renamed");
    request(
        &server,
        Method::DELETE,
        &format!("/computation/ordinary-definitions/{id}"),
        Some(json!({
            "expectedRevision": current["revision"], "requestId": "delete-http",
        })),
    )
    .await?;
    let current = request(&server, Method::GET, "/computation/desired", None).await?;
    request(
        &server,
        Method::POST,
        "/computation/snapshots/original/restore",
        Some(json!({
            "expectedRevision": current["revision"], "requestId": "restore-http",
        })),
    )
    .await?;
    let restored = request(&server, Method::GET, "/computation/desired", None).await?;
    assert_eq!(restored["desired"]["ordinary"][0]["id"], id);
    assert_eq!(restored["desired"]["ordinary"][0]["name"], "orders");
    server.stop().await?;
    Ok(())
}
