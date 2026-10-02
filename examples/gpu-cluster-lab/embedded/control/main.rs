use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use futures_util::StreamExt;
use gpu_contracts::*;
use gpu_control::db::{self, Table};
use gpu_policy::Evaluator;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{watch, RwLock};
use tower_http::services::ServeDir;
use uuid::Uuid;

#[derive(Clone)]
struct App {
    config: PgPool,
    placements: PgPool,
    reset: PgPool,
    gate: Arc<RwLock<bool>>,
    shutdown: watch::Receiver<bool>,
    stream_resets: watch::Sender<()>,
    client: reqwest::Client,
    drasi: String,
    sse: String,
    origin: String,
    token: String,
    csrf: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}
type ApiResult<T> = std::result::Result<T, ApiError>;
impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "INVALID_REQUEST",
            message: message.into(),
        }
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "REVISION_CONFLICT",
            message: message.into(),
        }
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error=%error,"demo operation failed");
        let constraint = error
            .downcast_ref::<sqlx::Error>()
            .and_then(|e| e.as_database_error())
            .and_then(|e| e.code())
            .is_some_and(|code| code.starts_with("23") || code.starts_with("22"));
        if constraint {
            Self::invalid("Database constraint rejected the command")
        } else {
            Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "OPERATION_FAILED",
                message: "Operation failed; inspect the control service logs".into(),
            }
        }
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"code":self.code,"message":self.message})),
        )
            .into_response()
    }
}

fn header<'a>(h: &'a HeaderMap, key: &str) -> ApiResult<&'a str> {
    h.get(key)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::invalid(format!("Missing {key}")))
}
fn authorize(app: &App, h: &HeaderMap, internal: bool) -> ApiResult<()> {
    // Reject browser origins even with an internal token: internal credentials never belong in a browser.
    if h.get("origin").is_none()
        && h.get("authorization").and_then(|h| h.to_str().ok())
            == Some(format!("Bearer {}", app.token).as_str())
    {
        return Ok(());
    }
    if !internal
        && h.get("origin").and_then(|h| h.to_str().ok()) == Some(app.origin.as_str())
        && h.get("x-csrf-token").and_then(|h| h.to_str().ok()) == Some(app.csrf.as_str())
    {
        return Ok(());
    }
    Err(ApiError {
        status: StatusCode::FORBIDDEN,
        code: "FORBIDDEN",
        message: "Command authorization failed".into(),
    })
}
async fn available(app: &App) -> ApiResult<tokio::sync::RwLockReadGuard<'_, bool>> {
    let guard = app
        .gate
        .try_read()
        .map_err(|_| ApiError::from(anyhow::anyhow!("reset in progress")))?;
    if !*guard {
        return Err(ApiError::from(anyhow::anyhow!(
            "reset failed; retry an explicit reset"
        )));
    }
    Ok(guard)
}

async fn restored_gate(reset: &PgPool) -> Result<Arc<RwLock<bool>>> {
    let pending = db::reset_pending(&mut *reset.acquire().await?).await?;
    if pending {
        tracing::warn!(
            "incomplete reset recovered; access remains gated until explicit reset retry"
        );
    }
    Ok(Arc::new(RwLock::new(!pending)))
}

async fn receipt(
    conn: &mut sqlx::PgConnection,
    kind: &str,
    key: &str,
    hash: &str,
) -> ApiResult<Option<Value>> {
    nonempty(key).map_err(|e| ApiError::invalid(e.to_string()))?;
    let row:Option<(String,Value)> = sqlx::query_as("SELECT payload_hash,response FROM command_receipts WHERE operation_kind=$1 AND request_key=$2")
        .bind(kind).bind(key).fetch_optional(conn).await?;
    match row {
        Some((old, value)) if old == hash => Ok(Some(value)),
        Some(_) => Err(ApiError::conflict(
            "Idempotency key was used for different content",
        )),
        None => Ok(None),
    }
}
async fn save_receipt(
    conn: &mut sqlx::PgConnection,
    kind: &str,
    key: &str,
    hash: &str,
    response: &Value,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO command_receipts(operation_kind,request_key,payload_hash,response) VALUES ($1,$2,$3,$4)")
        .bind(kind).bind(key).bind(hash).bind(response).execute(conn).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClusterRequest {
    cluster_id: String,
    name: String,
    region: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostRequest {
    host_id: String,
    cluster_id: String,
    hardware_profile_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GpuRequest {
    host_id: String,
    gpu_index: u8,
    hardware_profile_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkloadRequest {
    name: String,
    profile_id: String,
    data_profile_id: String,
    purpose: String,
    replicas: u32,
    spread_across_domains: bool,
}

async fn create(
    State(app): State<App>,
    Path(kind): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    authorize(&app, &headers, false)?;
    let _guard = available(&app).await?;
    let key = header(&headers, "idempotency-key")?;
    let fingerprint = hash("command-v1", &(&kind, &body))?;
    let mut tx = app.config.begin().await?;
    db::lock(&mut tx).await?;
    if let Some(value) = receipt(&mut tx, &kind, key, &fingerprint).await? {
        return Ok((StatusCode::OK, Json(value)).into_response());
    }
    let parse = |error: serde_json::Error| ApiError::invalid(error.to_string());
    let value = match kind.as_str() {
        "clusters" => {
            let c: ClusterRequest = serde_json::from_value(body).map_err(parse)?;
            db::insert(
                &mut tx,
                Table::Clusters,
                &Cluster {
                    cluster_id: c.cluster_id,
                    name: c.name,
                    region: c.region,
                    revision: 1,
                },
            )
            .await?
        }
        "hosts" => {
            let h: HostRequest = serde_json::from_value(body).map_err(parse)?;
            if h.hardware_profile_id != "h100-nvl-pair-v1" {
                return Err(ApiError::invalid("Unknown hardware profile"));
            }
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT FROM gpu_inventory WHERE host_id=$1)")
                    .bind(&h.host_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if exists {
                return Err(ApiError::conflict("Worker already exists"));
            }
            let mut ids = Vec::new();
            for slot in 0..2 {
                let g = Gpu::new(&h.host_id, &h.cluster_id, slot);
                db::insert(&mut tx, Table::Gpus, &g).await?;
                db::insert(&mut tx, Table::Settings, &Settings::baseline(g.gpu_id)).await?;
                ids.push(g.gpu_id);
            }
            json!({"id":h.host_id,"gpu_ids":ids,"revision":"1"})
        }
        "gpus" => {
            let h: GpuRequest = serde_json::from_value(body).map_err(parse)?;
            if h.hardware_profile_id != "h100-nvl-pair-v1" {
                return Err(ApiError::invalid("Unknown hardware profile"));
            }
            let cluster: Option<String> =
                sqlx::query_scalar("SELECT cluster_id FROM gpu_inventory WHERE host_id=$1 LIMIT 1")
                    .bind(&h.host_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let cluster =
                cluster.ok_or_else(|| ApiError::invalid("Worker must have a registered slot"))?;
            let g = Gpu::new(&h.host_id, &cluster, h.gpu_index);
            let value = db::insert(&mut tx, Table::Gpus, &g).await?;
            db::insert(&mut tx, Table::Settings, &Settings::baseline(g.gpu_id)).await?;
            value
        }
        "workloads" => {
            let w: WorkloadRequest = serde_json::from_value(body).map_err(parse)?;
            let mut w = Workload::from_profile(
                &w.name,
                &w.profile_id,
                w.replicas,
                &w.data_profile_id,
                &w.purpose,
            )
            .map(|mut row| {
                row.spread_across_domains = w.spread_across_domains;
                row
            })
            .map_err(|e| ApiError::invalid(e.to_string()))?;
            w.revision = 1;
            db::insert(&mut tx, Table::Workloads, &w).await?
        }
        _ => return Err(ApiError::invalid("Unknown command collection")),
    };
    db::configuration(&mut tx)
        .await?
        .validate()
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    let identity = match kind.as_str() {
        "clusters" => &value["cluster_id"],
        "hosts" => &value["id"],
        "gpus" => &value["gpu_id"],
        "workloads" => &value["workload_id"],
        _ => unreachable!(),
    };
    let response =
        json!({"status":"committed","id":identity,"revision":"1","gpu_ids":value.get("gpu_ids")});
    save_receipt(&mut tx, &kind, key, &fingerprint, &response).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

fn table(kind: &str) -> ApiResult<Table> {
    match kind {
        "gpus" => Ok(Table::Gpus),
        "workloads" => Ok(Table::Workloads),
        "policies" => Ok(Table::Policies),
        "telemetry" => Ok(Table::Settings),
        _ => Err(ApiError::invalid("Unknown command collection")),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    #[serde(with = "decimal")]
    expected_revision: u64,
    changes: Value,
}

async fn update_row(
    conn: &mut sqlx::PgConnection,
    t: Table,
    id: &str,
    patch: Patch,
) -> ApiResult<Value> {
    let changes = patch
        .changes
        .as_object()
        .ok_or_else(|| ApiError::invalid("changes must be an object"))?;
    if changes.is_empty() || changes.keys().any(|k| !t.editable().contains(&k.as_str())) {
        return Err(ApiError::invalid("Unknown or immutable field"));
    }
    let sql = format!(
        "SELECT to_jsonb(t)-'updated_at' FROM {} t WHERE {}::text=$1",
        t.name(),
        t.key()
    );
    let mut value: Value = sqlx::query_scalar(&sql)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ApiError {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND",
            message: "Record not found".into(),
        })?;
    if value["revision"].as_u64() != Some(patch.expected_revision) {
        return Err(ApiError::conflict(
            "Revision changed; refresh the query view",
        ));
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| ApiError::invalid("Invalid stored row"))?;
    for (k, v) in changes {
        object.insert(k.clone(), v.clone());
    }
    if matches!(t, Table::Workloads) && changes.contains_key("profile_id") {
        let profile = object["profile_id"]
            .as_str()
            .ok_or_else(|| ApiError::invalid("Invalid profile"))?;
        let p = serving_profiles()
            .into_iter()
            .find(|p| p.profile_id == profile)
            .ok_or_else(|| ApiError::invalid("Unknown serving profile"))?;
        object.insert("model_ref".into(), json!(p.model_ref));
        object.insert("memory_mib_per_replica".into(), json!(p.memory_mib));
        object.insert("compute_units_per_replica".into(), json!(p.compute_units));
    }
    let columns = t
        .columns()
        .iter()
        .filter(|c| **c != t.key())
        .copied()
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "UPDATE {t} SET ({columns})=(SELECT {columns} FROM jsonb_populate_record(NULL::{t},$1))
        WHERE {key}::text=$2 RETURNING to_jsonb({t})-'updated_at'",
        t = t.name(),
        key = t.key()
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(value)
        .bind(id)
        .fetch_one(conn)
        .await?)
}
async fn update(
    State(app): State<App>,
    Path((kind, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(p): Json<Patch>,
) -> ApiResult<Json<Value>> {
    authorize(&app, &headers, false)?;
    let _guard = available(&app).await?;
    let mut tx = app.config.begin().await?;
    db::lock(&mut tx).await?;
    let value = update_row(&mut tx, table(&kind)?, &id, p).await?;
    db::configuration(&mut tx)
        .await?
        .validate()
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    tx.commit().await?;
    Ok(Json(
        json!({"status":"committed","id":id,"revision":value["revision"].to_string()}),
    ))
}
async fn telemetry(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    p: Json<Patch>,
) -> ApiResult<Json<Value>> {
    update(State(app), Path(("telemetry".into(), id)), headers, p).await
}
async fn remove(
    State(app): State<App>,
    Path((kind, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    authorize(&app, &headers, false)?;
    let _guard = available(&app).await?;
    if !["gpus", "workloads"].contains(&kind.as_str()) {
        return Err(ApiError::invalid("Deletion not supported"));
    }
    let revision: u64 = header(&headers, "if-match")?
        .trim_matches('"')
        .parse()
        .map_err(|_| ApiError::invalid("Invalid If-Match revision"))?;
    let t = table(&kind)?;
    let mut tx = app.config.begin().await?;
    db::lock(&mut tx).await?;
    let sql = format!(
        "DELETE FROM {} WHERE {}::text=$1 AND revision=$2",
        t.name(),
        t.key()
    );
    let changed = sqlx::query(&sql)
        .bind(id)
        .bind(i64::try_from(revision).map_err(|_| ApiError::invalid("Revision out of range"))?)
        .execute(&mut *tx)
        .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::conflict("Record absent or revision changed"));
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupPatch {
    gpu_revisions: BTreeMap<Uuid, String>,
    changes: Value,
}
async fn group_telemetry(
    State(app): State<App>,
    Path((kind, id)): Path<(String, String)>,
    h: HeaderMap,
    Json(p): Json<GroupPatch>,
) -> ApiResult<Json<Value>> {
    authorize(&app, &h, false)?;
    let _guard = available(&app).await?;
    let mut tx = app.config.begin().await?;
    db::lock(&mut tx).await?;
    let filter = match kind.as_str() {
        "hosts" => "g.host_id",
        "regions" => "c.region",
        _ => return Err(ApiError::invalid("Unknown telemetry group")),
    };
    let sql=format!("SELECT g.gpu_id FROM gpu_inventory g JOIN regional_clusters c USING(cluster_id) WHERE {filter}=$1 ORDER BY g.gpu_id");
    let ids: Vec<Uuid> = sqlx::query_scalar(&sql)
        .bind(&id)
        .fetch_all(&mut *tx)
        .await?;
    if ids.is_empty() || ids != p.gpu_revisions.keys().copied().collect::<Vec<_>>() {
        return Err(ApiError::conflict("GPU membership changed"));
    }
    for gpu in ids {
        let revision = p.gpu_revisions[&gpu]
            .parse()
            .map_err(|_| ApiError::invalid("Invalid GPU revision"))?;
        update_row(
            &mut tx,
            Table::Settings,
            &gpu.to_string(),
            Patch {
                expected_revision: revision,
                changes: p.changes.clone(),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id,"status":"committed"})))
}

async fn write_plan(
    State(app): State<App>,
    h: HeaderMap,
    Json(c): Json<Candidate>,
) -> ApiResult<Json<Value>> {
    authorize(&app, &h, true)?;
    let _guard = available(&app).await?;
    let payload_hash = hash("candidate-v1", &c)?;
    let mut tx = app.placements.begin().await?;
    db::lock(&mut tx).await?;
    let current = db::plan(&mut tx).await?;
    if let Some(mut old) =
        receipt(&mut tx, "plan", &c.decision_id.to_string(), &payload_hash).await?
    {
        old["status"] = json!("already_committed");
        old["current_plan_version"] = json!(current.plan_version.to_string());
        return Ok(Json(old));
    }
    let config = db::configuration(&mut tx).await?;
    if c.expected_plan_version != current.plan_version
        || c.config_fingerprint != config.fingerprint()?
    {
        return Err(ApiError::conflict("Plan or configuration changed"));
    }
    let policy = Evaluator::new()?.evaluate(&config)?;
    policy.validate_current(&config)?;
    if c.policy_signature != policy.policy_signature
        || c.policy_bundle_hash != policy.policy_bundle_hash
    {
        return Err(ApiError::conflict("Policy changed"));
    }
    policy
        .validate_plan(&config, &c.assignments)
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    let static_capacity = config
        .gpus
        .values()
        .map(|g| {
            (
                g.gpu_id,
                Capacity {
                    gpu_id: g.gpu_id,
                    eligible: g.scheduling_enabled,
                    memory_mib: g.memory_mib,
                    compute_units: PLANNING_UNITS,
                },
            )
        })
        .collect();
    validate_assignments(&config, &static_capacity, &c.assignments)
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    if serde_json::to_vec(&c.decision_details)
        .map_err(anyhow::Error::from)?
        .len()
        > 128 * 1024
    {
        return Err(ApiError::invalid("Explanation exceeds 128 KiB"));
    }
    let version = current
        .plan_version
        .checked_add(1)
        .context("plan version exhausted")?;
    let scenario = current
        .decision_details
        .get("scenario")
        .and_then(Value::as_str)
        .context("saved plan is missing its starting scenario")?;
    let mut decision_details = c.decision_details;
    decision_details
        .as_object_mut()
        .context("decision details must be an object")?
        .insert("scenario".into(), json!(scenario));
    let p = Plan {
        fleet_id: "demo".into(),
        plan_version: version,
        decision_id: c.decision_id,
        config_fingerprint: c.config_fingerprint,
        policy_signature: c.policy_signature,
        policy_bundle_hash: c.policy_bundle_hash,
        assignments: c.assignments,
        decision_details,
    };
    db::put_plan(&mut tx, &p).await?;
    let response = json!({"decision_id":p.decision_id,"plan_version":version.to_string(),"current_plan_version":version.to_string(),"status":"committed"});
    save_receipt(
        &mut tx,
        "plan",
        &p.decision_id.to_string(),
        &payload_hash,
        &response,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(response))
}

async fn reset(
    State(app): State<App>,
    Path(name): Path<String>,
    h: HeaderMap,
) -> ApiResult<Json<Value>> {
    authorize(&app, &h, false)?;
    let name = if name == "regional" {
        "regional-boundary".to_owned()
    } else {
        name
    };
    fixtures::load(&name).map_err(|e| ApiError::invalid(e.to_string()))?;
    let mut gate = tokio::time::timeout(Duration::from_secs(30), app.gate.write())
        .await
        .map_err(anyhow::Error::from)?;
    *gate = false;
    app.stream_resets.send_replace(());
    let outcome = async {
        let mut transaction = app.reset.begin().await?;
        db::set_reset_pending(&mut transaction, true).await?;
        transaction.commit().await?;
        app.client
            .post(format!("{}/internal/runtime/stop", app.drasi))
            .bearer_auth(&app.token)
            .timeout(Duration::from_secs(90))
            .send()
            .await?
            .error_for_status()?;
        let mut transaction = app.reset.begin().await?;
        let plan = db::load_fixture(&mut transaction, &name).await?;
        transaction.commit().await?;
        app.client
            .post(format!("{}/internal/runtime/start", app.drasi))
            .bearer_auth(&app.token)
            .timeout(Duration::from_secs(90))
            .json(&json!({"scenario":name}))
            .send()
            .await?
            .error_for_status()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let response = app
                .client
                .get(format!("{}/health/ready", app.drasi))
                .send()
                .await?;
            let status = response.status();
            let observed: Value = response.json().await?;
            if status.is_success() && observed["ready"] == true {
                anyhow::ensure!(
                    observed["scenario"] == name,
                    "runtime bootstrapped a different scenario"
                );
                let mut transaction = app.reset.begin().await?;
                db::set_reset_pending(&mut transaction, false).await?;
                transaction.commit().await?;
                return Ok::<_, anyhow::Error>(json!({
                    "status":"ready", "scenario":name, "plan_version":plan.plan_version.to_string(),
                    "observation_epoch":observed["observation_epoch"],
                }));
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "reset readiness timed out: {observed}"
            );
            anyhow::ensure!(
                observed["state"] != "error",
                "runtime failed during reset: {observed}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    .await;
    match outcome {
        Ok(value) => {
            *gate = true;
            Ok(Json(value))
        }
        Err(error) => {
            tracing::error!(error=%error, "reset failed; writes remain gated");
            Err(ApiError {
                status: StatusCode::SERVICE_UNAVAILABLE, code:"RESET_FAILED",
                message:"Reset did not reach observed readiness. Writes remain gated; inspect the logs and retry reset.".into(),
            })
        }
    }
}

async fn proxy(
    State(app): State<App>,
    Path(path): Path<String>,
    uri: axum::http::Uri,
) -> ApiResult<Response> {
    let _guard = available(&app).await?;
    let parts: Vec<_> = path.split('/').collect();
    let query = parts.first() == Some(&"queries")
        && parts.get(1).is_some_and(|id| UI_QUERIES.contains(id))
        && (parts.len() == 2 || (parts.len() == 3 && parts[2] == "results"));
    let reaction = path == "reactions/gpu-demo-ui";
    if !(query || reaction) || uri.query().is_some_and(|q| q != "view=full") {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND",
            message: "Read route not exposed".into(),
        });
    }
    let url = format!(
        "{}/api/v1/instances/gpu-demo/{}{}",
        app.drasi,
        path,
        uri.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    let response = app
        .client
        .get(url)
        .send()
        .await
        .map_err(anyhow::Error::from)?;
    let status = response.status();
    let data = response.bytes().await.map_err(anyhow::Error::from)?;
    Ok((
        status,
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
        ],
        data,
    )
        .into_response())
}
async fn events(State(app): State<App>) -> ApiResult<Response> {
    let _guard = available(&app).await?;
    let reset = app.stream_resets.subscribe();
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(anyhow::Error::from)?
        .get(&app.sse)
        .send()
        .await
        .map_err(anyhow::Error::from)?;
    if !response.status().is_success() {
        return Err(ApiError::from(anyhow::anyhow!(
            "SSE reaction returned {}",
            response.status()
        )));
    }
    Ok((
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-cache"),
            ("x-accel-buffering", "no"),
        ],
        Body::from_stream(
            response
                .bytes_stream()
                .take_until(stream_finished(app.shutdown.clone(), reset)),
        ),
    )
        .into_response())
}
async fn stream_finished(shutdown: watch::Receiver<bool>, mut reset: watch::Receiver<()>) {
    tokio::select! {
        _ = shutdown_requested(shutdown) => {}
        _ = reset.changed() => {}
    }
}
async fn shutdown_requested(mut shutdown: watch::Receiver<bool>) {
    while !*shutdown.borrow_and_update() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}
async fn ready(State(app): State<App>) -> ApiResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(&app.config).await?;
    let _guard = available(&app).await?;
    let response = app
        .client
        .get(format!("{}/health/ready", app.drasi))
        .send()
        .await
        .map_err(anyhow::Error::from)?;
    let status = response.status();
    let observation: Value = response.json().await.map_err(anyhow::Error::from)?;
    if !status.is_success() || observation["ready"] != true {
        return Err(ApiError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "GRAPH_NOT_READY",
            message: format!("The live graph is not ready: {observation}"),
        });
    }
    Ok(Json(observation))
}
fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("Missing {name}"))
}
async fn pool(name: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&env(name)?)
        .await?)
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    if std::env::args().nth(1).as_deref() == Some("add-data-choices") {
        let added = db::add_data_choices(&pool("RESET_DATABASE_URL").await?).await?;
        println!("{}", json!({ "added": added }));
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("init") {
        let owner = pool("DEMO_OWNER_URL").await?;
        db::initialize(&owner).await?;
        let password = env("RESET_PASSWORD")?;
        anyhow::ensure!(password.len() >= 32, "reset password is too short");
        let statement: String =
            sqlx::query_scalar("SELECT format('ALTER ROLE gpu_reset LOGIN PASSWORD %L', $1::text)")
                .bind(password)
                .fetch_one(&owner)
                .await?;
        sqlx::query(&statement).execute(&owner).await?;
        return Ok(());
    }
    let (shutdown, stopped) = watch::channel(false);
    let reset_pool = pool("RESET_DATABASE_URL").await?;
    let gate = restored_gate(&reset_pool).await?;
    let app = App {
        shutdown: stopped,
        stream_resets: watch::channel(()).0,
        config: pool("DATABASE_URL").await?,
        placements: pool("PLAN_DATABASE_URL").await?,
        reset: reset_pool,
        gate,
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()?,
        drasi: env("DRASI_API_URL")?,
        sse: env("DRASI_SSE_URL")?,
        origin: env("PUBLIC_ORIGIN")?,
        token: env("INTERNAL_TOKEN")?,
        csrf: Uuid::new_v4().to_string(),
    };
    let gate = app.gate.clone();
    let router = Router::new()
        .route(
            "/health/live",
            get(|| async { Json(json!({"status":"alive"})) }),
        )
        .route("/health/ready", get(ready))
        .route(
            "/api/csrf",
            get(|State(a): State<App>| async move { Json(json!({"token":a.csrf})) }),
        )
        .route("/api/:kind", post(create))
        .route("/api/:kind/:id", patch(update).delete(remove))
        .route("/api/gpus/:id/telemetry", patch(telemetry))
        .route("/api/:kind/:id/telemetry", patch(group_telemetry))
        .route("/api/demo/presets/:name", post(reset))
        .route("/internal/placement-plans", post(write_plan))
        .route("/api/v1/instances/gpu-demo/*path", get(proxy))
        .route("/events/gpu-demo", get(events))
        .fallback_service(ServeDir::new("ui/dist"))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(env("LISTEN_ADDRESS")?).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            if let Err(e) = gpu_control::shutdown_signal().await {
                tracing::error!(error=%e,"shutdown signal failed");
            }
            shutdown.send_replace(true);
            *gate.write().await = false;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sse_shutdown_observes_existing_and_future_requests() {
        let (shutdown, stopped) = watch::channel(false);
        let pending = tokio::spawn(shutdown_requested(stopped.clone()));
        shutdown.send_replace(true);
        tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), shutdown_requested(stopped))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn sse_reset_closes_only_the_previous_stream_generation() {
        let (_shutdown, stopped) = watch::channel(false);
        let (reset, previous) = watch::channel(());
        reset.send_replace(());
        tokio::time::timeout(
            Duration::from_secs(1),
            stream_finished(stopped.clone(), previous),
        )
        .await
        .unwrap();
        let next = tokio::spawn(stream_finished(stopped, reset.subscribe()));
        tokio::task::yield_now().await;
        assert!(!next.is_finished());
        reset.send_replace(());
        tokio::time::timeout(Duration::from_secs(1), next)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires dedicated gpu_demo_test database and DEMO_TEST_* URLs"]
    async fn postgres_commands_revisions_roles_and_plan_receipts() -> Result<()> {
        let owner = pool("DEMO_TEST_OWNER_URL").await?;
        let name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&owner)
            .await?;
        anyhow::ensure!(
            name == "gpu_demo_test",
            "refusing to run destructive integration setup on another database"
        );
        db::initialize(&owner).await?;
        let config_pool = pool("DEMO_TEST_CONFIG_URL").await?;
        let plan_pool = pool("DEMO_TEST_PLAN_URL").await?;
        let catalog_pool = pool("DEMO_TEST_RESET_URL").await?;
        let (_shutdown, stopped) = watch::channel(false);
        let app = App {
            shutdown: stopped,
            stream_resets: watch::channel(()).0,
            config: config_pool.clone(),
            placements: plan_pool.clone(),
            reset: owner.clone(),
            gate: Arc::new(RwLock::new(true)),
            client: reqwest::Client::new(),
            drasi: "http://127.0.0.1:1".into(),
            sse: "http://127.0.0.1:1/events".into(),
            origin: "http://localhost:5400".into(),
            token: "test-internal-token".into(),
            csrf: "test-csrf".into(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-internal-token".parse()?);
        headers.insert("idempotency-key", Uuid::new_v4().to_string().parse()?);
        let mut tx = owner.begin().await?;
        db::load_fixture(&mut tx, "baseline").await?;
        tx.commit().await?;
        {
            let mut conn = owner.acquire().await?;
            assert!(db::add_data_choices(&catalog_pool).await?.is_empty());
            sqlx::query("DELETE FROM data_profiles WHERE data_profile_id='customer-eu-documents'")
                .execute(&mut *conn)
                .await?;
            sqlx::query("DELETE FROM placement_policies WHERE policy_id='customer-eu-processing'")
                .execute(&mut *conn)
                .await?;
            sqlx::query(r#"UPDATE placement_policies SET allowed_regions='["westeurope"]'::jsonb WHERE policy_id='demo-permissive'"#)
                .execute(&mut *conn).await?;
            let mut expected = db::configuration(&mut conn).await?;
            let plan = serde_json::to_value(db::plan(&mut conn).await?)?;
            let receipts_sql = "SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY operation_kind,request_key),'[]') FROM command_receipts r";
            let receipts: Value = sqlx::query_scalar(receipts_sql)
                .fetch_one(&mut *conn)
                .await?;
            assert_eq!(
                db::add_data_choices(&catalog_pool).await?,
                vec![
                    "policy/customer-eu-processing",
                    "data/customer-eu-documents"
                ]
            );
            let catalog = fixtures::data_catalog();
            expected.policies.insert(
                "customer-eu-processing".into(),
                catalog.policies["customer-eu-processing"].clone(),
            );
            expected.data_profiles.insert(
                "customer-eu-documents".into(),
                catalog.data_profiles["customer-eu-documents"].clone(),
            );
            assert_eq!(db::configuration(&mut conn).await?, expected);
            assert!(db::add_data_choices(&catalog_pool).await?.is_empty());
            assert_eq!(serde_json::to_value(db::plan(&mut conn).await?)?, plan);
            assert_eq!(
                sqlx::query_scalar::<_, Value>(receipts_sql)
                    .fetch_one(&mut *conn)
                    .await?,
                receipts
            );
            let mut tx = owner.begin().await?;
            db::set_reset_pending(&mut tx, true).await?;
            tx.commit().await?;
            assert!(db::add_data_choices(&catalog_pool)
                .await
                .unwrap_err()
                .to_string()
                .contains("reset is pending"));
            let mut tx = owner.begin().await?;
            db::set_reset_pending(&mut tx, false).await?;
            tx.commit().await?;
            sqlx::query("DELETE FROM data_profiles WHERE data_profile_id='customer-eu-documents'")
                .execute(&mut *conn)
                .await?;
            sqlx::query("DELETE FROM placement_policies WHERE policy_id='customer-eu-processing'")
                .execute(&mut *conn)
                .await?;
            let mut conflicting = catalog.data_profiles["demo-open"].clone();
            conflicting.data_profile_id = "customer-eu-documents".into();
            db::insert(&mut conn, Table::Data, &conflicting).await?;
            let before_conflict = db::configuration(&mut conn).await?;
            assert!(db::add_data_choices(&catalog_pool)
                .await
                .unwrap_err()
                .to_string()
                .contains("different facts"));
            assert_eq!(
                db::configuration(&mut conn).await?,
                before_conflict,
                "failed additions roll back together"
            );
            let mut tx = owner.begin().await?;
            db::load_fixture(&mut tx, "baseline").await?;
            tx.commit().await?;
        }
        assert!(*restored_gate(&owner).await?.read().await);
        let mut tx = owner.begin().await?;
        db::set_reset_pending(&mut tx, true).await?;
        tx.commit().await?;
        let recovered = App {
            gate: restored_gate(&owner).await?,
            ..app.clone()
        };
        assert!(available(&recovered).await.is_err());
        assert!(sqlx::query("UPDATE demo_reset_state SET pending=false")
            .execute(&config_pool)
            .await
            .is_err());
        assert!(sqlx::query("UPDATE demo_reset_state SET pending=false")
            .execute(&plan_pool)
            .await
            .is_err());
        let mut tx = owner.begin().await?;
        db::set_reset_pending(&mut tx, false).await?;
        tx.commit().await?;
        assert!(*restored_gate(&owner).await?.read().await);
        let mut conn = owner.acquire().await?;
        let config = db::configuration(&mut conn).await?;
        assert_eq!(config.gpus.len(), 6);
        assert_eq!(
            db::rows::<Settings>(&mut conn, Table::Settings)
                .await?
                .len(),
            6
        );
        let original = db::plan(&mut conn).await?;
        let gpu = config.gpus.values().next().unwrap();
        let before: Value =
            sqlx::query_scalar("SELECT to_jsonb(g) FROM gpu_inventory g WHERE gpu_id=$1")
                .bind(gpu.gpu_id)
                .fetch_one(&mut *conn)
                .await?;
        sqlx::query(
            "UPDATE gpu_inventory SET name=name,revision=999,updated_at=now() WHERE gpu_id=$1",
        )
        .bind(gpu.gpu_id)
        .execute(&mut *conn)
        .await?;
        let after: Value =
            sqlx::query_scalar("SELECT to_jsonb(g) FROM gpu_inventory g WHERE gpu_id=$1")
                .bind(gpu.gpu_id)
                .fetch_one(&mut *conn)
                .await?;
        assert_eq!(
            before, after,
            "no-op updates must not bump revisions or timestamps"
        );
        assert!(
            sqlx::query("UPDATE gpu_inventory SET host_id='renamed' WHERE gpu_id=$1")
                .bind(gpu.gpu_id)
                .execute(&mut *conn)
                .await
                .is_err()
        );
        assert!(sqlx::query("UPDATE regional_clusters SET region='eastus'")
            .execute(&mut *conn)
            .await
            .is_err());
        assert!(
            sqlx::query("UPDATE gpu_placements SET plan_version=plan_version+1")
                .execute(&config_pool)
                .await
                .is_err()
        );
        assert!(sqlx::query("DELETE FROM gpu_inventory")
            .execute(&plan_pool)
            .await
            .is_err());

        let request = json!({"host_id":"inference-d","cluster_id":"eu-primary","hardware_profile_id":"h100-nvl-pair-v1"});
        let (first, retry) = tokio::join!(
            create(
                State(app.clone()),
                Path("hosts".into()),
                headers.clone(),
                Json(request.clone())
            ),
            create(
                State(app.clone()),
                Path("hosts".into()),
                headers.clone(),
                Json(request)
            )
        );
        let a = first?;
        let b = retry?;
        assert!(a.status().is_success() && b.status().is_success());
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM gpu_inventory WHERE host_id='inference-d'")
                .fetch_one(&owner)
                .await?;
        assert_eq!(
            count, 2,
            "concurrent retries must not duplicate the ready worker"
        );
        let changed=create(State(app.clone()),Path("hosts".into()),headers.clone(),
            Json(json!({"host_id":"different","cluster_id":"eu-primary","hardware_profile_id":"h100-nvl-pair-v1"}))).await.unwrap_err();
        assert_eq!(changed.status, StatusCode::CONFLICT);

        let patch = || Patch {
            expected_revision: 1,
            changes: json!({"reporting_enabled":false}),
        };
        let (a, b) = tokio::join!(
            telemetry(
                State(app.clone()),
                Path(gpu.gpu_id.to_string()),
                headers.clone(),
                Json(patch())
            ),
            telemetry(
                State(app.clone()),
                Path(gpu.gpu_id.to_string()),
                headers.clone(),
                Json(patch())
            )
        );
        assert_eq!(
            usize::from(a.is_ok()) + usize::from(b.is_ok()),
            1,
            "one stale concurrent patch must be rejected"
        );
        let settings = db::rows::<Settings>(&mut conn, Table::Settings).await?;
        let changed = settings.iter().find(|s| s.gpu_id == gpu.gpu_id).unwrap();
        assert_eq!(changed.revision, 2);
        assert!(!changed.reporting_enabled);
        let before_group = settings.clone();
        let partial = GroupPatch {
            gpu_revisions: BTreeMap::from([(gpu.gpu_id, "2".into())]),
            changes: json!({"powered_on":false}),
        };
        let error = group_telemetry(
            State(app.clone()),
            Path(("hosts".into(), gpu.host_id.clone())),
            headers.clone(),
            Json(partial),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(
            before_group,
            db::rows::<Settings>(&mut conn, Table::Settings).await?
        );

        let config = db::configuration(&mut conn).await?;
        let policy = Evaluator::new()?.evaluate(&config)?;
        let candidate = Candidate {
            decision_id: Uuid::new_v4(),
            expected_plan_version: original.plan_version,
            config_fingerprint: config.fingerprint()?,
            policy_signature: policy.policy_signature.clone(),
            policy_bundle_hash: policy.policy_bundle_hash.clone(),
            scheduling_signature: "test-observation".into(),
            assignments: original.assignments.clone(),
            decision_details: json!({"reason_codes":["test-complete-plan"]}),
        };
        let mut invalid = candidate.clone();
        invalid.assignments.pop();
        let error = write_plan(State(app.clone()), headers.clone(), Json(invalid))
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            db::plan(&mut conn).await?.plan_version,
            original.plan_version
        );
        let committed = write_plan(State(app.clone()), headers.clone(), Json(candidate.clone()))
            .await?
            .0;
        let repeated = write_plan(State(app.clone()), headers.clone(), Json(candidate.clone()))
            .await?
            .0;
        assert_eq!(committed["plan_version"], repeated["plan_version"]);
        assert_eq!(repeated["status"], "already_committed");
        let mut reused = candidate.clone();
        reused.decision_details = json!({"modified":true});
        assert_eq!(
            write_plan(State(app.clone()), headers.clone(), Json(reused))
                .await
                .unwrap_err()
                .status,
            StatusCode::CONFLICT
        );

        let mut tx = owner.begin().await?;
        db::load_fixture(&mut tx, "regional-boundary").await?;
        tx.commit().await?;
        let replayed = write_plan(State(app.clone()), headers.clone(), Json(candidate))
            .await?
            .0;
        assert_eq!(replayed["plan_version"], committed["plan_version"]);
        assert_ne!(replayed["current_plan_version"], replayed["plan_version"]);
        assert_eq!(
            db::configuration(&mut conn).await?.gpus.len(),
            14,
            "old plan receipt must not restore old fixture"
        );
        let before_reset = db::plan(&mut conn).await?.plan_version;
        assert_eq!(
            reset(State(app), Path("baseline".into()), headers)
                .await
                .unwrap_err()
                .status,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(db::plan(&mut conn).await?.plan_version, before_reset);
        Ok(())
    }
}
