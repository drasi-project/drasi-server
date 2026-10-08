use super::*;
use crate::DrasiServerConfig;
use drasi_lib::{computation::v1::*, management::DesiredInstance};

fn settings(directory: &std::path::Path) -> anyhow::Result<DrasiServerConfig> {
    let key_file = directory.join("key");
    std::fs::write(&key_file, [73; 32])?;
    Ok(DrasiServerConfig {
        id: crate::api::models::ConfigValue::Static("managed-startup".into()),
        configuration_store: Some(
            crate::managed_configuration::ConfigurationStoreConfig::Redb {
                path: directory.join("configuration.redb"),
                key_file,
            },
        ),
        persist_config: false,
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
            "version":1, "revision":0, "allow_incomplete":true,
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
async fn stored_definitions_override_obsolete_yaml_and_keep_failed_startup_inspectable(
) -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    let plugins = directory.path().join("plugins");
    let mut config = settings(directory.path())?;
    let definition = unavailable_definition()?;
    let accepted = DesiredInstance::from(definition.definition.clone()).normalized()?;
    config.computation = Some(definition);
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
    config.computation.as_mut().unwrap().definition.version = 999;
    config
        .computation
        .as_mut()
        .unwrap()
        .definition
        .resource_configurations
        .insert(
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
    config.id = crate::api::models::ConfigValue::Static("uninitialized".into());
    config.save_to_file(&path)?;
    assert!(
        DrasiServer::new(path.clone(), 0, plugins.clone(), false, false)
            .await
            .is_err()
    );
    config.id = crate::api::models::ConfigValue::Static("managed-startup".into());
    config.computation = None;
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
fn durable_file_configuration_requires_explicit_stable_ids() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("server.yaml");
    for document in [
        "configurationStore: {kind: redb, path: store.redb, keyFile: key}\n",
        "instances:\n  - configurationStore: {kind: redb, path: store.redb, keyFile: key}\n",
        "id: null\nconfigurationStore: {kind: redb, path: store.redb, keyFile: key}\n",
    ] {
        std::fs::write(&path, document)?;
        let error = crate::load_config_file(&path).unwrap_err();
        assert!(error.to_string().contains("explicit"), "{error}");
    }
    std::fs::write(
        &path,
        "id: stable\nconfigurationStore: {kind: redb, path: store.redb, keyFile: key}\n",
    )?;
    assert!(crate::load_config_file(&path)?
        .configuration_store
        .is_some());
    Ok(())
}
