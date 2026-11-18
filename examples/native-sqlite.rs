// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate a canonical SQLite source/query Server configuration.
//! Usage: native-sqlite <plugin-directory> <database> <source-rest-port> [file|single|shared] [index-directory]
//! File mode requires quiescent writers during bootstrap; paired modes own that exclusion.

use anyhow::{Context, Result};
use drasi_host_sdk::computation::{NativeBootstrapNodeFactory, NativePlugin};
use drasi_lib::computation::v1::*;
use drasi_server::{api::models::ConfigValue, computation::ComputationConfig, DrasiServerConfig};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const INSTANCE: &str = "native-sqlite";
/// Native sources and queries share these bootstrap port names.
const BOOTSTRAP_INPUT_PORT: &str = "bootstrap";
const BOOTSTRAP_OUTPUT_PORT: &str = "snapshot";

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
    let value = match value {
        "source" => 1,
        "first" => 2,
        "second" => 3,
        "first-results" => 4,
        "second-results" => 5,
        "source-bootstrap" => 6,
        other => anyhow::bail!("missing explicit component ID for {other}"),
    };
    Ok(ComponentId::from_uuid(uuid::Uuid::from_u128(value)))
}
fn resource(value: &str) -> Result<ResourceId> {
    Ok(ResourceId::try_new(value)?)
}
fn endpoint(component: &str, port: &str) -> Result<Endpoint> {
    Ok(Endpoint::new(id(component)?, PortId::try_new(port)?))
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
        .cloned()
        .context("native SQLite bootstrap factory")?;
    let bootstrap = Arc::new(NativeBootstrapNodeFactory::new(bootstrap));
    let transactions = json!({"max_changes":64,"max_bytes":1048576,"max_duration_ms":5000});
    let mut source_configuration = json!({
        "stream":"source/out",
        "path":database,"tables":["items"],
        "maxTransactionChanges":64,"maxTransactionBytes":1048576,"maxTransactionDurationMs":5000,
        "maxSqlBytes":65536,"commandCapacity":8,"outputCapacity":4,"shutdownTimeoutMs":5000,
        "restApi":true,"restHost":"127.0.0.1","restPort":rest_port,"pageSize":64
    });
    if paired {
        source_configuration["coordinatedSnapshot"] = json!(true);
        source_configuration["maxJournalTransactions"] = json!(16);
        source_configuration["maxJournalBytes"] = json!(8388608);
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
                json!({"kind":"sourceProgress","component":id(name)?,"indexes":"indexes"}),
            )?;
            builder = builder.resource_dependency(
                resource(&progress)?,
                resource("indexes")?,
                ResourceRole::IndexBackend,
            )?;
            consumers.insert(id(name)?, progress);
        }
        let progress = if mode == Mode::Shared {
            builder = declare(
                builder,
                "consumers",
                ResourceRole::Checkpoint,
                json!({"kind":"consumerProgress","producer":id("source")?,"consumers":consumers}),
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
        // The source owns its library's bootstrap coordinator, created from
        // these progress owners; the paired provider node shares it.
        source_spec.dependencies = BTreeMap::from([(
            if mode == Mode::Shared {
                "consumer_progress".into()
            } else {
                "source_progress".into()
            },
            vec![resource(progress)?],
        )]);
    }
    builder = builder
        .component(ComponentName::try_new("source")?, source_spec, source)
        .bind_stream(endpoint("source", "out")?, StreamId::try_new("source/out")?);
    // One visible provider node serves the source; the source relays its
    // consumers' bootstrap sessions to it over bootstrap edges.
    let mut provider_configuration = json!({"source":id("source")?,"stream":"source/out"});
    if !paired {
        for (field, value) in [
            ("path", json!(database)),
            ("tables", json!(["items"])),
            ("maxTables", json!(8)),
            ("maxBytes", json!(1048576)),
            ("maxDurationMs", json!(5000)),
            ("outputCapacity", json!(2)),
            ("shutdownTimeoutMs", json!(5000)),
        ] {
            provider_configuration[field] = value;
        }
    }
    let provider =
        bootstrap.specification_from_config(id("source-bootstrap")?, provider_configuration)?;
    builder = builder.component(
        ComponentName::try_new("source-bootstrap")?,
        provider,
        bootstrap.clone(),
    );
    builder = bootstrap_edge(
        builder,
        endpoint("source-bootstrap", BOOTSTRAP_PROVIDER_OUTPUT_PORT)?,
        endpoint("source", BOOTSTRAP_INPUT_PORT)?,
    );
    builder = declare(
        builder,
        "catalog",
        ResourceRole::QueryCatalog,
        json!({"kind":"queryCatalog"}),
    )?;
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    for name in mode.queries() {
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
        builder = bootstrap_edge(
            builder,
            endpoint("source", BOOTSTRAP_OUTPUT_PORT)?,
            endpoint(name, BOOTSTRAP_INPUT_PORT)?,
        );
        builder = builder
            .component(ComponentName::try_new(*name)?, query, query_factory.clone())
            .component(
                ComponentName::try_new(outlet_id.as_str())?,
                outlet,
                outlet_factory.clone(),
            )
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
