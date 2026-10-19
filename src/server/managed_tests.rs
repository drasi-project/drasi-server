use super::*;
use crate::DrasiServerConfig;
use drasi_lib::{computation::v1::*, management::DesiredInstance};

fn settings(directory: &std::path::Path) -> anyhow::Result<DrasiServerConfig> {
    let key_file = directory.join("key");
    std::fs::write(&key_file, [73; 32])?;
    Ok(DrasiServerConfig {
        persist_config: false,

        instances: vec![crate::config::DrasiLibInstanceConfig {
            id: crate::api::models::ConfigValue::Static("managed-startup".into()).into(),
            configuration_store: Some(
                crate::managed_configuration::ConfigurationStoreConfig::Redb {
                    path: directory.join("configuration.redb"),
                    key_file,
                },
            ),
            ..Default::default()
        }],
        ..Default::default()
    })
}

fn unavailable_definition() -> anyhow::Result<crate::computation::ComputationConfig> {
    let resource = ResourceId::try_new("middleware")?;
    let mut specification = MiddlewareTransformerDefinition {
        id: ComponentId::try_new("unavailable-transformer")?,
        output_stream: StreamId::try_new("transformer/out")?,
        middleware: Vec::new(),
        pipeline: Vec::new(),
    }
    .specification(resource.clone())?;
    specification.implementation = ImplementationIdentity::try_new("unavailable/plugin", "1")?;
    Ok(crate::computation::ComputationConfig {
        definition: serde_json::from_value(serde_json::json!({
            "version":1, "revision":0,
            "components":[{
                "descriptor":specification.descriptor, "role":specification.role,
                "completion":null, "streams":{},
                "lifecycle":LifecyclePolicy { auto_start: true },
                "input_merge":InputMergePolicy::default(),
                "construction":ComponentConstruction::Factory(specification)
            }],
            "resources":[{
                "id":resource, "role":ResourceRole::Middleware,
                "ownership":ResourceOwnership::Borrowed, "binding":"instance-middleware"
            }],
            "resource_configurations":{"middleware":{"kind":"middleware"}},
            "relationships":[], "boundary_relationships":[],
            "requirements":PipeRequirements::default()
        }))?,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn file_only_instances_generate_fresh_ids_on_each_load() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    std::fs::write(
        &path,
        "persistConfig: false\nverifyPlugins: false\ninstances:\n  - {}\n",
    )?;
    let mut ids = Vec::new();
    for _ in 0..2 {
        let server = DrasiServer::new(
            path.clone(),
            0,
            directory.path().join("plugins"),
            false,
            false,
        )
        .await?;
        assert_eq!(server.instances.len(), 1);
        let id = server.instances[0].id_hint.clone().unwrap();
        uuid::Uuid::parse_str(&id)?;
        ids.push(id);
        server.instances[0].core.shutdown().await?;
    }
    assert_ne!(ids[0], ids[1]);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn generated_managed_id_and_accepted_configuration_survive_file_changes() -> anyhow::Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    let mut document = serde_json::to_value(settings(directory.path())?)?;
    document["verifyPlugins"] = false.into();
    document["instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("id");
    std::fs::write(&path, serde_yaml::to_string(&document)?)?;
    let (id, accepted) = {
        let server = DrasiServer::new(
            path.clone(),
            0,
            directory.path().join("plugins"),
            false,
            false,
        )
        .await?;
        let id = server.instances[0].id_hint.clone().unwrap();
        uuid::Uuid::parse_str(&id)?;
        let core = &server.instances[0].core;
        let accepted = core.desired_configuration()?;
        core.shutdown().await?;
        (id, accepted)
    };
    document["instances"][0]["computation"] = serde_json::json!({
        "graph": {
            "components": [{"id": "ignored-file-seed", "kind": "obsolete/missing-factory"}]
        }
    });
    std::fs::write(&path, serde_yaml::to_string(&document)?)?;
    {
        let server = DrasiServer::new(
            path.clone(),
            0,
            directory.path().join("plugins"),
            false,
            false,
        )
        .await?;
        assert_eq!(server.instances[0].id_hint.as_deref(), Some(id.as_str()));
        let core = &server.instances[0].core;
        let restored = core.desired_configuration()?;
        assert_eq!(restored.desired, accepted.desired);
        assert_eq!(restored.revision, accepted.revision);
        core.shutdown().await?;
    }
    let unchanged: serde_json::Value = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    assert!(
        unchanged["instances"][0].get("id").is_none(),
        "persistConfig is disabled"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn generated_id_never_selects_the_first_of_multiple_stored_instances() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut document = serde_json::to_value(settings(directory.path())?)?;
    document["instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("id");
    let mut config: DrasiServerConfig = serde_json::from_value(document)?;
    let stores = crate::managed_configuration::ConfigurationStores::default();
    let provider = stores
        .open(config.instances[0].configuration_store.as_ref().unwrap())
        .await?;
    for id in ["first", "second"] {
        provider
            .initialize_if_absent(id, "initial", &DesiredInstance::default())
            .await?;
    }
    let error = stores.resolve_instance_ids(&mut config).await.unwrap_err();
    assert!(
        error.to_string().contains("multiple instances"),
        "{error:#}"
    );
    assert_eq!(
        provider.configured_instance_ids().await?,
        ["first", "second"]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn stored_authored_seed_overrides_later_invalid_factory_declarations() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    let mut config = settings(directory.path())?;
    config.instances[0].computation = Some(serde_json::from_value(serde_json::json!({
        "graph": {
            "resources": {"indexes": {"kind": "memoryIndexes"}},
            "components": [{
                "id": "query", "kind": "drasi/continuous-query", "autoStart": false,
                "resources": {"indexes": "indexes"},
                "config": {"query": "MATCH (n) RETURN n.value AS value"}
            }]
        }
    }))?);
    config.save_to_file(&path)?;
    let accepted = {
        let server = DrasiServer::new(path.clone(), 0, plugins.clone(), false, false).await?;
        let core = &server.instances[0].core;
        let accepted = core.desired_configuration()?.desired;
        assert_eq!(accepted.topology.components.len(), 1);
        assert_eq!(core.desired_configuration()?.revision, 1);
        crate::managed_configuration::start_instance(core).await?;
        core.shutdown().await?;
        accepted
    };
    let crate::computation::ComputationConfigInput::Graph { graph } =
        config.instances[0].computation.as_mut().unwrap()
    else {
        panic!("readable test seed");
    };
    graph.components[0].kind = "obsolete/missing-factory".into();
    graph.components[0].config = serde_json::json!({"query": 123});
    graph.resources.insert(
        ResourceId::try_new("indexes")?,
        serde_json::json!({"kind": "obsolete"}),
    );
    graph.default_pipe_capacity = 0;
    config.save_to_file(&path)?;
    {
        let server = DrasiServer::new(path.clone(), 0, plugins.clone(), false, false).await?;
        let core = &server.instances[0].core;
        assert_eq!(core.desired_configuration()?.desired, accepted);
        assert_eq!(core.desired_configuration()?.revision, 1);
        crate::managed_configuration::start_instance(core).await?;
        core.shutdown().await?;
    }
    config.instances[0].id =
        crate::api::models::ConfigValue::Static("uninitialized-readable".into()).into();
    config.save_to_file(&path)?;
    assert!(DrasiServer::new(path, 0, plugins, false, false)
        .await
        .is_err());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn stored_definitions_override_obsolete_yaml_and_keep_failed_startup_inspectable(
) -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    let mut config = settings(directory.path())?;
    let definition = unavailable_definition()?;
    let accepted = DesiredInstance::from(definition.definition.clone()).normalized()?;
    config.instances[0].computation = Some(definition.into());
    config.save_to_file(&path)?;
    {
        let server = DrasiServer::new(path.clone(), 0, plugins.clone(), false, false).await?;
        let core = &server.instances[0].core;
        assert_eq!(core.desired_configuration()?.desired, accepted);
        assert_eq!(core.desired_configuration()?.revision, 1);
        crate::managed_configuration::start_instance(core).await?;
        assert!(core.is_running().await);
        assert!(!core.management_status().await?.converged());
        assert!(core.computation_info().await?.driver_error.is_none());
        assert!(
            core.configuration_receipt(crate::managed_configuration::INITIAL_REQUEST_ID)
                .await?
                .unwrap()
                .durable
        );
        core.shutdown().await?;
    }
    let crate::computation::ComputationConfigInput::Definition(seed) =
        config.instances[0].computation.as_mut().unwrap()
    else {
        panic!("expanded test seed");
    };
    seed.definition.version = 999;
    seed.definition.resource_configurations.insert(
        ResourceId::try_new("middleware")?,
        serde_json::json!({"kind":"obsolete"}),
    );
    config.save_to_file(&path)?;
    {
        let server = DrasiServer::new(path.clone(), 0, plugins.clone(), false, false).await?;
        let core = &server.instances[0].core;
        assert_eq!(core.desired_configuration()?.desired, accepted);
        assert_eq!(core.desired_configuration()?.revision, 1);
        crate::managed_configuration::start_instance(core).await?;
        assert!(!core.management_status().await?.converged());
        core.shutdown().await?;
    }
    // An invalid seed is ignored only for an already initialized namespace.
    config.instances[0].id = crate::api::models::ConfigValue::Static("uninitialized".into()).into();
    config.save_to_file(&path)?;
    assert!(
        DrasiServer::new(path.clone(), 0, plugins.clone(), false, false)
            .await
            .is_err()
    );
    config.instances[0].id =
        crate::api::models::ConfigValue::Static("managed-startup".into()).into();
    config.instances[0].computation = None;
    config.save_to_file(&path)?;
    for key in [Some([74; 32]), None] {
        let key_file = directory.path().join("key");
        if let Some(key) = key {
            std::fs::write(&key_file, key)?;
        } else {
            std::fs::remove_file(&key_file)?;
        }
        assert!(
            DrasiServer::new(path.clone(), 0, plugins.clone(), false, false)
                .await
                .is_err(),
            "key failures must not fall back to empty or in-memory state"
        );
    }
    Ok(())
}

#[test]
fn managed_file_configuration_accepts_missing_and_configured_ids() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    std::fs::write(
        &path,
        "instances:\n  - configurationStore: {kind: redb, path: store.redb, keyFile: key}\n",
    )?;
    let config = crate::load_config_file(&path)?;
    assert!(config.instances[0].id.is_generated());
    assert!(config.instances[0].configuration_store.is_some());
    for id in ["stable", "${DRASI_TEST_MANAGED_INSTANCE_ID:-stable}"] {
        std::fs::write(
            &path,
            format!("instances:\n  - id: '{id}'\n    configurationStore: {{kind: redb, path: store.redb, keyFile: key}}\n"),
        )?;
        let config = crate::load_config_file(&path)?;
        assert!(!config.instances[0].id.is_generated());
        assert_eq!(
            config.resolved_instances(&DtoMapper::new())?[0].id,
            "stable"
        );
    }
    Ok(())
}
