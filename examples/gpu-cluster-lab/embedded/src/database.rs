use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_core::middleware::MiddlewareTypeRegistry;
use drasi_index_rocksdb::{
    computation::RocksDbComputationProvider, RocksDbMemoryBudget, RocksIndexOptions,
};
use drasi_lib::computation::v1::*;
use drasi_source_postgres::{
    config::{PostgresSourceConfig, SslMode, TableKeyConfig},
    native::{
        PostgresTransactionConfig, PostgresTransactionSource, PostgresTransactionSourceFactory,
    },
};
use gpu_native::inputs::{self, DATABASE_QUERY};
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgSslMode},
    Connection,
};
use std::{
    collections::BTreeMap,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

pub async fn current_snapshot(
    host: &str,
    password: &str,
    epoch: uuid::Uuid,
) -> Result<inputs::DatabaseSnapshot> {
    use gpu_control::db::{self, Table};
    let options = PgConnectOptions::new()
        .host(host)
        .port(5432)
        .database("gpu_demo")
        .username("gpu_reader")
        .password(password)
        .ssl_mode(PgSslMode::Disable);
    let mut connection = sqlx::PgConnection::connect_with(&options).await?;
    let mut tx = connection.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let snapshot = inputs::DatabaseSnapshot {
        epoch,
        configuration: db::configuration(&mut tx).await?,
        settings: db::rows::<gpu_contracts::Settings>(&mut tx, Table::Settings)
            .await?
            .into_iter()
            .map(|row| (row.gpu_id, row))
            .collect(),
        plan: db::plan(&mut tx).await?,
    };
    tx.commit().await?;
    connection.close().await?;
    Ok(snapshot)
}

struct Settings(Value);
#[async_trait]
impl ConfigurationResolver for Settings {
    fn validate_reference(&self, key: &str) -> Result<()> {
        ensure!(key == "postgres", "unknown database settings reference");
        Ok(())
    }
    async fn resolve(&self, key: &str) -> Result<Value> {
        self.validate_reference(key)?;
        Ok(self.0.clone())
    }
}

pub fn components(
    host: &str,
    password: &str,
    path: &str,
    catalog: QueryResultsCatalog,
    middleware: Arc<MiddlewareTypeRegistry>,
) -> Result<ComponentBatch> {
    let tables = [
        ("regional_clusters", "cluster_id"),
        ("placement_policies", "policy_id"),
        ("data_profiles", "data_profile_id"),
        ("gpu_inventory", "gpu_id"),
        ("gpu_telemetry", "gpu_id"),
        ("workload_requirements", "workload_id"),
        ("gpu_placements", "fleet_id"),
    ];
    let limits = SourceTransactionLimits {
        max_changes: NonZeroUsize::new(1024).unwrap(),
        max_bytes: NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
        max_duration_ms: NonZeroU64::new(30_000).unwrap(),
    };
    let settings = PostgresTransactionConfig {
        connection: PostgresSourceConfig {
            host: host.into(),
            port: 5432,
            database: "gpu_demo".into(),
            user: "gpu_reader".into(),
            password: password.into(),
            tables: tables.iter().map(|(table, _)| (*table).into()).collect(),
            slot_name: "gpu_native".into(),
            publication_name: "gpu_demo_publication".into(),
            ssl_mode: SslMode::Disable,
            table_keys: tables
                .iter()
                .map(|(table, key)| TableKeyConfig {
                    table: (*table).into(),
                    key_columns: vec![(*key).into()],
                })
                .collect(),
        },
        tls_ca_pem: None,
        start_lsn: "0/0".into(),
        transactions: limits,
        max_protocol_bytes: NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
        io_timeout_ms: NonZeroU64::new(30_000).unwrap(),
        feedback_interval_ms: NonZeroU64::new(1_000).unwrap(),
    };
    let progress = Arc::new(QuerySourceProgress::new(
        catalog.graph_id(),
        super::id(DATABASE_QUERY)?,
    )?);
    let (_, snapshot) = PostgresTransactionSource::coordinated(
        super::id("postgres")?,
        StreamId::try_new("postgres/out")?,
        settings.clone(),
        progress.clone(),
    )?;
    let provider = Arc::new(RocksDbComputationProvider::new(
        path,
        RocksIndexOptions::new(
            false,
            false,
            RocksDbMemoryBudget::from_total_budget_bytes(64 << 20)?,
        ),
    ));
    let source_factory = Arc::new(PostgresTransactionSourceFactory::default());
    let query_factory = FactoryRegistry::standard()
        .get(
            &ContinuousQueryFactory::default()
                .descriptor()
                .implementation,
        )
        .context("built-in continuous query factory is missing")?
        .clone();
    let ids = [
        "gpu-db-indexes",
        "gpu-db-progress",
        "gpu-db-bootstrap",
        "gpu-db-secrets",
        "gpu-db-middleware",
        "gpu-db-catalog",
    ]
    .map(ResourceId::try_new);
    let [indexes, progress_id, bootstrap, secrets, middleware_id, catalog_id] = ids;
    let (indexes, progress_id, bootstrap, secrets, middleware_id, catalog_id) = (
        indexes?,
        progress_id?,
        bootstrap?,
        secrets?,
        middleware_id?,
        catalog_id?,
    );
    let mut batch = ComponentBatch::builder();
    for (id, role, handle) in [
        (
            indexes.clone(),
            ResourceRole::IndexBackend,
            ResourceHandle::new(
                ResourceRole::IndexBackend,
                Arc::new(QueryIndexProviderResource(provider)),
            ),
        ),
        (
            progress_id.clone(),
            ResourceRole::Checkpoint,
            ResourceHandle::new(
                ResourceRole::Checkpoint,
                Arc::new(QuerySourceProgressResource(progress)),
            ),
        ),
        (
            bootstrap.clone(),
            ResourceRole::Bootstrap,
            ResourceHandle::new(ResourceRole::Bootstrap, snapshot.clone()),
        ),
        (
            secrets.clone(),
            ResourceRole::SecretStore,
            ResourceHandle::new(
                ResourceRole::SecretStore,
                Arc::new(ConfigurationResolverResource(Arc::new(Settings(
                    serde_json::to_value(&settings)?,
                )))),
            ),
        ),
        (
            middleware_id.clone(),
            ResourceRole::Middleware,
            ResourceHandle::new(
                ResourceRole::Middleware,
                Arc::new(QueryMiddlewareResource(middleware)),
            ),
        ),
        (
            catalog_id.clone(),
            ResourceRole::QueryCatalog,
            ResourceHandle::new(ResourceRole::QueryCatalog, Arc::new(catalog.clone())),
        ),
    ] {
        batch = batch
            .declare_resource(ResourceSpecification {
                id: id.clone(),
                role,
                ownership: ResourceOwnership::Graph,
                binding: id.as_str().into(),
            })?
            .provide_resource(id, handle)?;
    }
    let query_bootstrap = ResourceId::try_new("gpu-db-query-bootstrap")?;
    batch = batch
        .declare_resource(ResourceSpecification {
            id: query_bootstrap.clone(),
            role: ResourceRole::Bootstrap,
            ownership: ResourceOwnership::Graph,
            binding: "gpu-db-query-bootstrap".into(),
        })?
        .provide_resource(
            query_bootstrap.clone(),
            ResourceHandle::new(
                ResourceRole::Bootstrap,
                Arc::new(QueryBootstrapResource(snapshot)),
            ),
        )?;
    let text = inputs::database_queries().remove(0).1;
    let definition = ContinuousQueryDefinition {
        graph_id: catalog.graph_id().into(),
        id: super::id(DATABASE_QUERY)?,
        query: text.clone(),
        language: ComputationQueryLanguage::Cypher,
        output_stream: StreamId::try_new(format!("{DATABASE_QUERY}/out"))?,
        outbox_capacity: NonZeroUsize::new(128).unwrap(),
    };
    let execution = QueryExecutionSettings {
        source_transactions: Some(limits),
        middleware: vec![drasi_core::models::SourceMiddlewareConfig {
            name: gpu_native::postgres::POSTGRES_JSON.into(),
            kind: gpu_native::postgres::POSTGRES_JSON.into(),
            config: Default::default(),
        }],
        sources: vec![serde_json::from_value(json!({
            "source_id":"postgres", "pipeline":[gpu_native::postgres::POSTGRES_JSON],
        }))?],
        ..Default::default()
    };
    batch = batch
        .component(
            ComponentSpecification {
                descriptor: PostgresTransactionSource::describe(super::id("postgres")?)?,
                role: ComponentRole::Source,
                completion: None,
                implementation: source_factory.descriptor().implementation.clone(),
                configuration_version: 1,
                configuration: BTreeMap::from([
                    (
                        "stream".into(),
                        ConfigurationValue::Literal(json!("postgres/out")),
                    ),
                    (
                        "coordinated_snapshot".into(),
                        ConfigurationValue::Literal(json!(true)),
                    ),
                    (
                        "settings".into(),
                        ConfigurationValue::Reference {
                            resource: secrets,
                            key: "postgres".into(),
                            secret: true,
                        },
                    ),
                ]),
                dependencies: BTreeMap::from([
                    ("source_progress".into(), vec![progress_id.clone()]),
                    ("snapshot".into(), vec![bootstrap]),
                ]),
            },
            source_factory,
        )
        .bind_stream(
            super::endpoint("postgres", "out")?,
            StreamId::try_new("postgres/out")?,
        )
        .component(
            ComponentSpecification {
                descriptor: definition.descriptor_with_execution(&execution),
                role: ComponentRole::Query,
                completion: None,
                implementation: query_factory.descriptor().implementation.clone(),
                configuration_version: 1,
                configuration: BTreeMap::from([
                    ("query".into(), ConfigurationValue::Literal(json!(text))),
                    (
                        "stream".into(),
                        ConfigurationValue::Literal(json!(definition.output_stream)),
                    ),
                    (
                        "outbox_capacity".into(),
                        ConfigurationValue::Literal(json!(128)),
                    ),
                    (
                        "execution".into(),
                        ConfigurationValue::Literal(serde_json::to_value(execution)?),
                    ),
                ]),
                dependencies: BTreeMap::from([
                    ("indexes".into(), vec![indexes]),
                    ("catalog".into(), vec![catalog_id]),
                    ("source_progress".into(), vec![progress_id]),
                    ("bootstrap".into(), vec![query_bootstrap]),
                    ("middleware".into(), vec![middleware_id]),
                ]),
            },
            query_factory,
        )
        .bind_stream(
            super::endpoint(DATABASE_QUERY, "out")?,
            definition.output_stream,
        )
        .sink(Box::new(QueryResultsOutlet::new(
            super::id("database-results")?,
            catalog,
        )));
    let mut batch = batch.build()?;
    batch.definition.relationships.extend([
        super::relationship("postgres", DATABASE_QUERY, 1)?,
        super::relationship(DATABASE_QUERY, "database-results", 32)?,
    ]);
    Ok(batch)
}
