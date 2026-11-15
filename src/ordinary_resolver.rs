// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Reconstruction of persisted ordinary sources and reactions.
//!
//! A persisted instance stores each source or reaction as an
//! [`OrdinaryComponentRecord`]. The record's configuration is the Server DTO
//! map without `kind`, `id`, `autoStart` and (for reactions) `queries`; those
//! live on the record itself. Secret references stay unresolved in the record
//! and are resolved by the plugin descriptor at construction time.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::{
    computation::v1::{
        ComponentId, ComponentName, ComponentUpdateData, ComponentUpdateDirective,
        ComponentUpdateMode, ComponentUpdatePlan,
    },
    config::{ComponentReference, QueryConfig},
    identity::IdentityProvider,
    management::{
        DesiredInstance, OrdinaryComponent, OrdinaryComponentDefinition, OrdinaryComponentRecord,
        OrdinaryComponentResolver,
    },
    Reaction, Source,
};
use tokio::sync::RwLock;

use crate::api::models::{BootstrapProviderConfig, ReactionConfig, SourceConfig};
use crate::plugin_registry::PluginRegistry;

const SOURCE_FIELDS: [&str; 3] = ["kind", "id", "autoStart"];
const REACTION_FIELDS: [&str; 4] = ["kind", "id", "autoStart", "queries"];

/// A requested change that a persisted component cannot apply in place.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OrdinaryTransitionRequired(pub String);

fn strip(value: serde_json::Value, fields: &[&str]) -> Result<serde_json::Value> {
    let serde_json::Value::Object(mut map) = value else {
        anyhow::bail!("component configuration must serialize to an object");
    };
    for field in fields {
        map.remove(*field);
    }
    Ok(serde_json::Value::Object(map))
}

fn restore(
    configuration: &serde_json::Value,
    record: &OrdinaryComponentRecord,
    kind: &str,
    fields: &[&str],
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut map = configuration
        .as_object()
        .cloned()
        .context("stored component configuration must be an object")?;
    for field in fields {
        map.remove(*field);
    }
    map.insert("kind".into(), kind.into());
    map.insert("id".into(), record.name.as_str().into());
    map.insert("autoStart".into(), record.auto_start.into());
    Ok(map)
}

/// Encode a source DTO as an ordinary definition named by its DTO `id`.
pub fn source_definition(config: &SourceConfig) -> Result<OrdinaryComponentDefinition> {
    Ok(OrdinaryComponentDefinition {
        id: None,
        name: ComponentName::try_new(config.id()).context("invalid source name")?,
        auto_start: config.auto_start(),
        component: OrdinaryComponent::Source {
            kind: config.kind().to_string(),
            configuration: strip(serde_json::to_value(config)?, &SOURCE_FIELDS)?,
        },
    })
}

/// Encode a reaction DTO as an ordinary definition named by its DTO `id`.
pub fn reaction_definition(config: &ReactionConfig) -> Result<OrdinaryComponentDefinition> {
    Ok(OrdinaryComponentDefinition {
        id: None,
        name: ComponentName::try_new(config.id()).context("invalid reaction name")?,
        auto_start: config.auto_start,
        component: OrdinaryComponent::Reaction {
            kind: config.kind.clone(),
            configuration: strip(serde_json::to_value(config)?, &REACTION_FIELDS)?,
            inputs: config.queries.clone(),
        },
    })
}

/// Rebuild the source DTO stored in a record. `id` is the record name.
pub fn source_config(record: &OrdinaryComponentRecord) -> Result<SourceConfig> {
    let OrdinaryComponent::Source {
        kind,
        configuration,
    } = &record.component
    else {
        anyhow::bail!("component {} is not a source", record.name);
    };
    let map = restore(configuration, record, kind, &SOURCE_FIELDS)?;
    serde_json::from_value(serde_json::Value::Object(map))
        .with_context(|| format!("decode stored source '{}'", record.name))
}

/// Rebuild the reaction DTO stored in a record with the given query references.
pub fn reaction_config(
    record: &OrdinaryComponentRecord,
    queries: Vec<ComponentReference>,
) -> Result<ReactionConfig> {
    let OrdinaryComponent::Reaction {
        kind,
        configuration,
        ..
    } = &record.component
    else {
        anyhow::bail!("component {} is not a reaction", record.name);
    };
    let mut map = restore(configuration, record, kind, &REACTION_FIELDS)?;
    map.insert("queries".into(), serde_json::to_value(queries)?);
    serde_json::from_value(serde_json::Value::Object(map))
        .with_context(|| format!("decode stored reaction '{}'", record.name))
}

/// Role and implementation kind; a change of either requires removal and addition.
pub fn record_kind(component: &OrdinaryComponent) -> (&'static str, Option<&str>) {
    match component {
        OrdinaryComponent::Source { kind, .. } => ("source", Some(kind)),
        OrdinaryComponent::Query { .. } => ("query", None),
        OrdinaryComponent::Reaction { kind, .. } => ("reaction", Some(kind)),
        OrdinaryComponent::ProcessLocal { .. } => ("process-local", None),
    }
}

/// Refuse a change that cannot be applied to the same persisted component.
pub fn check_transition(
    current: &OrdinaryComponent,
    proposed: &OrdinaryComponent,
) -> std::result::Result<(), OrdinaryTransitionRequired> {
    let (current_role, current_kind) = record_kind(current);
    let (proposed_role, proposed_kind) = record_kind(proposed);
    if current_role != proposed_role {
        return Err(OrdinaryTransitionRequired(format!(
            "cannot change a {current_role} into a {proposed_role}; delete it and create a new component"
        )));
    }
    if current_kind != proposed_kind {
        return Err(OrdinaryTransitionRequired(format!(
            "cannot change {current_role} kind from '{}' to '{}'; delete it and create a new component",
            current_kind.unwrap_or_default(),
            proposed_kind.unwrap_or_default()
        )));
    }
    Ok(())
}

/// Component references of one ordinary record.
pub fn references(record: &OrdinaryComponentRecord) -> Vec<ComponentReference> {
    match &record.component {
        OrdinaryComponent::Reaction { inputs, .. }
        | OrdinaryComponent::ProcessLocal { inputs, .. } => inputs.clone(),
        OrdinaryComponent::Query { .. } => record
            .query_configuration()
            .map(|query| query.inputs.into_iter().map(|i| i.component).collect())
            .unwrap_or_default(),
        OrdinaryComponent::Source { .. } => Vec::new(),
    }
}

/// Names referenced by ordinary records that do not resolve to any component.
pub fn unresolved_references(desired: &DesiredInstance) -> Vec<(String, String)> {
    let mut unresolved = Vec::new();
    for record in &desired.ordinary {
        for reference in references(record) {
            if let Some(name) = reference.as_name() {
                unresolved.push((record.name.to_string(), name.to_string()));
            }
        }
    }
    unresolved
}

/// Ensure every name reference in a normalized desired state resolved.
pub fn ensure_references_resolved(desired: &DesiredInstance) -> Result<()> {
    if let Some((component, name)) = unresolved_references(desired).into_iter().next() {
        anyhow::bail!("component '{component}' references unknown component '{name}'");
    }
    Ok(())
}

/// Instance settings needed to validate and reconstruct ordinary components.
#[derive(Clone, Default)]
pub struct InstanceComponentSettings {
    pub identity_providers: HashMap<String, Arc<dyn IdentityProvider>>,
    pub bootstrap_providers: HashMap<String, BootstrapProviderConfig>,
}

impl InstanceComponentSettings {
    /// Validate references to instance-level providers and registered kinds.
    pub fn validate_source(&self, registry: &PluginRegistry, config: &SourceConfig) -> Result<()> {
        self.validate_source_implementation(registry, config)?;
        self.validate_identity("Source", config.id(), config.identity_provider())
    }

    /// Validate kinds and bootstrap references, but not identity references.
    pub fn validate_source_implementation(
        &self,
        registry: &PluginRegistry,
        config: &SourceConfig,
    ) -> Result<()> {
        anyhow::ensure!(
            registry.get_source(config.kind()).is_some(),
            "Unknown source kind: '{}'",
            config.kind()
        );
        if let Some(id) = config
            .bootstrap_provider()
            .and_then(|provider| provider.as_reference())
        {
            anyhow::ensure!(
                self.bootstrap_providers.contains_key(id),
                "Source '{}' references unknown bootstrapProvider '{id}'",
                config.id()
            );
        }
        if let Some(inline) = config
            .bootstrap_provider()
            .and_then(|provider| provider.as_inline())
        {
            anyhow::ensure!(
                registry.get_bootstrapper(inline.kind()).is_some(),
                "Unknown bootstrap kind: '{}'",
                inline.kind()
            );
        }
        Ok(())
    }

    pub fn validate_reaction(
        &self,
        registry: &PluginRegistry,
        config: &ReactionConfig,
    ) -> Result<()> {
        anyhow::ensure!(
            registry.get_reaction(&config.kind).is_some(),
            "Unknown reaction kind: '{}'",
            config.kind
        );
        self.validate_identity("Reaction", config.id(), config.identity_provider())
    }

    fn validate_identity(&self, role: &str, id: &str, provider: Option<&str>) -> Result<()> {
        if let Some(provider) = provider {
            anyhow::ensure!(
                self.identity_providers.contains_key(provider),
                "{role} '{id}' references unknown identityProvider '{provider}'"
            );
        }
        Ok(())
    }

    fn identity(
        &self,
        role: &str,
        provider: Option<&str>,
    ) -> Result<Option<Arc<dyn IdentityProvider>>> {
        provider
            .map(|id| {
                self.identity_providers.get(id).cloned().ok_or_else(|| {
                    anyhow::anyhow!("{role} references unknown identityProvider '{id}'")
                })
            })
            .transpose()
    }
}

/// Declared YAML components used as the first-start seed of a persisted instance.
#[derive(Clone, Copy, Default)]
pub struct OrdinarySeed<'a> {
    pub sources: &'a [SourceConfig],
    pub queries: &'a [QueryConfig],
    pub reactions: &'a [ReactionConfig],
}

impl OrdinarySeed<'_> {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.queries.is_empty() && self.reactions.is_empty()
    }

    /// Assign UUIDs and encode the declared components. Kinds and provider
    /// references are validated; component references are checked after
    /// normalization by [`ensure_references_resolved`].
    pub fn records(
        &self,
        registry: &PluginRegistry,
        settings: &InstanceComponentSettings,
    ) -> Result<Vec<OrdinaryComponentRecord>> {
        let mut records = Vec::new();
        for source in self.sources {
            settings.validate_source(registry, source)?;
            records.push(source_definition(source)?.with_assigned_id(ComponentId::generate()));
        }
        for query in self.queries {
            let id = query.id.clone().unwrap_or_else(ComponentId::generate);
            records.push(
                OrdinaryComponentDefinition::query(query.clone())
                    .with_context(|| format!("invalid query '{}'", query.name))?
                    .with_assigned_id(id),
            );
        }
        for reaction in self.reactions {
            settings.validate_reaction(registry, reaction)?;
            records.push(reaction_definition(reaction)?.with_assigned_id(ComponentId::generate()));
        }
        Ok(records)
    }
}

/// Server's [`OrdinaryComponentResolver`] for a persisted instance.
pub struct ServerOrdinaryResolver {
    plugins: Arc<RwLock<PluginRegistry>>,
    settings: InstanceComponentSettings,
}

impl ServerOrdinaryResolver {
    pub fn new(plugins: Arc<RwLock<PluginRegistry>>, settings: InstanceComponentSettings) -> Self {
        Self { plugins, settings }
    }
}

#[async_trait]
impl OrdinaryComponentResolver for ServerOrdinaryResolver {
    fn plan_update(
        &self,
        _instance_id: &str,
        current: &OrdinaryComponentRecord,
        proposed: &OrdinaryComponentRecord,
    ) -> Result<ComponentUpdatePlan> {
        check_transition(&current.component, &proposed.component)?;
        // Core only supports preserving component data across ordinary updates.
        Ok(ComponentUpdatePlan {
            mode: ComponentUpdateMode::Recreate,
            data: ComponentUpdateData::Preserve,
        })
    }

    async fn create_source(
        &self,
        _instance_id: &str,
        record: &OrdinaryComponentRecord,
        _update: Option<&ComponentUpdateDirective>,
    ) -> Result<(Box<dyn Source>, HashMap<String, String>)> {
        let config = source_config(record)?;
        let identity = self
            .settings
            .identity("Source", config.identity_provider())?;
        let config = crate::factories::resolve_source_bootstrap_provider(
            config,
            &self.settings.bootstrap_providers,
        )?;
        let (source, metadata) =
            crate::factories::create_source_locked(&self.plugins, record.id.clone(), config)
                .await?;
        if let Some(identity) = identity {
            source.set_identity_provider(identity).await;
        }
        Ok((source, metadata))
    }

    async fn create_reaction(
        &self,
        _instance_id: &str,
        record: &OrdinaryComponentRecord,
        inputs: &[ComponentId],
        _update: Option<&ComponentUpdateDirective>,
    ) -> Result<(Box<dyn Reaction>, HashMap<String, String>)> {
        let config = reaction_config(
            record,
            inputs
                .iter()
                .cloned()
                .map(ComponentReference::by_id)
                .collect(),
        )?;
        let identity = self
            .settings
            .identity("Reaction", config.identity_provider())?;
        let (reaction, metadata) =
            crate::factories::create_reaction_locked(&self.plugins, record.id.clone(), config)
                .await?;
        if let Some(identity) = identity {
            reaction.set_identity_provider(identity).await;
        }
        Ok((reaction, metadata))
    }
}

/// Map each component UUID in a desired state to its name.
pub fn component_names(desired: &DesiredInstance) -> BTreeMap<ComponentId, ComponentName> {
    desired
        .topology
        .components
        .iter()
        .map(|component| (component.descriptor.id().clone(), component.name.clone()))
        .chain(
            desired
                .ordinary
                .iter()
                .map(|record| (record.id.clone(), record.name.clone())),
        )
        .collect()
}

/// Replace UUID references with names for human-readable rendering.
pub fn named_reference(
    names: &BTreeMap<ComponentId, ComponentName>,
    reference: ComponentReference,
) -> ComponentReference {
    match reference.as_id().and_then(|id| names.get(id)) {
        Some(name) => ComponentReference::by_name(name.clone()),
        None => reference,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    fn source() -> SourceConfig {
        serde_json::from_value(json!({
            "kind": "mock",
            "id": "orders",
            "autoStart": false,
            "identityProvider": "azure",
            "bootstrapProvider": "shared",
            "host": "db.example",
            "password": {"kind": "Secret", "name": "db-password"},
        }))
        .unwrap()
    }

    #[test]
    fn source_round_trip_keeps_secret_references_unresolved() {
        let definition = source_definition(&source()).unwrap();
        let OrdinaryComponent::Source { configuration, .. } = &definition.component else {
            panic!("expected a source");
        };
        for field in SOURCE_FIELDS {
            assert!(
                configuration.get(field).is_none(),
                "{field} must be stripped"
            );
        }
        assert_eq!(
            configuration["password"],
            json!({"kind": "Secret", "name": "db-password"})
        );
        let record = definition.with_assigned_id(ComponentId::generate());
        let rebuilt = source_config(&record).unwrap();
        assert_eq!(
            serde_json::to_value(rebuilt).unwrap(),
            serde_json::to_value(source()).unwrap()
        );
    }

    #[test]
    fn reaction_queries_become_record_inputs() {
        let config: ReactionConfig = serde_json::from_value(json!({
            "kind": "log",
            "id": "audit",
            "queries": ["orders-query"],
            "autoStart": true,
            "token": {"kind": "Secret", "name": "token"},
        }))
        .unwrap();
        let record = reaction_definition(&config)
            .unwrap()
            .with_assigned_id(ComponentId::generate());
        assert_eq!(
            references(&record),
            vec![ComponentReference::by_name(
                ComponentName::try_new("orders-query").unwrap()
            )]
        );
        let rebuilt = reaction_config(&record, references(&record)).unwrap();
        assert_eq!(
            serde_json::to_value(rebuilt).unwrap(),
            serde_json::to_value(config).unwrap()
        );
    }

    #[test]
    fn kind_and_role_changes_require_transition() {
        let current = source_definition(&source()).unwrap().component;
        let mut changed = source();
        changed.kind = "other".into();
        let proposed = source_definition(&changed).unwrap().component;
        assert!(check_transition(&current, &proposed).is_err());
        let reaction = OrdinaryComponent::Reaction {
            kind: "mock".into(),
            configuration: json!({}),
            inputs: Vec::new(),
        };
        assert!(check_transition(&current, &reaction).is_err());
        assert!(check_transition(&current, &current).is_ok());
    }
}
