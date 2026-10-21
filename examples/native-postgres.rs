// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Reproduce native PostgreSQL consumer configurations from application recipes.
//! The running Server consumes only the generated canonical definition.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use drasi_host_sdk::{
    computation::{NativeFactory, NativePlugin},
    management::NativeBootstrapConfig,
};
use drasi_lib::computation::v1::*;
use drasi_server::{api::mappings::DtoMapper, computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};

const SOURCE: &str = "drasi.postgres/shared-changes";
const BOOTSTRAP: &str = "drasi.postgres/shared-row-bootstrap";
const HTTP_SOURCE: &str = "drasi.network/http-source";
const HTTP_BOOTSTRAP: &str = "drasi.network/http-bootstrap";

fn id(value: &str) -> Result<ComponentId> {
    Ok(ComponentId::try_new(value)?)
}
fn resource(value: &str) -> Result<ResourceId> {
    Ok(ResourceId::try_new(value)?)
}
fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(id(component)?, PortId::try_new(port)?))
}
fn stream(source: &str) -> Result<StreamId> {
    Ok(StreamId::try_new(format!("{source}/out"))?)
}
fn literal(value: impl serde::Serialize) -> Result<ConfigurationValue> {
    Ok(ConfigurationValue::Literal(serde_json::to_value(value)?))
}
fn settings_key(kind: &str, source: &str) -> String {
    format!(
        "DRASI_{}_{}_SETTINGS",
        if kind == "postgres" { "PG" } else { "HTTP" },
        source.replace('-', "_").to_ascii_uppercase()
    )
}
fn settings_reference(kind: &str, source: &str) -> Result<ConfigurationValue> {
    Ok(ConfigurationValue::Reference {
        resource: resource("postgres-configuration")?,
        key: format!("env-json:{}", settings_key(kind, source)).into(),
        secret: true,
    })
}

fn load(directory: &Path, package: &str) -> Result<Arc<NativePlugin>> {
    let file = directory.join(format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_SUFFIX,
    ));
    drasi_host_sdk::computation::load(&file).with_context(|| format!("load {}", file.display()))
}

fn declare(
    builder: ComponentBatchBuilder,
    name: &str,
    role: ResourceRole,
    recipe: Value,
) -> Result<ComponentBatchBuilder> {
    let id = resource(name)?;
    Ok(builder
        .declare_resource(ResourceSpecification {
            id: id.clone(),
            role,
            ownership: if role == ResourceRole::SecretStore {
                ResourceOwnership::Borrowed
            } else {
                ResourceOwnership::Graph
            },
            binding: name.into(),
        })?
        .resource_configuration(id, recipe)?)
}

fn generate(libraries: &Path, recipe: &Path) -> Result<Value> {
    let raw: Value = serde_yaml::from_slice(&std::fs::read(recipe)?)?;
    let mut config: DrasiServerConfig = serde_json::from_value(raw.clone())?;
    config.validate()?;
    anyhow::ensure!(
        config.instances.len() == 1 && config.instances[0].computation.is_none(),
        "an application recipe must describe one instance, not an existing canonical graph"
    );
    let sources = raw["instances"][0]["sources"]
        .as_array()
        .context("recipe sources")?;
    anyhow::ensure!(
        !sources.is_empty()
            && sources
                .iter()
                .all(|source| matches!(source["kind"].as_str(), Some("postgres" | "http"))),
        "this recipe supports PostgreSQL and native HTTP sources"
    );
    let mut query_only = config.clone();
    query_only.instances[0].sources.clear();
    query_only.instances[0].reactions.clear();
    let instance = query_only
        .resolved_instances(&DtoMapper::new())?
        .pop()
        .context("query instance")?;
    let source_plugin = load(libraries, "drasi-computation-source-postgres")?;
    let bootstrap_plugin = load(libraries, "drasi-computation-bootstrap-postgres")?;
    let source_factory: Arc<NativeFactory> = source_plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == SOURCE)
        .cloned()
        .context("shared PostgreSQL source factory")?;
    let bootstrap = bootstrap_plugin
        .bootstrap_factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == BOOTSTRAP)
        .context("shared PostgreSQL bootstrap factory")?
        .metadata();
    let http_source_plugin = sources
        .iter()
        .any(|source| source["kind"] == "http")
        .then(|| load(libraries, "drasi-computation-source-http"))
        .transpose()?;
    let http_bootstrap_plugin = http_source_plugin
        .as_ref()
        .map(|_| load(libraries, "drasi-computation-bootstrap-http"))
        .transpose()?;
    let http_source_factory = http_source_plugin
        .as_ref()
        .map(|plugin| {
            plugin
                .factories()
                .iter()
                .find(|factory| factory.metadata().implementation.name.as_ref() == HTTP_SOURCE)
                .cloned()
                .context("native HTTP source factory")
        })
        .transpose()?;
    let http_bootstrap = http_bootstrap_plugin
        .as_ref()
        .map(|plugin| {
            plugin
                .bootstrap_factories()
                .iter()
                .find(|factory| factory.metadata().implementation.name.as_ref() == HTTP_BOOTSTRAP)
                .map(|factory| factory.metadata())
                .context("native HTTP bootstrap factory")
        })
        .transpose()?;
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    let mut builder = ComponentBatch::builder();
    builder = declare(
        builder,
        "postgres-configuration",
        ResourceRole::SecretStore,
        json!({"kind":"configuration"}),
    )?;
    builder = declare(
        builder,
        "query-indexes",
        ResourceRole::IndexBackend,
        json!({"kind":"memoryIndexes"}),
    )?;
    builder = declare(
        builder,
        "query-results",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    )?;
    anyhow::ensure!(!config.instances[0].persist_index,
        "persistent examples require explicit scoped index/state recipes; memory defaults cannot replace retained state");

    for query in &instance.queries {
        anyhow::ensure!(
            !query.sources.is_empty()
                && query.middleware.is_empty()
                && query
                    .sources
                    .iter()
                    .all(|source| source.pipeline.is_empty())
                && query.enable_bootstrap,
            "query {} requires an explicitly composed bootstrap/middleware contract",
            query.id
        );
        let progress = format!("{}-progress", query.id);
        builder = declare(
            builder,
            &progress,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":query.id}),
        )?;
        let mut providers = Vec::new();
        for input in &query.sources {
            let source = input.source_id.as_str();
            let declaration = sources
                .iter()
                .find(|candidate| candidate["id"] == source)
                .with_context(|| format!("query {} has an unknown source {source}", query.id))?;
            let kind = declaration["kind"].as_str().context("source kind")?;
            let metadata = if kind == "postgres" {
                &bootstrap
            } else {
                anyhow::ensure!(
                    declaration["bootstrapProvider"]["kind"] == "http",
                    "HTTP source {source} requires its real native HTTP initialization endpoint"
                );
                http_bootstrap.as_ref().context("HTTP bootstrap metadata")?
            };
            let bootstrap_id = if query.sources.len() == 1 && kind == "postgres" {
                format!("{}-postgres-bootstrap", query.id)
            } else {
                format!("{}-{source}-bootstrap", query.id)
            };
            let bootstrap_config = NativeBootstrapConfig {
                component: id(&query.id)?,
                implementation: metadata.implementation.clone(),
                configuration_version: metadata.configuration_version,
                configuration: BTreeMap::from([
                    ("source".into(), literal(source)?),
                    ("stream".into(), literal(stream(source)?)?),
                    ("settings".into(), settings_reference(kind, source)?),
                ]),
                source_progress: (kind == "postgres")
                    .then(|| resource(&progress))
                    .transpose()?,
                source_bootstrap: None,
            };
            let mut bootstrap_recipe = serde_json::to_value(bootstrap_config)?;
            bootstrap_recipe["kind"] = json!("nativeBootstrap");
            builder = declare(
                builder,
                &bootstrap_id,
                ResourceRole::Bootstrap,
                bootstrap_recipe,
            )?
            .resource_dependency(
                resource(&bootstrap_id)?,
                resource("postgres-configuration")?,
                ResourceRole::SecretStore,
            )?;
            if kind == "postgres" {
                builder = builder.resource_dependency(
                    resource(&bootstrap_id)?,
                    resource(&progress)?,
                    ResourceRole::Checkpoint,
                )?;
            }
            providers.push(bootstrap_id);
        }
        let bootstrap_id = if providers.len() == 1 {
            providers.remove(0)
        } else {
            let name = format!("{}-bootstrap", query.id);
            builder = declare(
                builder,
                &name,
                ResourceRole::Bootstrap,
                json!({"kind":"compositeBootstrap","component":query.id,"providers":providers}),
            )?;
            for provider in providers {
                builder = builder.resource_dependency(
                    resource(&name)?,
                    resource(&provider)?,
                    ResourceRole::Bootstrap,
                )?;
            }
            name
        };
        let mut specification = query_factory.specification_from_config(id(&query.id)?, json!({
            "query":query.query, "stream":stream(&query.id)?,
            "language":match query.query_language { drasi_lib::QueryLanguage::Cypher => "cypher", drasi_lib::QueryLanguage::GQL => "gql" },
            "outbox_capacity":query.outbox_capacity,
            "execution":QueryExecutionSettings::from_legacy_config(query),
        }))?;
        specification.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("query-indexes")?]),
            ("catalog".into(), vec![resource("query-results")?]),
            ("source_progress".into(), vec![resource(&progress)?]),
            ("bootstrap".into(), vec![resource(&bootstrap_id)?]),
        ]);
        builder = builder
            .component(specification, query_factory.clone())
            .bind_stream(endpoint(&query.id, "out")?, stream(&query.id)?);
        for source in &query.sources {
            builder = builder.connect(
                EdgeDefinition::new(
                    endpoint(&source.source_id, "out")?,
                    endpoint(&query.id, "in")?,
                ),
                Box::new(BoundedPipeConfig { capacity: 32 }),
            );
        }
    }

    for source in sources {
        let source_id = source["id"].as_str().context("source ID")?;
        if source["kind"] == "http" {
            let factory = http_source_factory
                .as_ref()
                .context("HTTP source factory")?;
            let mut specification = factory.specification(
                id(source_id)?,
                json!({
                    "sourceId":source_id, "stream":stream(source_id)?,
                    "host":source.get("host").cloned().unwrap_or(json!("0.0.0.0")),
                    "port":source.get("port").cloned().unwrap_or(json!(9100)),
                    "timeoutMs":source.get("timeoutMs").cloned().unwrap_or(json!(10000)),
                }),
            )?;
            let prefix = settings_key("http", source_id)
                .trim_end_matches("_SETTINGS")
                .to_owned();
            for (field, key) in [
                ("host", format!("env:{prefix}_HOST")),
                ("port", format!("env-json:{prefix}_PORT")),
            ] {
                specification.configuration.insert(
                    field.into(),
                    ConfigurationValue::Reference {
                        resource: resource("postgres-configuration")?,
                        key: key.into(),
                        secret: false,
                    },
                );
            }
            builder = builder
                .component(specification, factory.clone())
                .bind_stream(endpoint(source_id, "out")?, stream(source_id)?);
            continue;
        }
        let consumers: BTreeMap<_, _> = instance
            .queries
            .iter()
            .filter(|query| {
                query
                    .sources
                    .iter()
                    .any(|input| input.source_id == source_id)
            })
            .map(|query| (query.id.clone(), format!("{}-progress", query.id)))
            .collect();
        anyhow::ensure!(
            !consumers.is_empty(),
            "source {source_id} has no actual query owners"
        );
        let feedback = format!("{source_id}-consumers");
        builder = declare(
            builder,
            &feedback,
            ResourceRole::Checkpoint,
            json!({
                "kind":"consumerProgress", "producer":source_id, "consumers":consumers,
            }),
        )?;
        for progress in consumers.values() {
            builder = builder.resource_dependency(
                resource(&feedback)?,
                resource(progress)?,
                ResourceRole::Checkpoint,
            )?;
        }
        let mut specification = source_factory.specification(
            id(source_id)?,
            json!({
                "stream":stream(source_id)?, "settings":{},
                "persistent":false, "auto_create_slot":false, "start_from_slot":false,
            }),
        )?;
        specification.configuration.insert(
            "settings".into(),
            settings_reference("postgres", source_id)?,
        );
        specification
            .dependencies
            .insert("consumer_progress".into(), vec![resource(&feedback)?]);
        builder = builder
            .component(specification, source_factory.clone())
            .bind_stream(endpoint(source_id, "out")?, stream(source_id)?);
    }
    let mut outlet =
        outlet_factory.specification_from_config(id("postgres-query-results")?, json!({}))?;
    outlet
        .dependencies
        .insert("catalog".into(), vec![resource("query-results")?]);
    builder = builder.component(outlet, outlet_factory);
    for query in &instance.queries {
        builder = builder.connect(
            EdgeDefinition::new(
                endpoint(&query.id, "out")?,
                endpoint("postgres-query-results", "in")?,
            ),
            Box::new(BoundedPipeConfig { capacity: 32 }),
        );
    }
    let graph = builder.build()?;
    config.instances[0].sources.clear();
    config.instances[0].queries.clear();
    config.instances[0].computation = Some(
        ComputationConfig {
            definition: graph.definition,
        }
        .into(),
    );
    config.auto_install_plugins = false;
    config.plugins.clear();
    config.validate()?;
    serde_json::to_value(config).map_err(Into::into)
}

fn resolve(value: &Value) -> Result<Value> {
    if let Some(text) = value.as_str() {
        if let Some(expression) = text
            .strip_prefix("${")
            .and_then(|value| value.strip_suffix('}'))
        {
            let (name, fallback) = expression
                .split_once(":-")
                .map_or((expression, None), |(name, fallback)| {
                    (name, Some(fallback))
                });
            return match std::env::var(name) {
                Ok(value) => Ok(json!(value)),
                Err(std::env::VarError::NotPresent) => fallback
                    .map(|value| json!(value))
                    .with_context(|| format!("environment variable {name} is required")),
                Err(error) => {
                    Err(error).with_context(|| format!("read environment variable {name}"))
                }
            };
        }
    }
    if value["kind"] == "EnvironmentVariable" {
        let name = value["name"]
            .as_str()
            .context("environment variable name")?;
        return match std::env::var(name) {
            Ok(value) => Ok(json!(value)),
            Err(std::env::VarError::NotPresent) => value
                .get("default")
                .cloned()
                .context("environment variable has no default"),
            Err(error) => Err(error).with_context(|| format!("read environment variable {name}")),
        };
    }
    if let Some(values) = value.as_array() {
        return values
            .iter()
            .map(resolve)
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
    }
    if let Some(values) = value.as_object() {
        return values
            .iter()
            .map(|(key, value)| Ok((key.clone(), resolve(value)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(Value::Object);
    }
    Ok(value.clone())
}

fn settings(recipe: &Path, source_id: &str) -> Result<Value> {
    let value: Value = serde_yaml::from_slice(&std::fs::read(recipe)?)?;
    let source = value["instances"][0]["sources"]
        .as_array()
        .context("recipe sources")?
        .iter()
        .find(|source| source["id"] == source_id)
        .context("source")?;
    if source["kind"] == "http" {
        let mut settings = source["bootstrapProvider"].clone();
        anyhow::ensure!(
            settings["kind"] == "http",
            "HTTP bootstrap endpoint is required"
        );
        settings
            .as_object_mut()
            .context("HTTP bootstrap settings")?
            .remove("kind");
        return resolve(&settings);
    }
    anyhow::ensure!(source["kind"] == "postgres", "unsupported source kind");
    let get = |name: &str, default: Value| resolve(source.get(name).unwrap_or(&default));
    let required = |name: &str| {
        resolve(
            source
                .get(name)
                .with_context(|| format!("PostgreSQL {name} is required"))?,
        )
    };
    let port = get("port", json!(5432))?;
    let port: u16 = match port.as_u64() {
        Some(port) => port.try_into().context("PostgreSQL port exceeds 65535")?,
        None => port
            .as_str()
            .context("PostgreSQL port must be an integer")?
            .parse()
            .context("invalid PostgreSQL port")?,
    };
    anyhow::ensure!(port > 0, "PostgreSQL port must be nonzero");
    Ok(json!({
        "connection":{
            "host":get("host",json!("localhost"))?, "port":port,
            "database":required("database")?, "user":required("user")?,
            "password":required("password")?, "ssl_mode":get("sslMode",json!("prefer"))?,
            "tables":source.get("tables").cloned().unwrap_or(json!([])),
            "slot_name":required("slotName")?, "publication_name":required("publicationName")?,
            "table_keys":source.get("tableKeys").and_then(Value::as_array).map(|keys| keys.iter().map(|key| json!({
                "table":key["table"], "key_columns":key["keyColumns"],
            })).collect::<Vec<_>>()).unwrap_or_default(),
        },
        "start_lsn":"0/0",
        "transactions":{"max_changes":4096,"max_bytes":8388608,"max_duration_ms":30000},
        "max_protocol_bytes":8388608,"io_timeout_ms":10000,"feedback_interval_ms":1000,
    }))
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().context(
        "usage: native-postgres generate <plugins> <recipe> <output> | settings <recipe> <source>",
    )?;
    match mode.to_str().context("mode is not UTF-8")? {
        "generate" => {
            let libraries = PathBuf::from(args.next().context("native plugin directory")?);
            let recipe = PathBuf::from(args.next().context("application recipe")?);
            let output = PathBuf::from(args.next().context("output configuration")?);
            anyhow::ensure!(args.next().is_none(), "unexpected arguments");
            let config = generate(&libraries, &recipe)?;
            let header = "# Generated by examples/native-postgres.rs from the matching examples/postgres/recipes input.\n# Canonical definition preserves real factory metadata and secret configuration references.\n";
            std::fs::write(
                output,
                format!("{header}{}", serde_yaml::to_string(&config)?),
            )?;
        }
        "settings" => {
            let recipe = PathBuf::from(args.next().context("application recipe")?);
            let source = args.next().context("PostgreSQL source ID")?;
            anyhow::ensure!(args.next().is_none(), "unexpected arguments");
            println!(
                "{}",
                settings(&recipe, source.to_str().context("source ID is not UTF-8")?)?
            );
        }
        "inspect" => {
            let file = PathBuf::from(args.next().context("configuration path")?);
            anyhow::ensure!(args.next().is_none(), "unexpected arguments");
            let value: Value = serde_yaml::from_slice(&std::fs::read(file)?)?;
            println!("{}", serde_json::to_string(&value)?);
        }
        _ => anyhow::bail!("unknown generation mode"),
    }
    Ok(())
}
