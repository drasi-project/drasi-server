use super::*;
use drasi_lib::management::DesiredInstance;
use drasi_server::managed_configuration::{ConfigurationStoreConfig, ConfigurationStores};

fn settings(directory: &Path) -> Result<ConfigurationStoreConfig> {
    let key = directory.join("configuration.key");
    std::fs::write(&key, [31; 32])?;
    Ok(ConfigurationStoreConfig::Redb {
        path: directory.join("configuration.redb"),
        key_file: key,
    })
}

async fn managed_instance(
    stores: &ConfigurationStores,
    settings: &ConfigurationStoreConfig,
    plugins: Arc<RwLock<PluginRegistry>>,
    seed: Option<&ComputationConfig>,
) -> Result<Arc<DrasiLib>> {
    let secrets = Arc::new(
        drasi_lib::secret_store::MemorySecretStoreProvider::new().with_secret("COUNT", "4"),
    );
    let builder = stores
        .configure(
            DrasiLib::builder()
                .with_id("managed-native")
                .with_secret_store_provider(secrets.clone()),
            "managed-native",
            settings,
            seed,
            plugins,
            Some(secrets),
        )
        .await?;
    Ok(Arc::new(builder.build().await?))
}

#[tokio::test(flavor = "current_thread")]
async fn durable_router_receipts_native_execution_and_authoritative_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let settings = settings(directory.path())?;
    let output = directory.path().join("capture.jsonl");
    let mut config = definition(&output)?;
    for component in &mut config.definition.components {
        component.lifecycle.auto_start = true;
    }
    let desired = DesiredInstance::from(config.definition.clone()).normalized()?;
    let payload = json!({"expectedRevision":0,"requestId":"lost-response","desired":desired});
    {
        let stores = ConfigurationStores::default();
        let plugins = Arc::new(RwLock::new(factories()));
        let core = managed_instance(&stores, &settings, plugins.clone(), None).await?;
        assert!(core.configuration_is_persistent());
        core.start().await?;
        let registry = InstanceRegistry::new();
        registry
            .add("managed-native".into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let app = build_v1_router(
            registry.clone(),
            Arc::new(false),
            None,
            plugins.clone(),
            None,
        );
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/instances/managed-native/computation/desired")
                    .header("content-type", "application/yaml")
                    .body(Body::from(serde_yaml::to_string(
                        &drasi_server::api::v1::handlers::DesiredConfigurationRequest {
                            expected_revision: 0,
                            request_id: "lost-response".into(),
                            desired: desired.clone(),
                        },
                    )?))?,
            )
            .await?;
        if response.status() != StatusCode::ACCEPTED {
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
            anyhow::bail!(
                "acceptance failed: {status}: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        assert_eq!(response.headers()["cache-control"], "no-store");
        drop(response);
        let (status, receipt) = request(
            &app,
            "GET",
            "/instances/managed-native/computation/receipts/lost-response",
            "",
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert_eq!(receipt["data"]["revision"], 1);
        assert_eq!(receipt["data"]["durable"], true);
        assert!(core.reconcile_desired_state().await?.converged());
        exact_output(&output).await?;
        let before = core
            .inspect_computation_graph()?
            .snapshot()
            .observed
            .clone();
        let (status, repeated) = request(
            &app,
            "PUT",
            "/instances/managed-native/computation/desired",
            &payload.to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::ACCEPTED, "{repeated}");
        assert_eq!(repeated["data"], receipt["data"]);
        core.reconcile_desired_state().await?;
        for (id, observed) in &before.components {
            assert_eq!(
                core.inspect_computation_graph()?
                    .snapshot()
                    .observed
                    .components[id]
                    .generation,
                observed.generation
            );
        }
        for changed in [
            json!({"expectedRevision":0,"requestId":"new-request","desired":desired}),
            json!({"expectedRevision":1,"requestId":"lost-response","desired":DesiredInstance::default()}),
        ] {
            assert_eq!(
                request(
                    &app,
                    "PUT",
                    "/instances/managed-native/computation/desired",
                    &changed.to_string(),
                    "application/json"
                )
                .await?
                .0,
                StatusCode::CONFLICT
            );
            for path in ["sources/counter", "queries/missing", "reactions/capture"] {
                let (status, body) = request(
                    &app,
                    "DELETE",
                    &format!("/instances/managed-native/{path}"),
                    "",
                    "application/json",
                )
                .await?;
                assert_eq!(status, StatusCode::CONFLICT, "{body}");
                assert_eq!(body["code"], "MANAGED_CONFIGURATION_REQUIRED");
            }
            let mut foreign = desired.clone();
            foreign.topology.graph_id = "foreign-graph".into();
            let (status, body) = request(
                &app,
                "PUT",
                "/instances/managed-native/computation/desired",
                &json!({"expectedRevision":1, "requestId":"foreign", "desired":foreign})
                    .to_string(),
                "application/json",
            )
            .await?;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert!(core.configuration_receipt("foreign").await?.is_none());
        }
        assert_eq!(
            request(
                &app,
                "POST",
                "/instances/managed-native/computation/components",
                &serde_json::to_string(&config)?,
                "application/json"
            )
            .await?
            .0,
            StatusCode::CONFLICT
        );
        let read_only = build_v1_router(registry, Arc::new(true), None, plugins, None);
        assert_eq!(
            request(
                &read_only,
                "PUT",
                "/instances/managed-native/computation/desired",
                &payload.to_string(),
                "application/json"
            )
            .await?
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            request(
                &read_only,
                "GET",
                "/instances/managed-native/computation/receipts/lost-response",
                "",
                "application/json"
            )
            .await?
            .0,
            StatusCode::OK
        );
        core.shutdown().await?;
    }
    std::fs::remove_file(&output)?;
    {
        let stores = ConfigurationStores::default();
        let plugins = Arc::new(RwLock::new(factories()));
        let stale = ComputationConfig {
            definition: DesiredInstance::default().topology,
        };
        let core = managed_instance(&stores, &settings, plugins, Some(&stale)).await?;
        assert_eq!(core.desired_configuration()?.desired, desired);
        assert_eq!(
            core.configuration_receipt("lost-response")
                .await?
                .unwrap()
                .revision,
            1
        );
        core.start().await?;
        exact_output(&output).await?;
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn dynamically_created_managed_instances_share_storage_not_acceptance_namespaces(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let settings = settings(directory.path())?;
    let registry = InstanceRegistry::new();
    let app = build_v1_router(
        registry.clone(),
        Arc::new(false),
        None,
        Arc::new(RwLock::new(factories())),
        None,
    );
    for id in ["managed-first", "managed-second"] {
        let (status, body) = request(
            &app,
            "POST",
            "/instances",
            &json!({"id":id, "configurationStore":settings}).to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        let core = registry.get(id).await.unwrap();
        assert!(core.configuration_is_persistent());
        assert_eq!(core.get_current_config().await?.id, id);
        let mut desired = DesiredInstance::default();
        if id == "managed-first" {
            desired.topology.allow_incomplete = !desired.topology.allow_incomplete;
        }
        let (status, body) = request(
            &app,
            "PUT",
            &format!("/instances/{id}/computation/desired"),
            &json!({"expectedRevision":0,"requestId":"shared-request-name","desired":desired})
                .to_string(),
            "application/json",
        )
        .await?;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
        assert_eq!(
            body["data"]["revision"],
            if id == "managed-first" { 1 } else { 0 }
        );
        assert!(core.reconcile_desired_state().await?.converged());
    }
    for id in ["managed-first", "managed-second"] {
        let core = registry.get(id).await.unwrap();
        assert_eq!(
            core.configuration_receipt("shared-request-name")
                .await?
                .unwrap()
                .revision,
            if id == "managed-first" { 1 } else { 0 },
        );
        core.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn failed_managed_definitions_remain_visible_without_plaintext_yaml_or_status_secrets(
) -> Result<()> {
    const SECRET: &str = "secret-that-must-not-appear-in-public-status-or-yaml";
    let directory = tempfile::tempdir()?;
    let settings = settings(directory.path())?;
    let stores = ConfigurationStores::default();
    let plugins = Arc::new(RwLock::new(factories()));
    let core = managed_instance(&stores, &settings, plugins.clone(), None).await?;
    core.start().await?;
    let mut config = definition(&directory.path().join("capture.jsonl"))?;
    let ComponentConstruction::Factory(spec) = &mut config.definition.components[0].construction
    else {
        anyhow::bail!("factory fixture");
    };
    spec.implementation = ImplementationIdentity::try_new("unavailable/native", "1")?;
    spec.configuration.insert(
        "password".into(),
        ConfigurationValue::Literal(json!(SECRET)),
    );
    let desired = DesiredInstance::from(config.definition).normalized()?;
    let registry = InstanceRegistry::new();
    registry
        .add("managed-native".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let app = build_v1_router(registry.clone(), Arc::new(false), None, plugins, None);
    let (status, body) = request(
        &app,
        "PUT",
        "/instances/managed-native/computation/desired",
        &json!({"expectedRevision":0,"requestId":"unavailable","desired":desired}).to_string(),
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert!(!core.reconcile_desired_state().await?.converged());
    let (status, body) = request(
        &app,
        "GET",
        "/instances/managed-native/computation/management",
        "",
        "application/json",
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "failed");
    assert!(!body.to_string().contains(SECRET));
    assert_eq!(core.desired_configuration()?.desired, desired);
    let original: DrasiServerConfig = serde_json::from_value(json!({
        "id":"managed-native", "configurationStore":settings
    }))?;
    let path = directory.path().join("server.yaml");
    let persistence = ConfigPersistence::new(
        path.clone(),
        registry,
        "127.0.0.1".into(),
        8080,
        "info".into(),
        true,
        IndexMap::new(),
        IndexMap::new(),
        None,
        &original,
    );
    persistence.save().await?;
    let yaml = std::fs::read_to_string(&path)?;
    assert!(!yaml.contains(SECRET));
    assert!(!yaml.contains("computation:"));
    let saved = drasi_server::load_config_file(path)?;
    assert_eq!(
        serde_json::to_value(saved.configuration_store)?,
        serde_json::to_value(Some(settings))?
    );
    core.shutdown().await?;
    let encrypted = std::fs::read(directory.path().join("configuration.redb"))?;
    assert!(!encrypted
        .windows(SECRET.len())
        .any(|bytes| bytes == SECRET.as_bytes()));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn store_cache_shares_only_matching_keys_and_missing_keys_never_fall_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let settings = settings(directory.path())?;
    let stores = ConfigurationStores::default();
    let first = stores.open(&settings).await?;
    let second = stores.open(&settings).await?;
    assert!(Arc::ptr_eq(&first, &second));
    let ConfigurationStoreConfig::Redb { key_file, .. } = &settings;
    std::fs::write(key_file, [32; 32])?;
    assert!(stores.open(&settings).await.is_err());
    std::fs::write(key_file, [31; 31])?;
    assert!(stores.open(&settings).await.is_err());
    std::fs::remove_file(key_file)?;
    assert!(stores.open(&settings).await.is_err());
    Ok(())
}
