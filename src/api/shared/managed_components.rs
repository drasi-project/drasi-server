// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Component REST writes on instances with a `configurationStore`.
//!
//! Each create, update or delete becomes one desired-state request. Clients may
//! supply `Idempotency-Key` (the request ID) and `If-Match` (the expected
//! desired-state revision); otherwise the Server generates a request ID and
//! uses the current revision.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};
use drasi_lib::{
    computation::v1::{ComponentId, ComponentName, GraphError},
    management::{
        AcceptanceReceipt, ConfigurationRequest, ManagementError, NewComponentDefinition,
        OrdinaryComponentDefinition, OrdinaryComponentRecord, RecoveryTransitionRequired,
    },
    DrasiError, DrasiLib,
};

use super::error::{error_codes, ErrorDetail, ErrorResponse};
use super::responses::{ConfigurationChange, StatusResponse};
use crate::api::models::{BootstrapProviderConfig, ReactionConfig, SourceConfig};
use crate::ordinary_resolver::{
    check_transition, reaction_definition, record_kind, source_definition,
    InstanceComponentSettings, OrdinaryTransitionRequired,
};
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;

const RECONCILE_TIMEOUT: Duration = Duration::from_secs(30);

/// Client-supplied retry identity and optimistic-concurrency precondition.
#[derive(Debug, Clone, Default)]
pub struct ConfigurationPreconditions {
    pub request_id: Option<String>,
    pub expected_revision: Option<u64>,
}

tokio::task_local! {
    static PRECONDITIONS: ConfigurationPreconditions;
}

impl ConfigurationPreconditions {
    pub fn from_headers(headers: &HeaderMap) -> Result<Self, ErrorResponse> {
        let request_id = headers
            .get("idempotency-key")
            .map(|value| {
                let value = value.to_str().map_err(|_| invalid_key())?;
                if value.is_empty() || value.len() > 1024 || value.starts_with("drasi-server/") {
                    return Err(invalid_key());
                }
                Ok(value.to_string())
            })
            .transpose()?;
        let expected_revision = headers
            .get(axum::http::header::IF_MATCH)
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .map(|value| value.trim().trim_start_matches("W/").trim_matches('"'))
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        ErrorResponse::new(
                            error_codes::INVALID_REQUEST,
                            "If-Match must be a desired-state revision number",
                        )
                    })
            })
            .transpose()?;
        Ok(Self {
            request_id,
            expected_revision,
        })
    }

    /// Preconditions of the current request, or none outside the API layer.
    pub fn current() -> Self {
        PRECONDITIONS.try_with(Clone::clone).unwrap_or_default()
    }

    /// Run `future` with these preconditions visible to [`Self::current`].
    pub async fn scope<F: std::future::Future>(self, future: F) -> F::Output {
        PRECONDITIONS.scope(self, future).await
    }
}

fn invalid_key() -> ErrorResponse {
    ErrorResponse::new(
        error_codes::INVALID_REQUEST,
        "Idempotency-Key must be a nonempty value of at most 1024 bytes outside the reserved drasi-server/ prefix",
    )
}

/// Router middleware exposing `Idempotency-Key` and `If-Match` to handlers.
pub async fn preconditions_middleware(request: Request, next: Next) -> Response {
    let has_headers = request.headers().contains_key("idempotency-key")
        || request.headers().contains_key(axum::http::header::IF_MATCH);
    if !has_headers {
        return next.run(request).await;
    }
    match ConfigurationPreconditions::from_headers(request.headers()) {
        Ok(preconditions) => preconditions.scope(next.run(request)).await,
        Err(error) => error.into_response(),
    }
}

/// Role checked against an existing component of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    Source,
    Query,
    Reaction,
}

impl ComponentRole {
    fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Query => "query",
            Self::Reaction => "reaction",
        }
    }

    fn not_found_code(self) -> &'static str {
        match self {
            Self::Source => error_codes::SOURCE_NOT_FOUND,
            Self::Query => error_codes::QUERY_NOT_FOUND,
            Self::Reaction => error_codes::REACTION_NOT_FOUND,
        }
    }
}

/// Whether a write may create, replace or only create a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Create,
    Upsert,
}

fn transition_required(message: impl Into<String>) -> ErrorResponse {
    ErrorResponse::new(error_codes::CONFIGURATION_TRANSITION_REQUIRED, message)
}

fn unconfirmed(request_id: &str) -> ErrorResponse {
    ErrorResponse::new(
        error_codes::CONFIGURATION_UNCONFIRMED,
        "Configuration change was not confirmed; retry with the same Idempotency-Key or inspect the receipt",
    )
    .with_details(ErrorDetail {
        component_type: None,
        component_id: None,
        technical_details: Some(format!("requestId: {request_id}")),
    })
}

fn invalid(component_type: &str, name: &str, error: impl std::fmt::Display) -> ErrorResponse {
    ErrorResponse::new(
        error_codes::INVALID_REQUEST,
        format!("Invalid {component_type} configuration"),
    )
    .with_details(ErrorDetail {
        component_type: Some(component_type.to_string()),
        component_id: Some(name.to_string()),
        technical_details: Some(format!("{error:#}")),
    })
}

fn read_error(error: DrasiError) -> ErrorResponse {
    if matches!(
        error.downcast_ref::<ManagementError>(),
        Some(ManagementError::ConfigurationUnconfirmed)
    ) {
        return ErrorResponse::new(
            error_codes::CONFIGURATION_UNCONFIRMED,
            "Desired state is unconfirmed; reconcile the instance before changing it",
        );
    }
    ErrorResponse::new(
        error_codes::INTERNAL_ERROR,
        "Failed to read desired configuration",
    )
}

enum Classified {
    Retry,
    Error(ErrorResponse),
}

/// Distinguish definite rejection from an unknown commit outcome.
async fn classify(
    core: &DrasiLib,
    request_id: &str,
    component_type: &str,
    name: &str,
    retry_allowed: bool,
    error: DrasiError,
) -> Result<AcceptanceReceipt, Classified> {
    if error.downcast_ref::<OrdinaryTransitionRequired>().is_some()
        || error
            .downcast_ref::<crate::computation::UnsupportedResourceUpdate>()
            .is_some()
        || error.downcast_ref::<RecoveryTransitionRequired>().is_some()
    {
        return Err(Classified::Error(
            transition_required(format!(
                "The {component_type} change requires removal and re-creation"
            ))
            .with_details(ErrorDetail {
                component_type: Some(component_type.to_string()),
                component_id: Some(name.to_string()),
                technical_details: Some(format!("{error:#}")),
            }),
        ));
    }
    match error.downcast_ref::<ManagementError>() {
        Some(ManagementError::RevisionConflict { .. }) if retry_allowed => {
            return Err(Classified::Retry)
        }
        Some(
            ManagementError::RevisionConflict { .. }
            | ManagementError::RequestConflict
            | ManagementError::RequestExpired
            | ManagementError::GeneratedRequestIdRequired
            | ManagementError::RequestBatchFull,
        ) => {
            return Err(Classified::Error(
                ErrorResponse::new(
                    error_codes::CONFIGURATION_CONFLICT,
                    "Configuration revision or request ID conflicts with an accepted request",
                )
                .with_details(ErrorDetail {
                    component_type: Some(component_type.to_string()),
                    component_id: Some(name.to_string()),
                    technical_details: Some(error.to_string()),
                }),
            ))
        }
        Some(ManagementError::ConfigurationUnconfirmed) => {
            return Err(Classified::Error(unconfirmed(request_id)))
        }
        _ => {}
    }
    // The outcome may be unknown: a recorded receipt proves acceptance.
    match core.configuration_receipt(request_id).await {
        Ok(Some(receipt)) => return Ok(receipt),
        Ok(None) => {}
        Err(_) => return Err(Classified::Error(unconfirmed(request_id))),
    }
    if core.desired_configuration().is_err() {
        return Err(Classified::Error(unconfirmed(request_id)));
    }
    if matches!(
        error.downcast_ref::<GraphError>(),
        Some(GraphError::DuplicateComponentName { .. } | GraphError::DuplicateComponentId { .. })
    ) {
        return Err(Classified::Error(ErrorResponse::new(
            error_codes::DUPLICATE_RESOURCE,
            "Resource already exists",
        )));
    }
    log::warn!("Configuration change for {component_type} '{name}' was rejected: {error:#}");
    Err(Classified::Error(invalid(component_type, name, error)))
}

struct Accepted {
    receipt: AcceptanceReceipt,
    component_id: Option<String>,
}

/// Create or replace one ordinary component by name.
pub async fn write_component(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    role: ComponentRole,
    mode: WriteMode,
    definition: OrdinaryComponentDefinition,
) -> Result<StatusResponse, ErrorResponse> {
    let component_type = role.label();
    let name = definition.name.clone();
    let preconditions = ConfigurationPreconditions::current();
    let request_id = request_id(core, &preconditions).await?;
    if let Some(receipt) = replayed(core, &request_id, &preconditions).await? {
        return finish(
            core,
            persistence,
            format!("{component_type} '{name}' change replayed"),
            Accepted {
                component_id: receipt
                    .creation
                    .as_ref()
                    .and_then(|creation| creation.components.first())
                    .map(|component| component.id.to_string()),
                receipt,
            },
        )
        .await;
    }
    let mut attempts = 0;
    let accepted = loop {
        attempts += 1;
        let committed = core.desired_configuration().map_err(read_error)?;
        let expected = preconditions
            .expected_revision
            .unwrap_or(committed.revision);
        if committed
            .desired
            .topology
            .components
            .iter()
            .any(|component| component.name == name)
        {
            return Err(match mode {
                WriteMode::Create => {
                    ErrorResponse::new(error_codes::DUPLICATE_RESOURCE, "Resource already exists")
                }
                WriteMode::Upsert => native_component(&name),
            });
        }
        let existing = committed
            .desired
            .ordinary
            .iter()
            .find(|record| record.name == name)
            .cloned();
        let (request, component_id) = match existing {
            Some(_) if mode == WriteMode::Create => {
                return Err(ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    "Resource already exists",
                ));
            }
            Some(existing) => {
                check_transition(&existing.component, &definition.component).map_err(|error| {
                    if record_kind(&existing.component).0 != component_type {
                        ErrorResponse::new(
                            error_codes::DUPLICATE_RESOURCE,
                            format!(
                                "A {} named '{name}' already exists",
                                record_kind(&existing.component).0
                            ),
                        )
                    } else {
                        transition_required(error.to_string())
                    }
                })?;
                let record = definition.clone().with_assigned_id(existing.id.clone());
                let mut desired = committed.desired.clone();
                for slot in &mut desired.ordinary {
                    if slot.id == existing.id {
                        *slot = record.clone();
                    }
                }
                let desired = validate_desired(desired, component_type, name.as_str())?;
                (
                    ConfigurationRequest {
                        desired: Some(desired),
                        additions: Vec::new(),
                    },
                    Some(existing.id.to_string()),
                )
            }
            None => {
                let mut desired = committed.desired.clone();
                let id = definition.id.clone().unwrap_or_else(ComponentId::generate);
                desired
                    .ordinary
                    .push(definition.clone().with_assigned_id(id));
                validate_desired(desired, component_type, name.as_str())?;
                (
                    ConfigurationRequest::adding([NewComponentDefinition::Ordinary(
                        definition.clone(),
                    )]),
                    None,
                )
            }
        };
        match core
            .apply_configuration(expected, request_id.clone(), request)
            .await
        {
            Ok(receipt) => break Accepted::new(receipt, component_id),
            Err(error) => {
                let retry = preconditions.expected_revision.is_none() && attempts < 3;
                match classify(
                    core,
                    &request_id,
                    component_type,
                    name.as_str(),
                    retry,
                    error,
                )
                .await
                {
                    Ok(receipt) => break Accepted::new(receipt, component_id),
                    Err(Classified::Retry) => continue,
                    Err(Classified::Error(error)) => return Err(error),
                }
            }
        }
    };
    finish(
        core,
        persistence,
        format!("{component_type} '{name}' change accepted"),
        accepted,
    )
    .await
}

/// Remove one ordinary component by name.
/// Native computation components change only through `computation/desired`.
fn native_component(name: &ComponentName) -> ErrorResponse {
    ErrorResponse::new(
        error_codes::MANAGED_CONFIGURATION_REQUIRED,
        format!(
            "'{name}' is a native computation component; submit a revisioned request to computation/desired"
        ),
    )
}

pub async fn delete_component(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    role: ComponentRole,
    name: &ComponentName,
) -> Result<StatusResponse, ErrorResponse> {
    let component_type = role.label();
    let preconditions = ConfigurationPreconditions::current();
    let request_id = request_id(core, &preconditions).await?;
    if let Some(receipt) = replayed(core, &request_id, &preconditions).await? {
        return finish(
            core,
            persistence,
            format!("{component_type} '{name}' deletion replayed"),
            Accepted::new(receipt, None),
        )
        .await;
    }
    let mut attempts = 0;
    let accepted = loop {
        attempts += 1;
        let committed = core.desired_configuration().map_err(read_error)?;
        let expected = preconditions
            .expected_revision
            .unwrap_or(committed.revision);
        if committed
            .desired
            .topology
            .components
            .iter()
            .any(|component| &component.name == name)
        {
            return Err(native_component(name));
        }
        let Some(existing) = committed
            .desired
            .ordinary
            .iter()
            .find(|record| {
                &record.name == name && record_kind(&record.component).0 == component_type
            })
            .cloned()
        else {
            return Err(ErrorResponse::new(
                role.not_found_code(),
                format!("{component_type} '{name}' not found"),
            ));
        };
        let mut desired = committed.desired.clone();
        desired.ordinary.retain(|record| record.id != existing.id);
        let desired = validate_desired(desired, component_type, name.as_str())
            .map_err(|_| dependent(component_type, name, &committed.desired.ordinary, &existing))?;
        let request = ConfigurationRequest {
            desired: Some(desired),
            additions: Vec::new(),
        };
        let component_id = Some(existing.id.to_string());
        match core
            .apply_configuration(expected, request_id.clone(), request)
            .await
        {
            Ok(receipt) => break Accepted::new(receipt, component_id),
            Err(error) => {
                if error.to_string().contains("is depended on by")
                    || format!("{error:#}").contains("is depended on by")
                {
                    return Err(dependent(
                        component_type,
                        name,
                        &committed.desired.ordinary,
                        &existing,
                    ));
                }
                let retry = preconditions.expected_revision.is_none() && attempts < 3;
                match classify(
                    core,
                    &request_id,
                    component_type,
                    name.as_str(),
                    retry,
                    error,
                )
                .await
                {
                    Ok(receipt) => break Accepted::new(receipt, component_id),
                    Err(Classified::Retry) => continue,
                    Err(Classified::Error(error)) => return Err(error),
                }
            }
        }
    };
    finish(
        core,
        persistence,
        format!("{component_type} '{name}' deleted"),
        accepted,
    )
    .await
}

fn dependent(
    component_type: &str,
    name: &ComponentName,
    records: &[OrdinaryComponentRecord],
    removed: &OrdinaryComponentRecord,
) -> ErrorResponse {
    let consumers: Vec<String> = records
        .iter()
        .filter(|record| {
            crate::ordinary_resolver::references(record)
                .iter()
                .any(|reference| {
                    reference.as_id() == Some(&removed.id) || reference.as_name() == Some(name)
                })
        })
        .map(|record| record.name.to_string())
        .collect();
    ErrorResponse::new(
        error_codes::CONFIGURATION_CONFLICT,
        format!("{component_type} '{name}' is used by other components; remove them first"),
    )
    .with_details(ErrorDetail {
        component_type: Some(component_type.to_string()),
        component_id: Some(name.to_string()),
        technical_details: Some(format!("dependents: {}", consumers.join(", "))),
    })
}

fn validate_desired(
    desired: drasi_lib::management::DesiredInstance,
    component_type: &str,
    name: &str,
) -> Result<drasi_lib::management::DesiredInstance, ErrorResponse> {
    let desired = desired
        .normalized()
        .map_err(|error| invalid(component_type, name, error))?;
    crate::ordinary_resolver::ensure_references_resolved(&desired)
        .map_err(|error| invalid(component_type, name, error))?;
    Ok(desired)
}

impl Accepted {
    fn new(receipt: AcceptanceReceipt, component_id: Option<String>) -> Self {
        let component_id = component_id.or_else(|| {
            receipt
                .creation
                .as_ref()
                .and_then(|creation| creation.components.first())
                .map(|component| component.id.to_string())
        });
        Self {
            receipt,
            component_id,
        }
    }
}

async fn request_id(
    core: &DrasiLib,
    preconditions: &ConfigurationPreconditions,
) -> Result<String, ErrorResponse> {
    match &preconditions.request_id {
        Some(id) => Ok(id.clone()),
        None => core.new_configuration_request_id().await.map_err(|_| {
            ErrorResponse::new(
                error_codes::CONFIGURATION_UNCONFIRMED,
                "Could not allocate a configuration request ID",
            )
        }),
    }
}

/// A client retry of an accepted request returns its original receipt. Creation
/// retries are verified against the original input by Core; update and delete
/// retries are matched by request ID only.
async fn replayed(
    core: &DrasiLib,
    request_id: &str,
    preconditions: &ConfigurationPreconditions,
) -> Result<Option<AcceptanceReceipt>, ErrorResponse> {
    if preconditions.request_id.is_none() {
        return Ok(None);
    }
    core.configuration_receipt(request_id.to_string())
        .await
        .map_err(|_| unconfirmed(request_id))
}

async fn finish(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    message: String,
    accepted: Accepted,
) -> Result<StatusResponse, ErrorResponse> {
    let Accepted {
        receipt,
        component_id,
    } = accepted;
    // Acceptance is durable; realization is best-effort within the request.
    let status = match tokio::time::timeout(RECONCILE_TIMEOUT, core.reconcile_desired_state()).await
    {
        Ok(Ok(status)) if status.converged() => "converged".to_string(),
        Ok(Ok(status)) => match status.error {
            Some(error) => format!("not converged: {error}"),
            None => "reconciling".to_string(),
        },
        Ok(Err(error)) => {
            log::warn!("Reconciliation after accepted change failed: {error}");
            "reconciliation failed; inspect computation/management".to_string()
        }
        Err(_) => "reconciling".to_string(),
    };
    let write_back_error = match persistence {
        Some(persistence) if receipt.durable => persistence.write_back().await.err(),
        _ => None,
    };
    Ok(StatusResponse {
        message,
        configuration: Some(ConfigurationChange {
            request_id: receipt.request_id,
            revision: receipt.revision,
            durable: receipt.durable,
            component_id,
            plan: receipt.updates.first().map(|update| update.plan),
            status,
            write_back_error,
        }),
    })
}

/// Validate a source declared through REST and encode it as an ordinary
/// definition. Identity provider references are checked when the resolver
/// constructs the source; an unknown one appears in the component status.
pub async fn source_component(
    plugins: &tokio::sync::RwLock<PluginRegistry>,
    bootstrap_providers: std::collections::HashMap<String, BootstrapProviderConfig>,
    config: &SourceConfig,
) -> Result<OrdinaryComponentDefinition, ErrorResponse> {
    let settings = InstanceComponentSettings {
        identity_providers: Default::default(),
        bootstrap_providers,
    };
    settings
        .validate_source_implementation(&*plugins.read().await, config)
        .map_err(|error| invalid("source", config.id(), error))?;
    source_definition(config).map_err(|error| invalid("source", config.id(), error))
}

/// Validate a reaction declared through REST and encode it as an ordinary definition.
pub async fn reaction_component(
    plugins: &tokio::sync::RwLock<PluginRegistry>,
    config: &ReactionConfig,
) -> Result<OrdinaryComponentDefinition, ErrorResponse> {
    if plugins.read().await.get_reaction(&config.kind).is_none() {
        return Err(invalid(
            "reaction",
            config.id(),
            format!("Unknown reaction kind: '{}'", config.kind),
        ));
    }
    reaction_definition(config).map_err(|error| invalid("reaction", config.id(), error))
}

/// Validate and write a source declared through REST.
pub async fn write_source(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    plugins: &tokio::sync::RwLock<PluginRegistry>,
    bootstrap_providers: std::collections::HashMap<String, BootstrapProviderConfig>,
    config: &SourceConfig,
    mode: WriteMode,
) -> Result<StatusResponse, ErrorResponse> {
    let definition = source_component(plugins, bootstrap_providers, config).await?;
    write_component(core, persistence, ComponentRole::Source, mode, definition).await
}

/// Validate and write a reaction declared through REST.
pub async fn write_reaction(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    plugins: &tokio::sync::RwLock<PluginRegistry>,
    config: &ReactionConfig,
    mode: WriteMode,
) -> Result<StatusResponse, ErrorResponse> {
    let definition = reaction_component(plugins, config).await?;
    write_component(core, persistence, ComponentRole::Reaction, mode, definition).await
}

/// Add several ordinary components in one atomic desired-state request.
pub async fn add_components(
    core: &DrasiLib,
    persistence: &Option<Arc<ConfigPersistence>>,
    component_type: &str,
    definitions: Vec<OrdinaryComponentDefinition>,
) -> Result<StatusResponse, ErrorResponse> {
    let preconditions = ConfigurationPreconditions::current();
    let request_id = request_id(core, &preconditions).await?;
    let names: Vec<String> = definitions
        .iter()
        .map(|definition| definition.name.to_string())
        .collect();
    let label = names.join(", ");
    if let Some(receipt) = replayed(core, &request_id, &preconditions).await? {
        return finish(
            core,
            persistence,
            format!("{component_type} replayed"),
            Accepted::new(receipt, None),
        )
        .await;
    }
    let mut attempts = 0;
    let accepted = loop {
        attempts += 1;
        let committed = core.desired_configuration().map_err(read_error)?;
        let expected = preconditions
            .expected_revision
            .unwrap_or(committed.revision);
        let mut desired = committed.desired.clone();
        for definition in &definitions {
            let taken = desired
                .topology
                .components
                .iter()
                .any(|component| component.name == definition.name)
                || desired
                    .ordinary
                    .iter()
                    .any(|record| record.name == definition.name);
            if taken {
                return Err(ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    format!("Component '{}' already exists", definition.name),
                ));
            }
            let id = definition.id.clone().unwrap_or_else(ComponentId::generate);
            desired
                .ordinary
                .push(definition.clone().with_assigned_id(id));
        }
        validate_desired(desired, component_type, &label)?;
        let request = ConfigurationRequest::adding(
            definitions
                .iter()
                .cloned()
                .map(NewComponentDefinition::Ordinary),
        );
        match core
            .apply_configuration(expected, request_id.clone(), request)
            .await
        {
            Ok(receipt) => break Accepted::new(receipt, None),
            Err(error) => {
                let retry = preconditions.expected_revision.is_none() && attempts < 3;
                match classify(core, &request_id, component_type, &label, retry, error).await {
                    Ok(receipt) => break Accepted::new(receipt, None),
                    Err(Classified::Retry) => continue,
                    Err(Classified::Error(error)) => return Err(error),
                }
            }
        }
    };
    finish(
        core,
        persistence,
        format!("{component_type} accepted"),
        accepted,
    )
    .await
}
