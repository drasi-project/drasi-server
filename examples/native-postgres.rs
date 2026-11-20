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
use drasi_host_sdk::computation::{NativeBootstrapNodeFactory, NativeFactory, NativePlugin};
use drasi_lib::computation::v1::*;
use drasi_lib::config::ComponentReference;
use drasi_server::{api::mappings::DtoMapper, computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};

const SOURCE: &str = "drasi.postgres/shared-changes";
const BOOTSTRAP: &str = "drasi.postgres/shared-row-bootstrap";
const HTTP_SOURCE: &str = "drasi.network/http-source";
const HTTP_BOOTSTRAP: &str = "drasi.network/http-bootstrap";

fn resource(value: &str) -> Result<ResourceId> {
    Ok(ResourceId::try_new(value)?)
}
fn endpoint(component: &ComponentId, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(component.clone(), PortId::try_new(port)?))
}
fn stream(source: &str) -> Result<StreamId> {
    Ok(StreamId::try_new(format!("{source}/out"))?)
}
fn variable(kind: &str, source: &str, suffix: &str) -> String {
    format!(
        "DRASI_{}_{}_{suffix}",
        if kind == "postgres" { "PG" } else { "HTTP" },
        source.replace('-', "_").to_ascii_uppercase()
    )
}
fn settings_key(kind: &str, source: &str) -> String {
    variable(kind, source, "SETTINGS")
}
fn reference(key: String, secret: bool) -> Result<ConfigurationValue> {
    Ok(ConfigurationValue::Reference {
        resource: resource("postgres-configuration")?,
        key: key.into(),
        secret,
    })
}
fn settings_reference(kind: &str, source: &str) -> Result<ConfigurationValue> {
    reference(format!("env-json:{}", settings_key(kind, source)), true)
}

/// PostgreSQL connection fields supplied per deployment, each through its own
/// environment variable: (field, variable suffix, JSON-typed, secret).
const POSTGRES_FIELDS: &[(&str, &str, bool, bool)] = &[
    ("host", "HOST", false, false),
    ("port", "PORT", true, false),
    ("database", "DATABASE", false, false),
    ("user", "USER", false, false),
    ("password", "PASSWORD", false, true),
    ("sslMode", "SSL_MODE", false, false),
    ("publication", "PUBLICATION", false, false),
    ("slotName", "SLOT_NAME", false, false),
];

/// Flat PostgreSQL settings shared by the source and its bootstrap provider:
/// recipe literals plus valid placeholders for the per-deployment fields, which
/// `postgres_references` replaces with environment references.
fn postgres_literals(source: &Value) -> Value {
    json!({
        "host":"localhost", "port":5432, "database":"placeholder", "user":"placeholder",
        "password":"placeholder", "sslMode":"prefer", "publication":"placeholder",
        "slotOwnership":"external", "slotName":"placeholder",
        "tables":source.get("tables").cloned().unwrap_or(json!([])),
        "tableKeys":source.get("tableKeys").cloned().unwrap_or(json!([])),
        "maxTransactionChanges":4096, "maxTransactionBytes":8388608,
        "maxTransactionDurationMs":30000, "maxProtocolBytes":8388608,
        "ioTimeoutMs":10000, "feedbackIntervalMs":1000,
    })
}
fn postgres_references(source: &str) -> Result<Vec<(Arc<str>, ConfigurationValue)>> {
    POSTGRES_FIELDS
        .iter()
        .map(|(field, suffix, typed, secret)| {
            let name = variable("postgres", source, suffix);
            let key = if *typed {
                format!("env-json:{name}")
            } else {
                format!("env:{name}")
            };
            Ok(((*field).into(), reference(key, *secret)?))
        })
        .collect()
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

fn bootstrap_edge(
    builder: ComponentBatchBuilder,
    from: Endpoint,
    to: Endpoint,
) -> ComponentBatchBuilder {
    builder.connect(
        EdgeDefinition::new(from, to),
        Box::new(BoundedPipeConfig { capacity: 8 }),
    )
}

/// A source's visible bootstrap provider node. The source relays its
/// consumers' bootstrap sessions to it; a provider that reads progress lists
/// each bootstrapping query's progress owner.
fn provider_node(
    factory: &NativeBootstrapNodeFactory,
    id: ComponentId,
    configuration: BTreeMap<Arc<str>, ConfigurationValue>,
    progress: &[ResourceId],
) -> Result<ComponentSpecification> {
    Ok(ComponentSpecification {
        descriptor: NativeBootstrapNodeFactory::component_descriptor(id)?,
        role: ComponentRole::BootstrapProvider,
        completion: None,
        implementation: factory.descriptor().implementation.clone(),
        configuration_version: factory.descriptor().configuration_version,
        configuration,
        dependencies: if factory
            .descriptor()
            .dependencies
            .contains_key("source_progress")
        {
            BTreeMap::from([("source_progress".into(), progress.to_vec())])
        } else {
            BTreeMap::new()
        },
    })
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
    let source_ids: BTreeMap<String, ComponentId> = sources
        .iter()
        .map(|source| {
            Ok((
                source["id"].as_str().context("source name")?.to_owned(),
                ComponentId::generate(),
            ))
        })
        .collect::<Result<_>>()?;
    let query_ids: BTreeMap<ComponentName, ComponentId> = instance
        .queries
        .iter()
        .map(|query| {
            (
                query.name.clone(),
                query.id.clone().unwrap_or_else(ComponentId::generate),
            )
        })
        .collect();
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
        .cloned()
        .context("shared PostgreSQL bootstrap factory")?;
    let bootstrap = Arc::new(NativeBootstrapNodeFactory::new(bootstrap));
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
                .cloned()
                .map(|factory| Arc::new(NativeBootstrapNodeFactory::new(factory)))
                .context("native HTTP bootstrap factory")
        })
        .transpose()?;
    let query_plugin = load(libraries, "drasi-computation-transformer-query")?;
    let query_factory = query_plugin.query_factory()?;
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
        let query_id = query_ids
            .get(&query.name)
            .context("assigned query identity")?;
        anyhow::ensure!(
            !query.inputs.is_empty()
                && query.middleware.is_empty()
                && query.inputs.iter().all(|source| source.pipeline.is_empty())
                && query.enable_bootstrap,
            "query {} requires an explicitly composed bootstrap/middleware contract",
            query.name
        );
        let progress = format!("{}-progress", query.name);
        builder = declare(
            builder,
            &progress,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":query_id,"indexes":"query-indexes"}),
        )?;
        builder = builder.resource_dependency(
            resource(&progress)?,
            resource("query-indexes")?,
            ResourceRole::IndexBackend,
        )?;
        for input in &query.inputs {
            let source = input
                .component
                .as_name()
                .context("native PostgreSQL recipes require source names")?
                .as_str();
            let declaration = sources
                .iter()
                .find(|candidate| candidate["id"] == source)
                .with_context(|| format!("query {} has an unknown source {source}", query.name))?;
            anyhow::ensure!(
                declaration["kind"] == "postgres"
                    || declaration["bootstrapProvider"]["kind"] == "http",
                "HTTP source {source} requires its real native HTTP initialization endpoint"
            );
        }
        let execution_inputs = query
            .inputs
            .iter()
            .map(|input| {
                let source_name = input
                    .component
                    .as_name()
                    .context("native PostgreSQL recipes require source names")?;
                let source_id = source_ids
                    .get(source_name.as_str())
                    .context("assigned source identity")?;
                let mut resolved = input.clone();
                resolved.component = ComponentReference::by_id(source_id.clone());
                resolved.to_execution_input()
            })
            .collect::<Result<Vec<_>>>()?;
        let execution = QueryExecutionSettings {
            joins: query.joins.clone().unwrap_or_default(),
            middleware: query.middleware.clone(),
            inputs: execution_inputs,
            result: query.result.clone(),
            source_transactions: None,
        };
        let mut specification = query_factory.specification_from_config(query_id.clone(), json!({
            "query":query.query, "stream":stream(query.name.as_str())?,
            "language":match query.query_language { drasi_lib::QueryLanguage::Cypher => "cypher", drasi_lib::QueryLanguage::GQL => "gql" },
            "outbox_capacity":query.outbox_capacity,
            "execution":execution,
        }))?;
        specification.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("query-indexes")?]),
            ("catalog".into(), vec![resource("query-results")?]),
            ("source_progress".into(), vec![resource(&progress)?]),
        ]);
        builder = builder
            .component(query.name.clone(), specification, query_factory.clone())
            .bind_stream(endpoint(query_id, "out")?, stream(query.name.as_str())?);
        for source in &query.inputs {
            let source_name = source
                .component
                .as_name()
                .context("native PostgreSQL recipes require source names")?;
            let source_id = source_ids
                .get(source_name.as_str())
                .context("assigned source identity")?;
            builder = builder.connect(
                EdgeDefinition::new(endpoint(source_id, "out")?, endpoint(query_id, "in")?),
                Box::new(BoundedPipeConfig { capacity: 32 }),
            );
            // The query bootstraps through its input, which relays to the
            // source's provider node.
            builder = bootstrap_edge(
                builder,
                endpoint(source_id, "snapshot")?,
                endpoint(query_id, "bootstrap")?,
            );
        }
    }

    for source in sources {
        let source_id = source["id"].as_str().context("source ID")?;
        let component_id = source_ids
            .get(source_id)
            .context("assigned source identity")?;
        let consumers: BTreeMap<_, _> = instance
            .queries
            .iter()
            .filter(|query| {
                query.inputs.iter().any(|input| {
                    input
                        .component
                        .as_name()
                        .is_some_and(|name| name.as_str() == source_id)
                })
            })
            .map(|query| {
                (
                    query_ids[&query.name].clone(),
                    format!("{}-progress", query.name),
                )
            })
            .collect();
        let kind = source["kind"].as_str().context("source kind")?;
        let provider_id = ComponentId::generate();
        let provider_factory = if kind == "postgres" {
            bootstrap.clone()
        } else {
            http_bootstrap.clone().context("HTTP bootstrap factory")?
        };
        // The served source and stream come from the bootstrap edge, and the
        // paired PostgreSQL provider takes everything else from its source.
        let mut provider_configuration = BTreeMap::new();
        if kind != "postgres" {
            provider_configuration.insert("settings".into(), settings_reference(kind, source_id)?);
        }
        let provider = provider_node(
            &provider_factory,
            provider_id.clone(),
            provider_configuration,
            &consumers
                .values()
                .map(|progress| resource(progress))
                .collect::<Result<Vec<_>>>()?,
        )?;
        builder = builder.component(
            ComponentName::try_new(format!("{source_id}-bootstrap"))?,
            provider,
            provider_factory,
        );
        builder = bootstrap_edge(
            builder,
            endpoint(&provider_id, "snapshot")?,
            endpoint(component_id, "bootstrap")?,
        );
        if source["kind"] == "http" {
            let factory = http_source_factory
                .as_ref()
                .context("HTTP source factory")?;
            let mut specification = factory.specification(
                component_id.clone(),
                json!({
                    "stream":stream(source_id)?,
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
                .component(
                    ComponentName::try_new(source_id)?,
                    specification,
                    factory.clone(),
                )
                .bind_stream(endpoint(component_id, "out")?, stream(source_id)?);
            continue;
        }
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
                "kind":"consumerProgress", "producer":component_id, "consumers":consumers,
            }),
        )?;
        for progress in consumers.values() {
            builder = builder.resource_dependency(
                resource(&feedback)?,
                resource(progress)?,
                ResourceRole::Checkpoint,
            )?;
        }
        let mut configuration = postgres_literals(source);
        configuration["stream"] = json!(stream(source_id)?);
        configuration["persistent"] = json!(false);
        let mut specification =
            source_factory.specification(component_id.clone(), configuration)?;
        specification
            .configuration
            .extend(postgres_references(source_id)?);
        specification
            .dependencies
            .insert("consumer_progress".into(), vec![resource(&feedback)?]);
        builder = builder
            .component(
                ComponentName::try_new(source_id)?,
                specification,
                source_factory.clone(),
            )
            .bind_stream(endpoint(component_id, "out")?, stream(source_id)?);
    }
    let outlet_id = ComponentId::from_uuid(uuid::Uuid::from_u128(0xff01));
    let mut outlet = outlet_factory.specification_from_config(outlet_id.clone(), json!({}))?;
    outlet
        .dependencies
        .insert("catalog".into(), vec![resource("query-results")?]);
    builder = builder.component(
        ComponentName::try_new("postgres-query-results")?,
        outlet,
        outlet_factory,
    );
    for query in &instance.queries {
        let query_id = &query_ids[&query.name];
        builder = builder.connect(
            EdgeDefinition::new(endpoint(query_id, "out")?, endpoint(&outlet_id, "in")?),
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

/// The environment variables a generated configuration references for one
/// source, resolved from the application recipe.
fn settings(recipe: &Path, source_id: &str) -> Result<Vec<(String, String)>> {
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
        return Ok(vec![(
            settings_key("http", source_id),
            resolve(&settings)?.to_string(),
        )]);
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
    let text = |value: Value| match value {
        Value::String(value) => value,
        value => value.to_string(),
    };
    let values = [
        ("HOST", text(get("host", json!("localhost"))?)),
        ("PORT", port.to_string()),
        ("DATABASE", text(required("database")?)),
        ("USER", text(required("user")?)),
        ("PASSWORD", text(required("password")?)),
        ("SSL_MODE", text(get("sslMode", json!("prefer"))?)),
        ("PUBLICATION", text(required("publicationName")?)),
        ("SLOT_NAME", text(required("slotName")?)),
    ];
    Ok(values
        .into_iter()
        .map(|(suffix, value)| (variable("postgres", source_id, suffix), value))
        .collect())
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
            for (name, value) in
                settings(&recipe, source.to_str().context("source ID is not UTF-8")?)?
            {
                println!("export {name}='{}'", value.replace('\'', r"'\''"));
            }
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
