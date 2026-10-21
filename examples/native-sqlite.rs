// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate a canonical SQLite source/query Server configuration.
//! Usage: native-sqlite <plugin-directory> <database> <source-rest-port> [file|single|shared] [index-directory]
//! File mode requires quiescent writers during bootstrap; paired modes own that exclusion.

use anyhow::{Context, Result};
use drasi_host_sdk::{
    computation::NativePlugin,
    management::{NativeBootstrapConfig, NativeSourceBootstrapConfig},
};
use drasi_lib::computation::v1::*;
use drasi_server::{api::models::ConfigValue, computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const INSTANCE: &str = "native-sqlite";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    File,
    Single,
    Shared,
}

impl Mode {
    pub fn queries(self) -> &'static [&'static str] {
        match self {
            Self::Single => &["first"],
            Self::File | Self::Shared => &["first", "second"],
        }
    }
}

pub fn load(directory: &Path, package: &str) -> Result<Arc<NativePlugin>> {
    let path = directory.join(format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_SUFFIX,
    ));
    drasi_host_sdk::computation::load(&path).with_context(|| format!("load {}", path.display()))
}
fn id(value: &str) -> Result<ComponentId> {
    Ok(ComponentId::try_new(value)?)
}
fn resource(value: &str) -> Result<ResourceId> {
    Ok(ResourceId::try_new(value)?)
}
fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(id(component)?, PortId::try_new(port)?))
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
            ownership: ResourceOwnership::Graph,
            binding: name.into(),
        })?
        .resource_configuration(id, recipe)?)
}

pub fn definition(libraries: &Path, database: &Path, rest_port: u16) -> Result<ComputationConfig> {
    definition_with_mode(libraries, database, rest_port, Mode::File, None)
}

pub fn definition_with_mode(
    libraries: &Path,
    database: &Path,
    rest_port: u16,
    mode: Mode,
    indexes: Option<&Path>,
) -> Result<ComputationConfig> {
    anyhow::ensure!(
        database.is_file(),
        "initialize the SQLite database before generating its bootstrap configuration"
    );
    let paired = mode != Mode::File;
    anyhow::ensure!(paired == indexes.is_some(),
        "paired modes require an explicit persistent index directory; file mode uses memory indexes");
    let (source_name, bootstrap_name) = match mode {
        Mode::File => ("drasi.sqlite/changes", "drasi.sqlite/file-bootstrap"),
        Mode::Single => (
            "drasi.sqlite/replay-transactions",
            "drasi.sqlite/coordinated-bootstrap",
        ),
        Mode::Shared => (
            "drasi.sqlite/shared-transactions",
            "drasi.sqlite/shared-bootstrap",
        ),
    };
    let source = load(libraries, "drasi-computation-source-sqlite")?;
    let bootstrap = load(libraries, "drasi-computation-bootstrap-sqlite")?;
    let source = source
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == source_name)
        .cloned()
        .context("native SQLite source factory")?;
    let bootstrap = bootstrap
        .bootstrap_factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == bootstrap_name)
        .context("native SQLite bootstrap factory")?
        .metadata()
        .clone();
    let transactions = json!({"max_changes":64,"max_bytes":1048576,"max_duration_ms":5000});
    let mut source_configuration = json!({
        "stream":"source/out",
        "settings":{
            "path":database,"tables":["items"],"table_keys":[],
            "output":if paired {"transactions"} else {"changes"},
            "replay":if paired {json!({"max_transactions":16,"max_bytes":8388608})} else {Value::Null},
            "transactions":transactions,
            "max_sql_bytes":65536,"command_capacity":8,"output_capacity":4,"shutdown_timeout_ms":5000
        },
        "rest_api":{"host":"127.0.0.1","port":rest_port}
    });
    if paired {
        source_configuration["coordinated_snapshot"] = json!(true);
    }
    let mut source_spec = source.specification(id("source")?, source_configuration)?;
    let mut builder = ComponentBatch::builder();
    builder = declare(
        builder,
        "indexes",
        ResourceRole::IndexBackend,
        match indexes {
            Some(path) => json!({"kind":"rocksdbIndexes","path":path}),
            None => json!({"kind":"memoryIndexes"}),
        },
    )?;
    if paired {
        let mut consumers = BTreeMap::new();
        for name in mode.queries() {
            let progress = format!("{name}-progress");
            builder = declare(
                builder,
                &progress,
                ResourceRole::Checkpoint,
                json!({"kind":"sourceProgress","component":name}),
            )?;
            consumers.insert(*name, progress);
        }
        let progress = if mode == Mode::Shared {
            builder = declare(
                builder,
                "consumers",
                ResourceRole::Checkpoint,
                json!({"kind":"consumerProgress","producer":"source","consumers":consumers}),
            )?;
            for progress in consumers.values() {
                builder = builder.resource_dependency(
                    resource("consumers")?,
                    resource(progress)?,
                    ResourceRole::Checkpoint,
                )?;
            }
            "consumers"
        } else {
            "first-progress"
        };
        let coordinator = NativeSourceBootstrapConfig {
            component: id("source")?,
            implementation: source_spec.implementation.clone(),
            configuration_version: source_spec.configuration_version,
            configuration: source_spec.configuration.clone(),
            source_progress: (mode == Mode::Single)
                .then(|| resource(progress))
                .transpose()?,
            consumer_progress: (mode == Mode::Shared)
                .then(|| resource(progress))
                .transpose()?,
        };
        let mut recipe = serde_json::to_value(coordinator)?;
        recipe["kind"] = json!("nativeSourceBootstrap");
        builder = declare(builder, "source-bootstrap", ResourceRole::Bootstrap, recipe)?
            .resource_dependency(
                resource("source-bootstrap")?,
                resource(progress)?,
                ResourceRole::Checkpoint,
            )?;
        source_spec.dependencies = BTreeMap::from([
            (
                "source_bootstrap".into(),
                vec![resource("source-bootstrap")?],
            ),
            (
                if mode == Mode::Shared {
                    "consumer_progress".into()
                } else {
                    "source_progress".into()
                },
                vec![resource(progress)?],
            ),
        ]);
    }
    builder = builder
        .component(source_spec, source)
        .bind_stream(endpoint("source", "out")?, StreamId::try_new("source/out")?);
    builder = declare(
        builder,
        "catalog",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    )?;
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    for name in mode.queries() {
        let bootstrap_id = format!("{name}-snapshot");
        let mut configuration = BTreeMap::from([
            (
                "source".into(),
                ConfigurationValue::Literal(json!("source")),
            ),
            (
                "stream".into(),
                ConfigurationValue::Literal(json!("source/out")),
            ),
        ]);
        if !paired {
            configuration.insert("settings".into(), ConfigurationValue::Literal(json!({
                "path":database,"tables":["items"],"table_keys":[],"max_tables":8,"max_bytes":1048576,
                "max_duration_ms":5000,"output_capacity":2,"shutdown_timeout_ms":5000
            })));
        }
        let snapshot = NativeBootstrapConfig {
            component: id(name)?,
            implementation: bootstrap.implementation.clone(),
            configuration_version: bootstrap.configuration_version,
            configuration,
            source_progress: paired
                .then(|| resource(&format!("{name}-progress")))
                .transpose()?,
            source_bootstrap: paired.then(|| resource("source-bootstrap")).transpose()?,
        };
        let mut recipe = serde_json::to_value(snapshot)?;
        recipe["kind"] = json!("nativeBootstrap");
        builder = declare(builder, &bootstrap_id, ResourceRole::Bootstrap, recipe)?;
        if paired {
            builder = builder
                .resource_dependency(
                    resource(&bootstrap_id)?,
                    resource("source-bootstrap")?,
                    ResourceRole::Bootstrap,
                )?
                .resource_dependency(
                    resource(&bootstrap_id)?,
                    resource(&format!("{name}-progress"))?,
                    ResourceRole::Checkpoint,
                )?;
        }
        let mut query_config = json!({
            "query":"MATCH (n:items) RETURN n.id AS id,n.value AS value",
            "stream":format!("{name}/out"),"outbox_capacity":32,
        });
        if paired {
            query_config["execution"] = serde_json::to_value(QueryExecutionSettings {
                source_transactions: Some(serde_json::from_value(transactions.clone())?),
                ..Default::default()
            })?;
        }
        let mut query = query_factory.specification_from_config(id(name)?, query_config)?;
        query.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("indexes")?]),
            ("catalog".into(), vec![resource("catalog")?]),
            ("bootstrap".into(), vec![resource(&bootstrap_id)?]),
        ]);
        if paired {
            query.dependencies.insert(
                "source_progress".into(),
                vec![resource(&format!("{name}-progress"))?],
            );
        }
        let outlet_id = format!("{name}-results");
        let outlet = ComponentSpecification {
            descriptor: QueryResultsOutlet::new(
                id(&outlet_id)?,
                QueryResultsCatalog::new(ComponentBatch::GRAPH_ID)?,
            )
            .descriptor()
            .clone(),
            role: ComponentRole::Sink,
            completion: Some(SinkCompletion::Handled),
            implementation: outlet_factory.descriptor().implementation.clone(),
            configuration_version: 1,
            configuration: BTreeMap::new(),
            dependencies: BTreeMap::from([("catalog".into(), vec![resource("catalog")?])]),
        };
        builder = builder
            .component(query, query_factory.clone())
            .component(outlet, outlet_factory.clone())
            .bind_stream(
                endpoint(name, "out")?,
                StreamId::try_new(format!("{name}/out"))?,
            )
            .connect(
                EdgeDefinition::new(endpoint("source", "out")?, endpoint(name, "in")?),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            )
            .connect(
                EdgeDefinition::new(endpoint(name, "out")?, endpoint(&outlet_id, "in")?),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            );
    }
    Ok(ComputationConfig {
        definition: builder.build()?.definition,
    })
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let libraries = PathBuf::from(args.next().context(
        "usage: native-sqlite <plugin-directory> <database> <source-rest-port> [file|single|shared] [index-directory]",
    )?);
    let database = PathBuf::from(args.next().context("missing SQLite path")?);
    let port: u16 = args
        .next()
        .context("missing source REST port")?
        .to_str()
        .context("invalid REST port")?
        .parse()?;
    let mode = match args
        .next()
        .as_deref()
        .map(|value| value.to_str().context("invalid mode"))
        .transpose()?
    {
        None | Some("file") => Mode::File,
        Some("single") => Mode::Single,
        Some("shared") => Mode::Shared,
        Some(mode) => anyhow::bail!("unknown SQLite mode {mode}"),
    };
    let indexes = args.next().map(PathBuf::from);
    anyhow::ensure!(args.next().is_none(), "unexpected argument");
    let config = DrasiServerConfig {
        host: ConfigValue::Static("127.0.0.1".into()),
        verify_plugins: false,
        enable_ui: false,
        persist_config: false,
        instances: vec![drasi_server::config::DrasiLibInstanceConfig {
            id: ConfigValue::Static(INSTANCE.into()).into(),
            computation: Some(
                definition_with_mode(&libraries, &database, port, mode, indexes.as_deref())?.into(),
            ),
            ..Default::default()
        }],
        ..Default::default()
    };
    config.validate()?;
    println!("{}", serde_yaml::to_string(&config)?);
    Ok(())
}
