// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.

//! Instances with a `configurationStore`: seeding, restart and REST
//! desired-state writes.

#![allow(clippy::unwrap_used)]

mod test_support;

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use drasi_lib::{
    config::{ComponentReference, QueryConfig},
    management::OrdinaryComponent,
    ComponentId, ComponentName, DrasiLib, Query,
};
use drasi_plugin_sdk::{ReactionPluginDescriptor, SourcePluginDescriptor};
use drasi_server::{
    api::models::{BootstrapProviderConfig, ConfigValue},
    api::v1::routes::build_v1_router,
    instance_registry::InstanceRegistry,
    managed_configuration::{ConfigurationStoreConfig, ConfigurationStores},
    ordinary_resolver::{InstanceComponentSettings, OrdinarySeed},
    persistence::ConfigPersistence,
    DrasiLibInstanceConfig, DrasiServerConfig, PluginRegistry, ReactionConfig, SourceConfig,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const INSTANCE: &str = "persisted";

struct MockSourceDescriptor(&'static str);

#[async_trait]
impl SourcePluginDescriptor for MockSourceDescriptor {
    fn kind(&self) -> &str {
        self.0
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    fn config_schema_name(&self) -> &str {
        "MockSourceConfig"
    }
    async fn create_source(
        &self,
        id: &ComponentId,
        _config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::Source>> {
        Ok(Box::new(
            test_support::MockSource::with_id(id.clone()).with_auto_start(auto_start),
        ))
    }
}

struct MockReactionDescriptor;

#[async_trait]
impl ReactionPluginDescriptor for MockReactionDescriptor {
    fn kind(&self) -> &str {
        "log"
    }
    fn config_version(&self) -> &str {
        "1.0.0"
    }
    fn config_schema_json(&self) -> String {
        r#"{"type":"object"}"#.into()
    }
    fn config_schema_name(&self) -> &str {
        "MockReactionConfig"
    }
    async fn create_reaction(
        &self,
        id: &ComponentId,
        queries: Vec<ComponentReference>,
        _config: &Value,
        auto_start: bool,
    ) -> anyhow::Result<Box<dyn drasi_lib::Reaction>> {
        let queries = queries
            .into_iter()
            .map(|query| match query.as_name() {
                Some(name) => name.to_string(),
                None => format!("{query:?}"),
            })
            .collect();
        Ok(Box::new(
            test_support::MockReaction::with_id(id.clone(), queries).with_auto_start(auto_start),
        ))
    }
}

fn plugins() -> Arc<RwLock<PluginRegistry>> {
    let registry = query_plugin::registry();
    {
        let mut plugins = registry.try_write().unwrap();
        plugins.register_source(Arc::new(MockSourceDescriptor("mock")));
        plugins.register_source(Arc::new(MockSourceDescriptor("other-mock")));
        plugins.register_reaction(Arc::new(MockReactionDescriptor));
        drasi_server::register_core_plugins(&mut plugins);
    }
    registry
}

fn store(directory: &Path) -> ConfigurationStoreConfig {
    ConfigurationStoreConfig::Redb {
        path: directory.join("configuration.redb"),
        key_file: directory.join("configuration.key"),
    }
}

fn source(id: &str, kind: &str) -> SourceConfig {
    serde_json::from_value(json!({
        "kind": kind,
        "id": id,
        "autoStart": false,
        "password": {"kind": "Secret", "name": "db-password"},
    }))
    .unwrap()
}

fn query(id: &str, input: &str) -> QueryConfig {
    Query::cypher(ComponentName::try_new(id).unwrap())
        .query("MATCH (n) RETURN n")
        .from_input_by_name(ComponentName::try_new(input).unwrap())
        .auto_start(false)
        .build()
        .unwrap()
}

fn reaction(id: &str, query: &str) -> ReactionConfig {
    serde_json::from_value(json!({
        "kind": "log",
        "id": id,
        "queries": [query],
        "autoStart": false,
    }))
    .unwrap()
}

struct Seed {
    sources: Vec<SourceConfig>,
    queries: Vec<QueryConfig>,
    reactions: Vec<ReactionConfig>,
}

fn pipeline_seed() -> Seed {
    Seed {
        sources: vec![source("orders", "mock")],
        queries: vec![query("orders-query", "orders")],
        reactions: vec![reaction("audit", "orders-query")],
    }
}

async fn build(directory: &Path, seed: &Seed) -> anyhow::Result<Arc<DrasiLib>> {
    build_with(directory, seed, InstanceComponentSettings::default()).await
}

async fn build_with(
    directory: &Path,
    seed: &Seed,
    settings: InstanceComponentSettings,
) -> anyhow::Result<Arc<DrasiLib>> {
    let builder = DrasiLib::builder()
        .with_id(INSTANCE)
        .with_allow_external_component_ids(true);
    let builder = ConfigurationStores::default()
        .configure_with_components(
            builder,
            INSTANCE,
            &store(directory),
            None,
            OrdinarySeed {
                sources: &seed.sources,
                queries: &seed.queries,
                reactions: &seed.reactions,
            },
            settings,
            plugins(),
            None,
        )
        .await?;
    let core = Arc::new(builder.build().await?);
    core.start().await?;
    Ok(core)
}

fn names(core: &DrasiLib) -> Vec<(String, String, ComponentId)> {
    let mut names: Vec<_> = core
        .desired_configuration()
        .unwrap()
        .desired
        .ordinary
        .into_iter()
        .map(|record| {
            let role = match record.component {
                OrdinaryComponent::Source { .. } => "source",
                OrdinaryComponent::Query { .. } => "query",
                OrdinaryComponent::Reaction { .. } => "reaction",
                OrdinaryComponent::ProcessLocal { .. } => "process-local",
            };
            (role.to_string(), record.name.to_string(), record.id)
        })
        .collect();
    names.sort();
    names
}

fn role_names(core: &DrasiLib) -> Vec<(String, String)> {
    names(core)
        .into_iter()
        .map(|(role, name, _)| (role, name))
        .collect()
}

fn pair(role: &str, name: &str) -> (String, String) {
    (role.to_string(), name.to_string())
}

struct Api {
    router: Router,
    config_path: PathBuf,
}

fn api(directory: &Path, core: Arc<DrasiLib>, config_path: PathBuf) -> Api {
    api_with(
        directory,
        core,
        config_path,
        DrasiLibInstanceConfig::default(),
    )
}

fn api_with(
    directory: &Path,
    core: Arc<DrasiLib>,
    config_path: PathBuf,
    instance: DrasiLibInstanceConfig,
) -> Api {
    let registry = InstanceRegistry::from_map([(INSTANCE.to_string(), core)].into_iter().collect());
    let original = DrasiServerConfig {
        instances: vec![DrasiLibInstanceConfig {
            id: ConfigValue::Static(INSTANCE.to_string()).into(),
            configuration_store: Some(store(directory)),
            ..instance
        }],
        ..Default::default()
    };
    let persistence = Arc::new(
        ConfigPersistence::new(
            config_path.clone(),
            registry.clone(),
            "127.0.0.1".into(),
            8080,
            "info".into(),
            true,
            &original,
        )
        .unwrap(),
    );
    let router = build_v1_router(
        registry,
        Arc::new(false),
        Some(persistence),
        plugins(),
        None,
    );
    Api {
        router,
        config_path,
    }
}

async fn send(
    api: &Api,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/instances/{INSTANCE}/{uri}"))
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = api
        .router
        .clone()
        .oneshot(
            request
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn change(body: &Value) -> &Value {
    &body["data"]["configuration"]
}

#[tokio::test]
async fn generates_key_and_fails_closed_without_it() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let key_file = directory.path().join("configuration.key");
    assert_eq!(std::fs::read(&key_file).unwrap().len(), 32);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key_file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    core.stop().await.unwrap();
    drop(core);

    std::fs::remove_file(&key_file).unwrap();
    let error = build(directory.path(), &pipeline_seed())
        .await
        .err()
        .expect("a database without its key must fail closed");
    assert!(
        format!("{error:#}").contains("key file is missing"),
        "{error:#}"
    );
    assert!(!key_file.exists(), "the key must not be regenerated");
}

#[tokio::test]
async fn seeds_on_first_start_and_store_is_authoritative_on_restart() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let first = names(&core);
    assert_eq!(
        role_names(&core),
        vec![
            pair("query", "orders-query"),
            pair("reaction", "audit"),
            pair("source", "orders"),
        ]
    );
    assert!(core
        .configuration_receipt(drasi_server::managed_configuration::INITIAL_REQUEST_ID)
        .await
        .unwrap()
        .is_some());
    assert!(core
        .get_source_status_by_name(&name("orders"))
        .await
        .is_ok());
    core.stop().await.unwrap();
    drop(core);

    // Changed YAML is ignored once the store holds a desired state.
    let changed = Seed {
        sources: vec![source("replacement", "mock")],
        queries: Vec::new(),
        reactions: Vec::new(),
    };
    let core = build(directory.path(), &changed).await.unwrap();
    assert_eq!(names(&core), first, "UUIDs and records survive restart");
    assert!(core
        .get_source_status_by_name(&name("orders"))
        .await
        .is_ok());
    assert!(core
        .get_source_status_by_name(&name("replacement"))
        .await
        .is_err());
    core.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_seed_reference_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let seed = Seed {
        sources: Vec::new(),
        queries: Vec::new(),
        reactions: vec![reaction("audit", "missing-query")],
    };
    assert!(build(directory.path(), &seed).await.is_err());
}

fn name(value: &str) -> ComponentName {
    ComponentName::try_new(value).unwrap()
}

#[tokio::test]
async fn rest_writes_update_desired_state_and_write_back() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let api = api(
        directory.path(),
        core.clone(),
        directory.path().join("server.yaml"),
    );

    // Create.
    let created = serde_json::to_value(source("inventory", "mock")).unwrap();
    let (status, body) = send(&api, Method::POST, "sources", &[], created.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let component_id = change(&body)["componentId"].as_str().unwrap().to_string();
    assert_eq!(change(&body)["durable"], true);
    assert!(change(&body)["writeBackError"].is_null(), "{body}");
    assert!(role_names(&core).contains(&pair("source", "inventory")));
    assert!(core
        .get_source_status_by_name(&name("inventory"))
        .await
        .is_ok());

    // Duplicate create.
    let (status, _) = send(&api, Method::POST, "sources", &[], created).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // PUT updates in place.
    let mut updated = source("inventory", "mock");
    updated.config = json!({"password": {"kind": "Secret", "name": "rotated"}});
    let (status, body) = send(
        &api,
        Method::PUT,
        "sources/inventory",
        &[],
        serde_json::to_value(&updated).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(change(&body)["componentId"], component_id);

    // A kind change requires an explicit transition.
    let (status, body) = send(
        &api,
        Method::PUT,
        "sources/inventory",
        &[],
        serde_json::to_value(source("inventory", "other-mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CONFIGURATION_TRANSITION_REQUIRED");

    // PUT on a missing component creates it.
    let (status, body) = send(
        &api,
        Method::PUT,
        "sources/fresh",
        &[],
        serde_json::to_value(source("fresh", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Reactions and queries.
    let (status, body) = send(
        &api,
        Method::POST,
        "queries",
        &[],
        json!({
            "name": "inventory-query",
            "query": "MATCH (n) RETURN n",
            "inputs": ["inventory"],
            "autoStart": false,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        &api,
        Method::POST,
        "reactions",
        &[],
        serde_json::to_value(reaction("inventory-log", "inventory-query")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A referenced component cannot be removed.
    let (status, body) = send(&api, Method::DELETE, "sources/orders", &[], Value::Null).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = send(&api, Method::DELETE, "reactions/audit", &[], Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(&api, Method::DELETE, "sources/fresh", &[], Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        role_names(&core),
        vec![
            pair("query", "inventory-query"),
            pair("query", "orders-query"),
            pair("reaction", "inventory-log"),
            pair("source", "inventory"),
            pair("source", "orders"),
        ]
    );

    // The rendered YAML is the desired state, with secret references intact.
    let yaml = std::fs::read_to_string(&api.config_path).unwrap();
    let rendered: Value = serde_yaml::from_str(&yaml).unwrap();
    let instance = &rendered["instances"][0];
    assert!(instance["configurationStore"].is_object(), "{yaml}");
    let source_ids: Vec<_> = instance["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["id"].as_str().unwrap())
        .collect();
    assert_eq!(source_ids.len(), 2, "{yaml}");
    assert!(source_ids.contains(&"inventory") && source_ids.contains(&"orders"));
    assert!(yaml.contains("rotated"), "{yaml}");
    assert!(!yaml.contains("fresh"), "{yaml}");
    assert!(yaml.contains("inventory-query") && yaml.contains("inventory-log"));
    assert!(
        !yaml.contains(&component_id),
        "YAML names components rather than UUIDs: {yaml}"
    );
    core.stop().await.unwrap();
}

#[tokio::test]
async fn idempotency_key_and_if_match() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let api = api(
        directory.path(),
        core.clone(),
        directory.path().join("server.yaml"),
    );
    let body = serde_json::to_value(source("keyed", "mock")).unwrap();
    let headers = [("Idempotency-Key", "create-keyed-1")];
    let (status, first) = send(&api, Method::POST, "sources", &headers, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(change(&first)["requestId"], "create-keyed-1");
    let (status, second) = send(&api, Method::POST, "sources", &headers, body).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(change(&second)["requestId"], "create-keyed-1");
    assert_eq!(change(&second)["revision"], change(&first)["revision"]);
    assert_eq!(
        role_names(&core)
            .iter()
            .filter(|(_, name)| name == "keyed")
            .count(),
        1
    );

    let revision = core.desired_configuration().unwrap().revision;
    let stale = (revision - 1).to_string();
    let (status, body) = send(
        &api,
        Method::POST,
        "sources",
        &[("If-Match", stale.as_str())],
        serde_json::to_value(source("stale", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CONFIGURATION_CONFLICT");

    let current = format!("\"{revision}\"");
    let (status, body) = send(
        &api,
        Method::POST,
        "sources",
        &[("If-Match", current.as_str())],
        serde_json::to_value(source("current", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(change(&body)["revision"], revision + 1);

    let (status, _) = send(
        &api,
        Method::POST,
        "sources",
        &[("Idempotency-Key", "drasi-server/reserved")],
        serde_json::to_value(source("reserved", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    core.stop().await.unwrap();
}

#[tokio::test]
async fn write_back_failure_keeps_the_change() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let api = api(
        directory.path(),
        core.clone(),
        directory
            .path()
            .join("missing-directory")
            .join("server.yaml"),
    );
    let (status, body) = send(
        &api,
        Method::POST,
        "sources",
        &[],
        serde_json::to_value(source("kept", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(change(&body)["writeBackError"].is_string(), "{body}");
    assert!(role_names(&core).contains(&pair("source", "kept")));
    core.stop().await.unwrap();
}

#[tokio::test]
async fn changes_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let api = api(
        directory.path(),
        core.clone(),
        directory.path().join("server.yaml"),
    );
    let (status, body) = send(
        &api,
        Method::POST,
        "sources",
        &[],
        serde_json::to_value(source("added", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let before = names(&core);
    core.stop().await.unwrap();
    drop(api);
    drop(core);

    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    assert_eq!(names(&core), before);
    assert!(core.get_source_status_by_name(&name("added")).await.is_ok());
    core.stop().await.unwrap();
}

#[tokio::test]
async fn solution_deploy_is_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let core = build(directory.path(), &pipeline_seed()).await.unwrap();
    let api = api(
        directory.path(),
        core.clone(),
        directory.path().join("server.yaml"),
    );
    let before = names(&core);
    let invalid = r#"
name: Broken
sources:
  - kind: mock
    id: solution-source
    autoStart: false
reactions:
  - kind: log
    id: solution-logger
    queries: [missing-query]
    autoStart: false
"#;
    let (status, body) = send(
        &api,
        Method::POST,
        "solutions",
        &[],
        json!({"yaml": invalid}),
    )
    .await;
    assert!(!status.is_success(), "{status}: {body}");
    assert_eq!(names(&core), before, "a failed deployment adds nothing");

    let valid = r#"
name: Working
sources:
  - kind: mock
    id: solution-source
    autoStart: false
queries:
  - name: solution-query
    query: "MATCH (n) RETURN n"
    inputs: [solution-source]
    autoStart: false
reactions:
  - kind: log
    id: solution-logger
    queries: [solution-query]
    autoStart: false
"#;
    let (status, body) = send(&api, Method::POST, "solutions", &[], json!({"yaml": valid})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names = role_names(&core);
    for expected in [
        pair("source", "solution-source"),
        pair("query", "solution-query"),
        pair("reaction", "solution-logger"),
    ] {
        assert!(names.contains(&expected), "{expected:?} in {names:?}");
    }
    core.stop().await.unwrap();
}

fn provider_settings() -> InstanceComponentSettings {
    let mut settings = InstanceComponentSettings::default();
    settings.identity_providers.insert(
        "service-account".into(),
        Arc::new(drasi_lib::identity::PasswordIdentityProvider::new(
            "user",
            "resolved-password",
        )),
    );
    settings.bootstrap_providers.insert(
        "shared".into(),
        BootstrapProviderConfig {
            kind: "noop".into(),
            config: json!({}),
        },
    );
    settings
}

#[tokio::test]
async fn provider_references_are_restored_by_name() {
    let directory = tempfile::tempdir().unwrap();
    let with_providers: SourceConfig = serde_json::from_value(json!({
        "kind": "mock",
        "id": "orders",
        "autoStart": false,
        "identityProvider": "service-account",
        "bootstrapProvider": "shared",
    }))
    .unwrap();
    let seed = Seed {
        sources: vec![with_providers],
        queries: Vec::new(),
        reactions: Vec::new(),
    };

    // Unknown instance-level providers are rejected while seeding.
    let missing = tempfile::tempdir().unwrap();
    assert!(build(missing.path(), &seed).await.is_err());

    let core = build_with(directory.path(), &seed, provider_settings())
        .await
        .unwrap();
    let record = core.desired_configuration().unwrap().desired.ordinary[0].clone();
    let OrdinaryComponent::Source { configuration, .. } = &record.component else {
        panic!("expected a source");
    };
    assert_eq!(configuration["identityProvider"], "service-account");
    assert_eq!(configuration["bootstrapProvider"], "shared");
    let declared: DrasiLibInstanceConfig = serde_json::from_value(json!({
        "identityProviders": [{
            "kind": "password",
            "id": "service-account",
            "username": "user",
            "password": {"kind": "Secret", "name": "service-password"},
        }],
        "bootstrapProviders": [{"id": "shared", "kind": "noop"}],
    }))
    .unwrap();
    let api = api_with(
        directory.path(),
        core.clone(),
        directory.path().join("server.yaml"),
        declared,
    );
    let (status, body) = send(
        &api,
        Method::POST,
        "sources",
        &[],
        serde_json::to_value(source("trigger-write", "mock")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(change(&body)["writeBackError"].is_null(), "{body}");
    let yaml = std::fs::read_to_string(&api.config_path).unwrap();
    assert!(
        yaml.contains("service-account") && yaml.contains("shared"),
        "{yaml}"
    );
    assert!(!yaml.contains("resolved-password"), "{yaml}");
    core.stop().await.unwrap();
    drop(api);
    drop(core);

    // Restart resolves the stored references against the instance settings.
    let core = build_with(directory.path(), &seed, provider_settings())
        .await
        .unwrap();
    assert!(core
        .get_source_status_by_name(&name("orders"))
        .await
        .is_ok());
    core.stop().await.unwrap();
}

#[path = "query_plugin/mod.rs"]
mod query_plugin;
