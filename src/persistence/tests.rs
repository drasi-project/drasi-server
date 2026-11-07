// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;
use crate::{
    api::{
        models::{ReactionConfig, SourceConfig},
        shared::{error::error_codes, handlers::persist_after_operation},
    },
    config::{PluginDependency, TrustedIdentity},
    managed_configuration::{
        ordinary::{reaction_definition, source_configuration, source_definition},
        ConfigurationStoreConfig,
    },
};
use drasi_lib::{
    management::{ConfigurationRequest, NewComponentDefinition, OrdinaryComponentDefinition},
    DrasiLib, Query,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn managed(
    directory: &Path,
    id: &str,
    allow_external_ids: bool,
) -> Result<(Arc<DrasiLib>, DrasiLibInstanceConfig)> {
    let path = directory.join(format!("{id}.components.yaml"));
    let core = Arc::new(
        crate::query_test_support::builder()
            .with_id(id)
            .with_allow_external_component_ids(allow_external_ids)
            .with_configuration_store(Arc::new(configuration_store::YamlConfigurationStore::new(
                path.clone(),
                true,
            )?))
            .build()
            .await?,
    );
    let config = DrasiLibInstanceConfig {
        id: ConfigValue::Static(id.to_owned()).into(),
        configuration_store: Some(ConfigurationStoreConfig::Yaml {
            path,
            initialize_if_missing: true,
        }),
        ..Default::default()
    };
    Ok((core, config))
}

fn write_initial(directory: &Path, config: &DrasiServerConfig) -> Result<PathBuf> {
    let path = directory.join("server.yaml");
    std::fs::write(&path, serde_yaml::to_string(config)?)?;
    Ok(path)
}

async fn persistence(
    path: &Path,
    registry: InstanceRegistry,
    config: &DrasiServerConfig,
    enabled: bool,
) -> Result<ConfigPersistence> {
    ConfigPersistence::new(
        path.to_owned(),
        registry,
        "127.0.0.1".into(),
        8080,
        "info".into(),
        enabled,
        config,
    )
    .await
}

fn read(path: &Path) -> Result<DrasiServerConfig> {
    Ok(serde_yaml::from_slice(&std::fs::read(path)?)?)
}

#[tokio::test]
async fn definitions_and_provider_references_survive_without_runtime_snapshots() -> Result<()> {
    let directory = TempDir::new()?;
    let (state_path, expected) = {
        let (core, instance) = managed(directory.path(), "stored", false).await?;
        let inline: SourceConfig = serde_json::from_value(json!({
            "kind": "unavailable-fixture", "name": "inline", "autoStart": false,
            "identityProvider": "auth",
            "bootstrapProvider": {"kind": "scriptfile", "filePaths": ["seed.jsonl"]},
            "endpoint": "fixture-endpoint"
        }))?;
        let referenced: SourceConfig = serde_json::from_value(json!({
            "kind": "unavailable-fixture", "name": "referenced", "autoStart": true,
            "bootstrapProvider": "shared-bootstrap"
        }))?;
        let reaction: ReactionConfig = serde_json::from_value(json!({
            "kind": "unavailable-fixture", "name": "logs", "autoStart": false,
            "identityProvider": "auth", "queries": ["view"], "setting": "retained"
        }))?;
        let additions: Vec<_> = vec![
            source_definition(inline)?,
            source_definition(referenced)?,
            OrdinaryComponentDefinition::query(
                Query::cypher("view")
                    .query("MATCH (n) RETURN n")
                    .from_input("inline")
                    .enable_bootstrap(false)
                    .auto_start(false)
                    .build()?,
            )?,
            reaction_definition(reaction)?,
        ]
        .into_iter()
        .map(NewComponentDefinition::Ordinary)
        .collect();
        let receipt = core
            .apply_configuration(0, "definitions", ConfigurationRequest::adding(additions))
            .await?;
        assert_eq!(receipt.creation.as_ref().unwrap().components.len(), 4);
        let expected = core.desired_configuration()?.desired;
        let source = source_configuration(
            expected
                .ordinary
                .iter()
                .find(|record| record.name.as_str() == "inline")
                .unwrap(),
        )?;
        assert!(!source.auto_start);
        assert_eq!(source.identity_provider.as_deref(), Some("auth"));
        assert_eq!(
            source
                .bootstrap_provider
                .as_ref()
                .unwrap()
                .as_inline()
                .unwrap()
                .config["filePaths"],
            json!(["seed.jsonl"])
        );
        let reference = source_configuration(
            expected
                .ordinary
                .iter()
                .find(|record| record.name.as_str() == "referenced")
                .unwrap(),
        )?;
        assert!(reference.auto_start);
        assert_eq!(
            serde_json::to_value(reference.bootstrap_provider)?,
            "shared-bootstrap"
        );

        let registry =
            InstanceRegistry::from_map(IndexMap::from([("stored".into(), core.clone())]));
        let config = DrasiServerConfig {
            instances: vec![instance],
            ..Default::default()
        };
        let path = write_initial(directory.path(), &config)?;
        let settings = persistence(&path, registry, &config, true).await?;
        settings.save().await?;
        let saved = read(&path)?;
        let instance = &saved.instances[0];
        assert!(instance.sources.is_empty());
        assert!(instance.queries.is_empty());
        assert!(instance.reactions.is_empty());
        assert!(instance.computation.is_none());
        let Some(ConfigurationStoreConfig::Yaml {
            path: state_path,
            initialize_if_missing,
        }) = &instance.configuration_store
        else {
            anyhow::bail!("missing YAML store location")
        };
        assert!(!initialize_if_missing);
        let state_path = state_path.clone();
        core.shutdown().await?;
        (state_path, expected)
    };

    let restored = crate::query_test_support::builder()
        .with_id("stored")
        .with_configuration_store(Arc::new(configuration_store::YamlConfigurationStore::new(
            state_path, false,
        )?))
        .build()
        .await?;
    assert!(!restored.allow_external_component_ids());
    assert_eq!(restored.desired_configuration()?.desired, expected);
    restored.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn host_settings_and_live_identity_policy_are_preserved() -> Result<()> {
    let directory = TempDir::new()?;
    let (core, instance) = managed(directory.path(), "settings", true).await?;
    let config = DrasiServerConfig {
        enable_ui: false,
        plugin_registry: Some("registry.example/plugins".into()),
        auto_install_plugins: true,
        plugins: vec![PluginDependency {
            reference: "source/fixture:1.0.0".into(),
        }],
        verify_plugins: true,
        trusted_identities: vec![TrustedIdentity {
            issuer: "https://issuer.example".into(),
            subject_pattern: "fixture-publisher".into(),
        }],
        hot_reload_plugins: true,
        hot_reload_debounce_ms: 500,
        cors_allowed_origins: vec!["http://localhost:3000".into()],
        instances: vec![instance],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let registry = InstanceRegistry::from_map(IndexMap::from([("settings".into(), core.clone())]));
    persistence(&path, registry, &config, true)
        .await?
        .save()
        .await?;
    let saved = read(&path)?;
    assert!(!saved.enable_ui);
    assert_eq!(saved.plugin_registry, config.plugin_registry);
    assert!(saved.auto_install_plugins && saved.verify_plugins && saved.hot_reload_plugins);
    assert_eq!(saved.plugins[0].reference, config.plugins[0].reference);
    assert_eq!(
        saved.trusted_identities[0].issuer,
        config.trusted_identities[0].issuer
    );
    assert_eq!(
        saved.trusted_identities[0].subject_pattern,
        config.trusted_identities[0].subject_pattern
    );
    assert_eq!(saved.hot_reload_debounce_ms, 500);
    assert_eq!(saved.cors_allowed_origins, config.cors_allowed_origins);
    assert!(saved.instances[0].allow_external_component_ids);
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn budgets_stay_with_their_instances_and_registration_replaces_settings() -> Result<()> {
    let directory = TempDir::new()?;
    let (first, mut first_config) = managed(directory.path(), "first", false).await?;
    let (second, mut second_config) = managed(directory.path(), "second", false).await?;
    first_config.persist_index = true;
    first_config.memory_budget_mib = Some(ConfigValue::Static(128));
    second_config.persist_index = true;
    second_config.memory_budget_mib = Some(ConfigValue::Static(256));
    let registry = InstanceRegistry::from_map(IndexMap::from([
        ("first".into(), first.clone()),
        ("second".into(), second.clone()),
    ]));
    let config = DrasiServerConfig {
        instances: vec![first_config.clone(), second_config],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let settings = persistence(&path, registry, &config, true).await?;
    settings.save().await?;
    let saved = read(&path)?;
    assert_eq!(
        saved.instances[0].memory_budget_mib,
        Some(ConfigValue::Static(128))
    );
    assert_eq!(
        saved.instances[1].memory_budget_mib,
        Some(ConfigValue::Static(256))
    );
    first_config.persist_index = false;
    first_config.memory_budget_mib = None;
    settings.register_instance(first_config).await?;
    settings.save().await?;
    let saved = read(&path)?;
    assert!(!saved.instances[0].persist_index);
    assert_eq!(saved.instances[0].memory_budget_mib, None);
    assert_eq!(
        saved.instances[1].memory_budget_mib,
        Some(ConfigValue::Static(256))
    );
    first.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn missing_instance_settings_are_not_inherited_and_empty_registry_is_saved() -> Result<()> {
    let directory = TempDir::new()?;
    let (first, mut first_config) = managed(directory.path(), "first", false).await?;
    first_config.memory_budget_mib = Some(ConfigValue::Static(512));
    first_config.persist_index = true;
    let (second, second_config) = managed(directory.path(), "second", false).await?;
    let registry = InstanceRegistry::from_map(IndexMap::from([("first".into(), first.clone())]));
    let config = DrasiServerConfig {
        instances: vec![first_config],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let settings = persistence(&path, registry.clone(), &config, true).await?;
    settings.save().await?;
    let unchanged = std::fs::read(&path)?;
    registry
        .add("second".into(), second.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(settings
        .save()
        .await
        .unwrap_err()
        .to_string()
        .contains("second"));
    assert_eq!(std::fs::read(&path)?, unchanged);
    settings.register_instance(second_config).await?;
    settings.save().await?;
    let saved = read(&path)?;
    assert_eq!(saved.instances[1].memory_budget_mib, None);
    registry
        .remove("first")
        .await
        .context("registered first instance")?;
    registry
        .remove("second")
        .await
        .context("registered second instance")?;
    settings.save().await?;
    assert!(read(&path)?.instances.is_empty());
    first.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn memory_only_instances_are_never_saved_as_restoration_records() -> Result<()> {
    let directory = TempDir::new()?;
    let core = Arc::new(
        crate::query_test_support::builder()
            .with_id("memory")
            .build()
            .await?,
    );
    let config = DrasiServerConfig {
        instances: vec![DrasiLibInstanceConfig {
            id: ConfigValue::Static("memory".into()).into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let unchanged = std::fs::read(&path)?;
    let registry = InstanceRegistry::from_map(IndexMap::from([("memory".into(), core.clone())]));
    let error = persistence(&path, registry.clone(), &config, true)
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("durable configurationStore"));
    persistence(&path, registry, &config, false)
        .await?
        .save()
        .await?;
    assert_eq!(std::fs::read(&path)?, unchanged);
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn late_memory_only_registration_cannot_bypass_the_durable_store_requirement() -> Result<()> {
    let directory = TempDir::new()?;
    let config = DrasiServerConfig::default();
    let path = write_initial(directory.path(), &config)?;
    let registry = InstanceRegistry::new();
    let settings = persistence(&path, registry.clone(), &config, true).await?;
    let core = Arc::new(
        crate::query_test_support::builder()
            .with_id("late")
            .build()
            .await?,
    );
    registry
        .add("late".into(), core.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    settings
        .register_instance(DrasiLibInstanceConfig {
            id: ConfigValue::Static("late".into()).into(),
            ..Default::default()
        })
        .await?;
    assert!(settings
        .save()
        .await
        .unwrap_err()
        .to_string()
        .contains("durable configurationStore"));
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_saves_use_complete_documents_and_leave_no_staging_files() -> Result<()> {
    let directory = TempDir::new()?;
    let (core, instance) = managed(directory.path(), "concurrent", false).await?;
    let config = DrasiServerConfig {
        instances: vec![instance],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let registry =
        InstanceRegistry::from_map(IndexMap::from([("concurrent".into(), core.clone())]));
    let settings = persistence(&path, registry, &config, true).await?;
    for saved in futures_util::future::join_all((0..8).map(|_| settings.save())).await {
        saved?;
    }
    assert_eq!(read(&path)?.instances.len(), 1);
    for entry in std::fs::read_dir(directory.path())? {
        assert!(!entry?.file_name().to_string_lossy().ends_with(".tmp"));
    }
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn persistence_errors_retain_diagnostics_without_exposing_them_in_the_summary() -> Result<()>
{
    let directory = TempDir::new()?;
    let (core, instance) = managed(directory.path(), "failure", false).await?;
    let config = DrasiServerConfig {
        instances: vec![instance],
        ..Default::default()
    };
    let path = write_initial(directory.path(), &config)?;
    let registry = InstanceRegistry::from_map(IndexMap::from([("failure".into(), core.clone())]));
    let settings = Arc::new(persistence(&path, registry, &config, true).await?);
    persist_after_operation(&Some(settings.clone()), "creating instance")
        .await
        .expect("baseline settings save");
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    let error = persist_after_operation(&Some(settings), "creating instance")
        .await
        .unwrap_err();
    assert_eq!(error.code, error_codes::PERSISTENCE_FAILED);
    assert!(error.message.contains("creating instance"));
    let technical = error
        .details
        .as_ref()
        .unwrap()
        .technical_details
        .as_ref()
        .unwrap();
    assert!(!technical.is_empty());
    assert!(!error.message.contains(technical));
    persist_after_operation(&None, "disabled")
        .await
        .expect("disabled persistence");
    core.shutdown().await?;
    Ok(())
}
