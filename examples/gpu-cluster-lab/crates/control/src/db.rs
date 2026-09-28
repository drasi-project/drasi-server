use anyhow::{ensure, Context, Result};
use gpu_contracts::*;
use gpu_policy::Evaluator;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub const LOCK: i64 = 778807123;

#[derive(Clone, Copy)]
pub enum Table {
    Clusters,
    Policies,
    Data,
    Gpus,
    Settings,
    Workloads,
}
impl Table {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clusters => "regional_clusters",
            Self::Policies => "placement_policies",
            Self::Data => "data_profiles",
            Self::Gpus => "gpu_inventory",
            Self::Settings => "gpu_telemetry",
            Self::Workloads => "workload_requirements",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Clusters => "cluster_id",
            Self::Policies => "policy_id",
            Self::Data => "data_profile_id",
            Self::Gpus | Self::Settings => "gpu_id",
            Self::Workloads => "workload_id",
        }
    }
    pub fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Clusters => &["cluster_id", "name", "region"],
            Self::Policies => &[
                "policy_id",
                "name",
                "customer_id",
                "allowed_regions",
                "allowed_purposes",
                "allowed_classifications",
                "authority_ref",
            ],
            Self::Data => &[
                "data_profile_id",
                "customer_id",
                "classification",
                "policy_id",
                "authority_ref",
            ],
            Self::Gpus => &[
                "gpu_id",
                "name",
                "cluster_id",
                "host_id",
                "gpu_index",
                "model",
                "vm_size",
                "nominal_vram_gb",
                "memory_mib",
                "compute_units",
                "failure_domain",
                "scheduling_enabled",
            ],
            Self::Settings => &[
                "gpu_id",
                "powered_on",
                "reporting_enabled",
                "interval_ms",
                "background_compute_units",
                "background_memory_mib",
            ],
            Self::Workloads => &[
                "workload_id",
                "name",
                "model_ref",
                "profile_id",
                "data_profile_id",
                "purpose",
                "replicas",
                "memory_mib_per_replica",
                "compute_units_per_replica",
                "allowed_gpu_models",
                "spread_across_domains",
            ],
        }
    }
    pub fn editable(self) -> &'static [&'static str] {
        match self {
            Self::Gpus => &["name", "scheduling_enabled"],
            Self::Settings => &[
                "powered_on",
                "reporting_enabled",
                "interval_ms",
                "background_compute_units",
                "background_memory_mib",
            ],
            Self::Policies => &[
                "allowed_regions",
                "allowed_purposes",
                "allowed_classifications",
            ],
            Self::Workloads => &[
                "name",
                "profile_id",
                "data_profile_id",
                "purpose",
                "replicas",
                "spread_across_domains",
            ],
            _ => &[],
        }
    }
}

pub async fn lock(conn: &mut PgConnection) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn reset_pending(conn: &mut PgConnection) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT pending FROM demo_reset_state WHERE fleet_id='demo'")
            .fetch_one(conn)
            .await?,
    )
}

pub async fn set_reset_pending(conn: &mut PgConnection, pending: bool) -> Result<()> {
    lock(conn).await?;
    let result = sqlx::query("UPDATE demo_reset_state SET pending=$1 WHERE fleet_id='demo'")
        .bind(pending)
        .execute(conn)
        .await?;
    ensure!(result.rows_affected() == 1, "missing reset recovery state");
    Ok(())
}

pub async fn rows<T: DeserializeOwned>(conn: &mut PgConnection, table: Table) -> Result<Vec<T>> {
    let sql = format!(
        "SELECT to_jsonb(t)-'updated_at' FROM {} t ORDER BY {}",
        table.name(),
        table.key()
    );
    let rows = sqlx::query_scalar::<_, Value>(&sql).fetch_all(conn).await?;
    rows.into_iter()
        .map(|v| serde_json::from_value(v).map_err(Into::into))
        .collect()
}

pub async fn configuration(conn: &mut PgConnection) -> Result<Configuration> {
    Ok(Configuration {
        clusters: rows::<Cluster>(conn, Table::Clusters)
            .await?
            .into_iter()
            .map(|v| (v.cluster_id.clone(), v))
            .collect(),
        policies: rows::<Policy>(conn, Table::Policies)
            .await?
            .into_iter()
            .map(|v| (v.policy_id.clone(), v))
            .collect(),
        data_profiles: rows::<DataProfile>(conn, Table::Data)
            .await?
            .into_iter()
            .map(|v| (v.data_profile_id.clone(), v))
            .collect(),
        gpus: rows::<Gpu>(conn, Table::Gpus)
            .await?
            .into_iter()
            .map(|v| (v.gpu_id, v))
            .collect(),
        workloads: rows::<Workload>(conn, Table::Workloads)
            .await?
            .into_iter()
            .map(|v| (v.workload_id, v))
            .collect(),
    })
}

pub async fn insert<T: Serialize>(conn: &mut PgConnection, table: Table, row: &T) -> Result<Value> {
    let columns = table.columns().join(",");
    let sql = format!("INSERT INTO {t} ({columns}) SELECT {columns} FROM jsonb_populate_record(NULL::{t},$1) RETURNING to_jsonb({t})-'updated_at'",
        t=table.name());
    Ok(sqlx::query_scalar(&sql)
        .bind(serde_json::to_value(row)?)
        .fetch_one(conn)
        .await?)
}

pub async fn plan(conn: &mut PgConnection) -> Result<Plan> {
    let value: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p)-'committed_at' FROM gpu_placements p WHERE fleet_id='demo'",
    )
    .fetch_one(conn)
    .await?;
    Ok(serde_json::from_value(value)?)
}

pub async fn put_plan(conn: &mut PgConnection, p: &Plan) -> Result<()> {
    let version = i64::try_from(p.plan_version).context("plan version exhausted")?;
    sqlx::query("INSERT INTO gpu_placements (fleet_id,plan_version,decision_id,config_fingerprint,policy_signature,policy_bundle_hash,assignments,decision_details)
        VALUES ('demo',$1,$2,$3,$4,$5,$6,$7) ON CONFLICT (fleet_id) DO UPDATE SET plan_version=EXCLUDED.plan_version,
        decision_id=EXCLUDED.decision_id,config_fingerprint=EXCLUDED.config_fingerprint,policy_signature=EXCLUDED.policy_signature,
        policy_bundle_hash=EXCLUDED.policy_bundle_hash,assignments=EXCLUDED.assignments,decision_details=EXCLUDED.decision_details,committed_at=now()")
        .bind(version).bind(p.decision_id).bind(&p.config_fingerprint).bind(&p.policy_signature).bind(&p.policy_bundle_hash)
        .bind(serde_json::to_value(&p.assignments)?).bind(&p.decision_details).execute(conn).await?;
    Ok(())
}

pub async fn load_fixture(conn: &mut PgConnection, name: &str) -> Result<Plan> {
    lock(conn).await?;
    let previous = plan(conn).await?;
    let fixture = fixtures::load(name)?;
    for table in [
        Table::Workloads,
        Table::Settings,
        Table::Gpus,
        Table::Data,
        Table::Policies,
        Table::Clusters,
    ] {
        sqlx::query(&format!("DELETE FROM {}", table.name()))
            .execute(&mut *conn)
            .await?;
    }
    for v in fixture.configuration.clusters.values() {
        insert(conn, Table::Clusters, v).await?;
    }
    for v in fixture.configuration.policies.values() {
        insert(conn, Table::Policies, v).await?;
    }
    for v in fixture.configuration.data_profiles.values() {
        insert(conn, Table::Data, v).await?;
    }
    for v in fixture.configuration.gpus.values() {
        insert(conn, Table::Gpus, v).await?;
    }
    for v in fixture.settings.values() {
        insert(conn, Table::Settings, v).await?;
    }
    for v in fixture.configuration.workloads.values() {
        insert(conn, Table::Workloads, v).await?;
    }
    let config = configuration(conn).await?;
    ensure!(
        config == fixture.configuration,
        "fixture database representation differs"
    );
    let policy = Evaluator::new()?.evaluate(&config)?;
    policy.validate_plan(&config, &fixture.assignments)?;
    let p = Plan {
        fleet_id: "demo".into(),
        plan_version: previous
            .plan_version
            .checked_add(1)
            .context("plan version exhausted")?,
        decision_id: Uuid::new_v4(),
        config_fingerprint: config.fingerprint()?,
        policy_signature: policy.policy_signature,
        policy_bundle_hash: policy.policy_bundle_hash,
        assignments: fixture.assignments,
        decision_details: json!({"schema_version":1,"reason_codes":["fixture-setup"],"scenario":name}),
    };
    put_plan(conn, &p).await?;
    Ok(p)
}

pub async fn initialize(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    let mut tx = pool.begin().await?;
    lock(&mut tx).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT FROM gpu_placements)")
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        let config = configuration(&mut tx).await?;
        ensure!(
            config == Configuration::default(),
            "refusing to seed a nonempty uninitialized database"
        );
        let p = Evaluator::new()?.evaluate(&config)?;
        put_plan(
            &mut tx,
            &Plan {
                fleet_id: "demo".into(),
                plan_version: 0,
                decision_id: Uuid::new_v4(),
                config_fingerprint: config.fingerprint()?,
                policy_signature: p.policy_signature,
                policy_bundle_hash: p.policy_bundle_hash,
                assignments: vec![],
                decision_details: json!({}),
            },
        )
        .await?;
        load_fixture(&mut tx, "baseline").await?;
    }
    tx.commit().await?;
    Ok(())
}
