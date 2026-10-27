// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Generate the native MySQL example from the matching libraries' real metadata.

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
use drasi_server::DrasiServerConfig;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    instance: String,
    source: String,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    id: String,
    query: String,
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
fn stream(component: &str) -> Result<StreamId> {
    Ok(StreamId::try_new(format!("{component}/out"))?)
}
fn literal(value: impl serde::Serialize) -> Result<ConfigurationValue> {
    Ok(ConfigurationValue::Literal(serde_json::to_value(value)?))
}
fn reference(key: &str) -> Result<ConfigurationValue> {
    Ok(ConfigurationValue::Reference {
        resource: resource("mysql-configuration")?,
        key: format!("env-json:{key}").into(),
        secret: true,
    })
}
fn load(directory: &Path, package: &str) -> Result<Arc<NativePlugin>> {
    let file = directory.join(format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_SUFFIX
    ));
    drasi_host_sdk::computation::load(&file).with_context(|| format!("load {}", file.display()))
}
fn factory(plugin: &NativePlugin, name: &str) -> Result<Arc<NativeFactory>> {
    plugin
        .factories()
        .iter()
        .find(|factory| factory.metadata().implementation.name.as_ref() == name)
        .cloned()
        .with_context(|| format!("native MySQL factory {name}"))
}
fn declare(
    builder: ComponentBatchBuilder,
    name: &str,
    role: ResourceRole,
    recipe: Value,
) -> Result<ComponentBatchBuilder> {
    let name = resource(name)?;
    Ok(builder
        .declare_resource(ResourceSpecification {
            id: name.clone(),
            role,
            ownership: if role == ResourceRole::SecretStore {
                ResourceOwnership::Borrowed
            } else {
                ResourceOwnership::Graph
            },
            binding: name.as_str().into(),
        })?
        .resource_configuration(name, recipe)?)
}

fn generate(directory: &Path, recipe: &Path) -> Result<Value> {
    let recipe: Recipe = serde_json::from_slice(&std::fs::read(recipe)?)?;
    id(&recipe.instance)?;
    anyhow::ensure!(
        !recipe.queries.is_empty(),
        "MySQL needs actual query owners"
    );
    let sources = load(directory, "drasi-computation-source-mysql")?;
    let bootstraps = load(directory, "drasi-computation-bootstrap-mysql")?;
    let sinks = load(directory, "drasi-computation-sink-storedproc-mysql")?;
    let source_factory = factory(&sources, "drasi.mysql/shared-changes")?;
    let sink_factory = factory(&sinks, "drasi.mysql/storedproc")?;
    let bootstrap = bootstraps
        .bootstrap_factories()
        .iter()
        .find(|factory| {
            factory.metadata().implementation.name.as_ref() == "drasi.mysql/shared-row-bootstrap"
        })
        .context("native MySQL row bootstrap factory")?
        .metadata();
    let query_factory = Arc::new(ContinuousQueryFactory::default());
    let outlet_factory = Arc::new(QueryResultsOutletFactory::default());
    let mut builder = ComponentBatch::builder();
    for (name, role, recipe) in [
        (
            "mysql-configuration",
            ResourceRole::SecretStore,
            json!({"kind":"configuration"}),
        ),
        (
            "mysql-indexes",
            ResourceRole::IndexBackend,
            json!({"kind":"memoryIndexes"}),
        ),
        (
            "mysql-results",
            ResourceRole::QueryCatalog,
            json!({"kind":"queryCatalog"}),
        ),
    ] {
        builder = declare(builder, name, role, recipe)?;
    }
    let mut consumers = BTreeMap::new();
    for query in &recipe.queries {
        let progress = format!("{}-progress", query.id);
        let bootstrap_id = format!("{}-bootstrap", query.id);
        anyhow::ensure!(
            consumers
                .insert(query.id.clone(), progress.clone())
                .is_none(),
            "duplicate MySQL query owner {}",
            query.id
        );
        builder = declare(
            builder,
            &progress,
            ResourceRole::Checkpoint,
            json!({"kind":"sourceProgress","component":query.id}),
        )?;
        let configuration = NativeBootstrapConfig {
            component: id(&query.id)?,
            implementation: bootstrap.implementation.clone(),
            configuration_version: bootstrap.configuration_version,
            configuration: BTreeMap::from([
                ("source".into(), literal(&recipe.source)?),
                ("stream".into(), literal(stream(&recipe.source)?)?),
                ("settings".into(), reference("DRASI_MYSQL_SOURCE_SETTINGS")?),
            ]),
            source_progress: Some(resource(&progress)?),
            source_bootstrap: None,
        };
        let mut bootstrap_recipe = serde_json::to_value(configuration)?;
        bootstrap_recipe["kind"] = json!("nativeBootstrap");
        builder = declare(
            builder,
            &bootstrap_id,
            ResourceRole::Bootstrap,
            bootstrap_recipe,
        )?
        .resource_dependency(
            resource(&bootstrap_id)?,
            resource(&progress)?,
            ResourceRole::Checkpoint,
        )?
        .resource_dependency(
            resource(&bootstrap_id)?,
            resource("mysql-configuration")?,
            ResourceRole::SecretStore,
        )?;
        let mut specification = query_factory.specification_from_config(
            id(&query.id)?,
            json!({
                "query":query.query,"language":"cypher","stream":stream(&query.id)?,
                "outbox_capacity":32,
            }),
        )?;
        specification.dependencies = BTreeMap::from([
            ("indexes".into(), vec![resource("mysql-indexes")?]),
            ("catalog".into(), vec![resource("mysql-results")?]),
            ("source_progress".into(), vec![resource(&progress)?]),
            ("bootstrap".into(), vec![resource(&bootstrap_id)?]),
        ]);
        builder = builder
            .component(specification, query_factory.clone())
            .bind_stream(endpoint(&query.id, "out")?, stream(&query.id)?)
            .connect(
                EdgeDefinition::new(endpoint(&recipe.source, "out")?, endpoint(&query.id, "in")?),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            );
    }
    builder = declare(
        builder,
        "mysql-consumers",
        ResourceRole::Checkpoint,
        json!({"kind":"consumerProgress","producer":recipe.source,"consumers":consumers}),
    )?;
    for progress in consumers.values() {
        builder = builder.resource_dependency(
            resource("mysql-consumers")?,
            resource(progress)?,
            ResourceRole::Checkpoint,
        )?;
    }
    let mut source = source_factory.specification(
        id(&recipe.source)?,
        json!({"stream":stream(&recipe.source)?,"settings":{},"persistent":false}),
    )?;
    source
        .configuration
        .insert("settings".into(), reference("DRASI_MYSQL_SOURCE_SETTINGS")?);
    source.dependencies.insert(
        "consumer_progress".into(),
        vec![resource("mysql-consumers")?],
    );
    let mut sink = sink_factory.specification(id("mysql-procedures")?, json!({"settings":{}}))?;
    sink.configuration
        .insert("settings".into(), reference("DRASI_MYSQL_SINK_SETTINGS")?);
    let mut outlet =
        outlet_factory.specification_from_config(id("mysql-query-results")?, json!({}))?;
    outlet
        .dependencies
        .insert("catalog".into(), vec![resource("mysql-results")?]);
    builder = builder
        .component(source, source_factory)
        .bind_stream(endpoint(&recipe.source, "out")?, stream(&recipe.source)?)
        .component(sink, sink_factory)
        .component(outlet, outlet_factory);
    for query in &recipe.queries {
        for destination in ["mysql-procedures", "mysql-query-results"] {
            builder = builder.connect(
                EdgeDefinition::new(endpoint(&query.id, "out")?, endpoint(destination, "in")?),
                Box::new(BoundedPipeConfig { capacity: 8 }),
            );
        }
    }
    let definition = builder.build()?.definition;
    let config: DrasiServerConfig = serde_json::from_value(json!({
        "apiVersion":"drasi.io/v1","host":"127.0.0.1","port":8080,"logLevel":"info",
        "persistConfig":false,"enableUi":false,"autoInstallPlugins":false,
        "instances":[{
            "id":recipe.instance,"persistIndex":false,
            "computation":{"definition":definition}
        }]
    }))?;
    config.validate()?;
    Ok(serde_json::to_value(config)?)
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().context(
        "usage: native-mysql generate <plugins> <recipe.json> <server.yaml> | inspect <server.yaml>",
    )?;
    match mode.to_str().context("mode is not UTF-8")? {
        "generate" => {
            let directory = PathBuf::from(args.next().context("native plugin directory")?);
            let recipe = PathBuf::from(args.next().context("MySQL application recipe")?);
            let output = PathBuf::from(args.next().context("output configuration")?);
            anyhow::ensure!(args.next().is_none(), "unexpected arguments");
            let config = generate(&directory, &recipe)?;
            std::fs::write(
                output,
                format!(
                    "# Generated by examples/native-mysql.rs from examples/mysql/recipe.json.\n\
                     # Real factory metadata and canonical secret references; do not edit by hand.\n{}",
                    serde_yaml::to_string(&config)?
                ),
            )?;
        }
        "inspect" => {
            let file = PathBuf::from(args.next().context("configuration path")?);
            anyhow::ensure!(args.next().is_none(), "unexpected arguments");
            let value: Value = serde_yaml::from_slice(&std::fs::read(file)?)?;
            println!("{}", serde_json::to_string(&value)?);
        }
        _ => anyhow::bail!("unknown MySQL example generation mode"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../drasi-core/components/host-sdk/tests/fixtures/native_mysql/database.rs"]
mod database;

#[cfg(test)]
mod tests {
    use super::*;
    use database::Database;
    use drasi_lib::{error::OperationFailures, secret_store::MemorySecretStoreProvider, DrasiLib};
    use drasi_server::{
        api::v1::routes::build_v1_router,
        computation::{configuration_from_snapshot, register_components, ComputationConfig},
        instance_registry::InstanceRegistry,
        plugin_registry::PluginRegistry,
    };
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use tokio::sync::{oneshot, RwLock};

    const DEADLINE: Duration = Duration::from_secs(30);

    struct TestIdentity(AtomicUsize);

    #[async_trait::async_trait]
    impl drasi_plugin_sdk::IdentityProviderPluginDescriptor for TestIdentity {
        fn kind(&self) -> &str {
            "mysql-test-password"
        }
        fn config_version(&self) -> &str {
            "1"
        }
        fn config_schema_name(&self) -> &str {
            "MySqlIdentity"
        }
        fn config_schema_json(&self) -> String {
            r#"{"MySqlIdentity":{"type":"object","required":["username","password"],"properties":{"username":{"type":"string"},"password":{}},"additionalProperties":false}}"#.into()
        }
        async fn create_identity_provider(
            &self,
            configuration: &Value,
        ) -> Result<Box<dyn drasi_lib::identity::IdentityProvider>> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Settings {
                username: String,
                password: drasi_plugin_sdk::config_value::ConfigValue<String>,
            }
            let settings: Settings = serde_json::from_value(configuration.clone())?;
            self.0.fetch_add(1, Ordering::SeqCst);
            let password = drasi_plugin_sdk::mapper::DtoMapper::new()
                .resolve_string(&settings.password)
                .await?;
            Ok(Box::new(
                drasi_lib::identity::PasswordIdentityProvider::new(settings.username, password),
            ))
        }
    }

    async fn results(client: &reqwest::Client, base: &str, expected: &[Value]) -> Result<()> {
        tokio::time::timeout(DEADLINE, async {
            loop {
                let mut complete = true;
                for query in ["all-items", "selected-items"] {
                    let body: Value = client
                        .get(format!("{base}/instances/mysql/queries/{query}/results"))
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    anyhow::ensure!(body["success"] == true, "Server query request failed");
                    let mut actual = body["data"]
                        .as_array()
                        .context("Server query result data")?
                        .clone();
                    actual.sort_by_key(|row| row["id"].as_i64());
                    let expected: Vec<_> = expected
                        .iter()
                        .filter(|row| {
                            query == "all-items" || row["id"].as_i64().is_some_and(|id| id >= 2)
                        })
                        .cloned()
                        .collect();
                    complete &= actual == expected;
                }
                if complete {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("Server did not expose the expected MySQL query rows")?
    }

    async fn round(
        config: &ComputationConfig,
        plugins: Arc<RwLock<PluginRegistry>>,
        secrets: Arc<MemorySecretStoreProvider>,
        database: &Database,
        restarted: bool,
        identity: Option<Arc<TestIdentity>>,
    ) -> Result<ComputationConfig> {
        let instances = InstanceRegistry::new();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let base = format!("http://{}", listener.local_addr()?);
        let client = reqwest::Client::builder().timeout(DEADLINE).build()?;
        let core = Arc::new(
            DrasiLib::builder()
                .with_id("mysql")
                .with_secret_store_provider(secrets.clone())
                .build()
                .await?,
        );
        let prior_creations = identity
            .as_ref()
            .map(|identity| identity.0.load(Ordering::SeqCst));
        let setup = async {
            let registry = plugins.read().await;
            register_components(config, &core, &registry).await?;
            if let Some(identity) = &identity {
                anyhow::ensure!(
                    Some(identity.0.load(Ordering::SeqCst)) == prior_creations,
                    "Server construction acquired identity credentials"
                );
            }
            drop(registry);
            core.start().await?;
            for component in [
                "mysql-source",
                "all-items",
                "selected-items",
                "mysql-procedures",
                "mysql-query-results",
            ] {
                tokio::time::timeout(
                    DEADLINE,
                    core.computation_component(component)?.wait_started(),
                )
                .await??;
            }
            if let Some(identity) = &identity {
                anyhow::ensure!(
                    Some(identity.0.load(Ordering::SeqCst))
                        == prior_creations.map(|count| count + 1),
                    "Server did not resolve identity on activation"
                );
            }
            instances
                .add("mysql".into(), core.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = setup {
            return match core.shutdown().await {
                Ok(()) => Err(error),
                Err(cleanup) => Err(OperationFailures::new(
                    "MySQL Server setup and cleanup failed",
                    vec![error, cleanup.into()],
                )
                .into()),
            };
        }

        let router = build_v1_router(instances.clone(), Arc::new(false), None, plugins, None);
        let (stop, stopped) = oneshot::channel();
        let mut http = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        let proof = async {
            let schema = tokio::time::timeout(DEADLINE, core.get_source_schema("mysql-source"))
                .await??
                .context("native MySQL catalog schema")?;
            anyhow::ensure!(
                schema.nodes.len() == 1 && schema.nodes[0].label == "items",
                "Server source inspection did not read the actual MySQL catalog"
            );
            anyhow::ensure!(
                core.get_graph_schema()
                    .await?
                    .nodes
                    .get("items")
                    .is_some_and(|node| node.sources.contains("mysql-source")),
                "Server graph schema omitted native MySQL"
            );
            if restarted {
                results(
                    &client,
                    &base,
                    &[
                        json!({"id":1,"value":"offline"}),
                        json!({"id":3,"value":"barrier"}),
                    ],
                )
                .await?;
                database
                    .sql("UPDATE items SET value='resumed' WHERE id=1")
                    .await?;
                results(
                    &client,
                    &base,
                    &[
                        json!({"id":1,"value":"resumed"}),
                        json!({"id":3,"value":"barrier"}),
                    ],
                )
                .await?;
                database
                    .expect_count("SELECT COUNT(*) FROM audit", "8")
                    .await?;
                anyhow::ensure!(
                    database
                        .sql("SELECT value FROM effects WHERE query_id='all-items' AND id=1")
                        .await?
                        == "resumed",
                    "native MySQL procedure effect missing after Server reconstruction"
                );
            } else {
                results(
                    &client,
                    &base,
                    &[
                        json!({"id":1,"value":"initial"}),
                        json!({"id":2,"value":"selected"}),
                    ],
                )
                .await?;
                database
                    .sql(
                        "BEGIN; UPDATE items SET value='updated' WHERE id=1; \
                    INSERT INTO items VALUES(3,'third'); DELETE FROM items WHERE id=2; COMMIT;",
                    )
                    .await?;
                results(
                    &client,
                    &base,
                    &[
                        json!({"id":1,"value":"updated"}),
                        json!({"id":3,"value":"third"}),
                    ],
                )
                .await?;
                database
                    .expect_count("SELECT COUNT(*) FROM audit", "5")
                    .await?;
                anyhow::ensure!(
                    database
                        .sql("SELECT query_id,id,value FROM effects ORDER BY query_id,id")
                        .await?
                        == "all-items\t1\tupdated\nall-items\t3\tthird\nselected-items\t3\tthird",
                    "native MySQL procedure routing or bound values changed"
                );
                database
                    .sql(
                        "BEGIN; UPDATE items SET value='rolled-back' WHERE id=1; \
                    INSERT INTO items VALUES(4,'rolled-back'); ROLLBACK; \
                    UPDATE items SET value='barrier' WHERE id=3;",
                    )
                    .await?;
                results(
                    &client,
                    &base,
                    &[
                        json!({"id":1,"value":"updated"}),
                        json!({"id":3,"value":"barrier"}),
                    ],
                )
                .await?;
                database
                    .expect_count("SELECT COUNT(*) FROM audit", "7")
                    .await?;
                if let Some(identity) = &identity {
                    let sink = core.computation_component("mysql-procedures")?;
                    sink.stop().await?;
                    database
                        .sql(
                            "SET SESSION sql_log_bin=0; \
                        ALTER USER native_identity@'%' IDENTIFIED BY 'server-second-auth'",
                        )
                        .await?;
                    secrets
                        .set_secret("MYSQL_ACTIVATION_PASSWORD", "server-second-auth")
                        .await;
                    sink.start().await?;
                    anyhow::ensure!(
                        Some(identity.0.load(Ordering::SeqCst))
                            == prior_creations.map(|count| count + 2),
                        "Server restart did not re-resolve the identity secret"
                    );
                }
            }
            let snapshot = core.snapshot_computation_configuration().await?;
            let saved = configuration_from_snapshot(&snapshot)?
                .context("MySQL Server configuration snapshot")?;
            anyhow::ensure!(
                saved.definition.resource_configurations
                    == config.definition.resource_configurations,
                "Server changed native MySQL bootstrap resource recipes"
            );
            let serialized = serde_json::to_string(&saved)?;
            anyhow::ensure!(
                serialized.contains("secret-json:DRASI_MYSQL_SOURCE_SETTINGS")
                    && serialized.contains("secret-json:DRASI_MYSQL_SINK_SETTINGS")
                    && !serialized.contains("native-test")
                    && !serialized.contains("server-first-auth")
                    && !serialized.contains("server-second-auth"),
                "Server replaced MySQL credential references with resolved secrets"
            );
            Ok::<_, anyhow::Error>(saved)
        }
        .await;

        drop(client);
        let _ = stop.send(());
        let http_cleanup = match tokio::time::timeout(DEADLINE, &mut http).await {
            Ok(joined) => joined
                .map_err(anyhow::Error::from)
                .and_then(|result| result.map_err(Into::into)),
            Err(timeout) => {
                http.abort();
                let mut failures = vec![timeout.into()];
                match http.await {
                    Err(error) if !error.is_cancelled() => failures.push(error.into()),
                    Ok(Err(error)) => failures.push(error.into()),
                    _ => {}
                }
                Err(OperationFailures::new("MySQL Server HTTP cleanup timed out", failures).into())
            }
        };
        let cleanup = core.shutdown().await.map_err(anyhow::Error::from);
        instances.remove("mysql").await;
        let retired = database.expect_count(
            "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER='root' AND ID<>CONNECTION_ID()",
            "0",
        ).await;
        let mut failures: Vec<_> = [http_cleanup, cleanup, retired]
            .into_iter()
            .filter_map(Result::err)
            .collect();
        match proof {
            Ok(saved) if failures.is_empty() => Ok(saved),
            result => {
                if let Err(error) = result {
                    failures.insert(0, error);
                }
                Err(
                    OperationFailures::new("MySQL Server proof and cleanup failed", failures)
                        .into(),
                )
            }
        }
    }

    async fn real_server(with_identity: bool) -> Result<()> {
        let libraries = std::env::var_os("DRASI_NATIVE_PLUGIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
        let recipe = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/mysql/recipe.json");
        let generated = generate(&libraries, &recipe)?;
        anyhow::ensure!(
            generated == generate(&libraries, &recipe)?,
            "MySQL generation is not reproducible"
        );
        let checked_in: Value =
            serde_yaml::from_slice(&std::fs::read(recipe.with_file_name("server.yaml"))?)?;
        anyhow::ensure!(
            generated == checked_in,
            "MySQL checked-in configuration differs from factory metadata"
        );
        let serialized = serde_json::to_string(&generated)?
            .replace("env-json:DRASI_MYSQL", "secret-json:DRASI_MYSQL");
        let server: DrasiServerConfig = serde_json::from_str(&serialized)?;
        server.validate()?;
        let mut config = server.instances[0]
            .computation
            .as_ref()
            .context("MySQL Server computation configuration")?
            .resolve(&FactoryRegistry::default())?
            .into_owned();
        let mut registry = PluginRegistry::new();
        drasi_server::register_core_plugins(&mut registry);
        let identity = with_identity.then(|| Arc::new(TestIdentity(AtomicUsize::new(0))));
        if let Some(identity) = &identity {
            registry.register_identity_provider(identity.clone());
            let identity_id = resource("mysql-identity")?;
            config.definition.resources.push(ResourceSpecification {
                id: identity_id.clone(),
                role: ResourceRole::Identity,
                ownership: ResourceOwnership::Graph,
                binding: "mysql-identity".into(),
            });
            config.definition.resource_configurations.insert(identity_id.clone(), json!({
                "kind":"nativeIdentity",
                "provider":{"kind":"mysql-test-password","config_version":"1",
                    "config":{"username":"native_identity","password":{"kind":"Secret","name":"MYSQL_ACTIVATION_PASSWORD"}}}
            }));
            let sink = config
                .definition
                .components
                .iter_mut()
                .find(|component| component.descriptor.id().as_str() == "mysql-procedures")
                .context("MySQL procedure definition")?;
            let ComponentConstruction::Factory(specification) = &mut sink.construction else {
                anyhow::bail!("MySQL sink must remain factory-backed");
            };
            specification
                .dependencies
                .insert("identity".into(), vec![identity_id]);
        }
        for package in [
            "drasi-computation-source-mysql",
            "drasi-computation-bootstrap-mysql",
            "drasi-computation-sink-storedproc-mysql",
        ] {
            registry.register_computation_plugin(load(&libraries, package)?)?;
        }
        let registry = Arc::new(RwLock::new(registry));
        let mut database = Database::new().await?;
        let proof = async {
            database.initialize().await?;
            database
                .sql("INSERT INTO items VALUES(2,'selected')")
                .await?;
            let mut sink = database.sink_settings();
            sink["outputs"] = json!({
                "all-items": {"component": "all-items", "port": "out"},
                "selected-items": {"component": "selected-items", "port": "out"},
            });
            if with_identity {
                database
                    .sql(
                        "SET SESSION sql_log_bin=0; \
                    CREATE USER native_identity@'%' IDENTIFIED BY 'server-first-auth'; \
                    GRANT EXECUTE ON test.* TO native_identity@'%'",
                    )
                    .await?;
                let settings = sink.as_object_mut().context("MySQL sink settings")?;
                settings.remove("user");
                settings.remove("password");
            }
            let secrets = Arc::new(
                MemorySecretStoreProvider::new()
                    .with_secret(
                        "DRASI_MYSQL_SOURCE_SETTINGS",
                        database.source_settings().to_string(),
                    )
                    .with_secret("DRASI_MYSQL_SINK_SETTINGS", sink.to_string())
                    .with_secret("MYSQL_ACTIVATION_PASSWORD", "server-first-auth"),
            );
            config = round(
                &config,
                registry.clone(),
                secrets.clone(),
                &database,
                false,
                identity.clone(),
            )
            .await?;
            database
                .sql("UPDATE items SET value='offline' WHERE id=1")
                .await?;
            round(
                &config,
                registry,
                secrets,
                &database,
                true,
                identity.clone(),
            )
            .await?;
            if let Some(identity) = &identity {
                anyhow::ensure!(
                    identity.0.load(Ordering::SeqCst) == 3,
                    "Server reconstruction lost activation-scoped identity resolution"
                );
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let cleanup = database.cleanup();
        let failures: Vec<_> = [proof, cleanup]
            .into_iter()
            .filter_map(Result::err)
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(OperationFailures::new("MySQL Server qualification failed", failures).into())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires Podman and three rebuilt native MySQL libraries"]
    async fn native_mysql_server_current_thread() -> Result<()> {
        real_server(false).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires Podman and three rebuilt native MySQL libraries"]
    async fn native_mysql_server_multi_thread() -> Result<()> {
        real_server(false).await
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires Podman and three rebuilt native MySQL libraries"]
    async fn native_mysql_identity_server_current_thread() -> Result<()> {
        real_server(true).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires Podman and three rebuilt native MySQL libraries"]
    async fn native_mysql_identity_server_multi_thread() -> Result<()> {
        real_server(true).await
    }
}
