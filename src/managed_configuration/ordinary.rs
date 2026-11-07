// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    config::{ComponentId, ComponentName, ComponentReference},
    identity::IdentityProvider,
    management::{
        OrdinaryComponent, OrdinaryComponentDefinition, OrdinaryComponentRecord,
        OrdinaryComponentResolver,
    },
    Reaction, Source,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    api::models::{BootstrapProviderConfig, BootstrapProviderRef, ReactionConfig, SourceConfig},
    plugin_registry::PluginRegistry,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceSettings {
    configuration: serde_json::Value,
    identity_provider: Option<String>,
    bootstrap_provider: Option<BootstrapProviderRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReactionSettings {
    configuration: serde_json::Value,
    identity_provider: Option<String>,
    // Route keys use the original input labels; their positions bind them to
    // Core's persisted UUID inputs, even after a target is renamed.
    route_inputs: Vec<ComponentReference>,
}

pub(crate) fn source_definition(config: SourceConfig) -> Result<OrdinaryComponentDefinition> {
    Ok(OrdinaryComponentDefinition {
        id: config.id,
        name: ComponentName::try_new(config.name)?,
        auto_start: config.auto_start,
        component: OrdinaryComponent::Source {
            kind: config.kind,
            configuration: serde_json::to_value(SourceSettings {
                configuration: config.config,
                identity_provider: config.identity_provider,
                bootstrap_provider: config.bootstrap_provider,
            })?,
        },
    })
}

pub(crate) fn reaction_definition(config: ReactionConfig) -> Result<OrdinaryComponentDefinition> {
    Ok(OrdinaryComponentDefinition {
        id: config.id,
        name: ComponentName::try_new(config.name)?,
        auto_start: config.auto_start,
        component: OrdinaryComponent::Reaction {
            kind: config.kind,
            inputs: config.queries.clone(),
            configuration: serde_json::to_value(ReactionSettings {
                configuration: config.config,
                identity_provider: config.identity_provider,
                route_inputs: config.queries,
            })?,
        },
    })
}

pub(crate) fn source_configuration(record: &OrdinaryComponentRecord) -> Result<SourceConfig> {
    let OrdinaryComponent::Source {
        kind,
        configuration,
    } = &record.component
    else {
        anyhow::bail!("component {} is not a source", record.id);
    };
    let settings: SourceSettings =
        serde_json::from_value(configuration.clone()).context("invalid stored source settings")?;
    Ok(SourceConfig {
        id: Some(record.id.clone()),
        name: record.name.to_string(),
        kind: kind.clone(),
        auto_start: record.auto_start,
        config: settings.configuration,
        identity_provider: settings.identity_provider,
        bootstrap_provider: settings.bootstrap_provider,
    })
}

pub(crate) fn reaction_configuration(record: &OrdinaryComponentRecord) -> Result<ReactionConfig> {
    let OrdinaryComponent::Reaction {
        kind,
        configuration,
        inputs,
    } = &record.component
    else {
        anyhow::bail!("component {} is not a reaction", record.id);
    };
    let settings: ReactionSettings = serde_json::from_value(configuration.clone())
        .context("invalid stored reaction settings")?;
    let mut config = ReactionConfig {
        id: Some(record.id.clone()),
        name: record.name.to_string(),
        kind: kind.clone(),
        auto_start: record.auto_start,
        config: settings.configuration,
        identity_provider: settings.identity_provider,
        queries: inputs.clone(),
    };
    anyhow::ensure!(
        settings.route_inputs.len() == inputs.len(),
        "stored reaction route bindings disagree"
    );
    let mut bindings = HashMap::new();
    for (original, current) in settings.route_inputs.iter().zip(inputs) {
        let Some(id) = current.as_id() else {
            continue;
        };
        let label = match original {
            ComponentReference::Name(name) => name.as_str(),
            ComponentReference::Id { id } => id.as_str(),
        };
        if crate::factories::reaction_uses_query_reference(kind, &config.config, label) {
            if let Some(previous) = bindings.insert(label.to_owned(), id.to_string()) {
                anyhow::ensure!(
                    previous == id.as_str(),
                    "ambiguous reaction route '{label}'"
                );
            }
        }
    }
    crate::factories::remap_reaction_references(kind, &mut config.config, &bindings)?;
    Ok(config)
}

pub(crate) struct ServerOrdinaryComponents {
    pub plugins: Arc<RwLock<PluginRegistry>>,
    pub identity_providers: HashMap<String, Arc<dyn IdentityProvider>>,
    pub bootstrap_providers: HashMap<String, BootstrapProviderConfig>,
}

impl ServerOrdinaryComponents {
    fn identity(&self, id: Option<&str>) -> Result<Option<Arc<dyn IdentityProvider>>> {
        id.map(|id| {
            self.identity_providers
                .get(id)
                .cloned()
                .with_context(|| format!("unknown identityProvider '{id}'"))
        })
        .transpose()
    }
}

#[async_trait]
impl OrdinaryComponentResolver for ServerOrdinaryComponents {
    async fn create_source(
        &self,
        _: &str,
        record: &OrdinaryComponentRecord,
        update: Option<&drasi_lib::computation::v1::ComponentUpdateDirective>,
    ) -> Result<(Box<dyn Source>, HashMap<String, String>)> {
        anyhow::ensure!(
            update.is_none(),
            "the ordinary Source plugin ABI does not support factory-managed reconstruction updates"
        );
        let config = crate::factories::resolve_source_bootstrap_provider(
            source_configuration(record)?,
            &self.bootstrap_providers,
        )?;
        let identity = self.identity(config.identity_provider())?;
        let (source, metadata) =
            crate::factories::create_source_locked(&self.plugins, config).await?;
        if let Some(identity) = identity {
            source.set_identity_provider(identity).await;
        }
        Ok((source, metadata))
    }

    async fn create_reaction(
        &self,
        _: &str,
        record: &OrdinaryComponentRecord,
        inputs: &[ComponentId],
        update: Option<&drasi_lib::computation::v1::ComponentUpdateDirective>,
    ) -> Result<(Box<dyn Reaction>, HashMap<String, String>)> {
        anyhow::ensure!(
            update.is_none(),
            "the ordinary Reaction plugin ABI does not support factory-managed reconstruction updates"
        );
        let config = reaction_configuration(record)?;
        anyhow::ensure!(
            config.resolved_query_ids()?
                == inputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "stored reaction UUID inputs disagree with the configured owner"
        );
        let identity = self.identity(config.identity_provider())?;
        let (reaction, metadata) =
            crate::factories::create_reaction_locked(&self.plugins, config).await?;
        if let Some(identity) = identity {
            reaction.set_identity_provider(identity).await;
        }
        Ok((reaction, metadata))
    }
}
