use anyhow::{ensure, Context, Result};
use drasi_core::{
    computation::{ComputationIndexProvider, InMemoryComputationProvider},
    models::{Element, ElementValue, SourceChange},
};
use drasi_lib::{computation::v1::*, DrasiLib};
use gpu_native::{inputs, projections, wire::Emitter};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader},
    num::NonZeroUsize,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_FRAMES: usize = 4096;

struct Frame {
    envelope: ChangeEnvelope,
    observed_at_ms: u64,
}
struct Corpus {
    frames: Vec<Frame>,
    configuration: Value,
    plan: Value,
    workload_counts: BTreeSet<usize>,
    workload_transitions: Vec<usize>,
    plan_versions: BTreeSet<String>,
    report_batches: usize,
}

fn snapshot_changes(source: &mut Emitter, snapshot: &Value) -> Result<Vec<SourceChange>> {
    let mut changes = Vec::new();
    for (label, object) in [
        ("regional_clusters", &snapshot["configuration"]["clusters"]),
        ("gpu_inventory", &snapshot["configuration"]["gpus"]),
        (
            "workload_requirements",
            &snapshot["configuration"]["workloads"],
        ),
        ("placement_policies", &snapshot["configuration"]["policies"]),
        ("data_profiles", &snapshot["configuration"]["data_profiles"]),
        ("gpu_telemetry", &snapshot["settings"]),
    ] {
        let object = object
            .as_object()
            .with_context(|| format!("missing {label} snapshot"))?;
        let keys = object.keys().cloned().collect();
        changes.extend(source.retain(label, &keys)?);
        for (key, value) in object {
            changes.extend(source.record(label, key, value)?);
        }
    }
    ensure!(
        snapshot["plan"].is_object(),
        "missing authoritative saved plan"
    );
    changes.extend(source.record("gpu_placements", "demo", &snapshot["plan"])?);
    Ok(changes)
}

fn load_corpus(path: &Path) -> Result<Corpus> {
    ensure!(
        path.metadata()?.len() <= 256 * 1024 * 1024,
        "corpus exceeds recorder bound"
    );
    let mut codec = EnvelopeCodec::new(NonZeroUsize::new(8 * 1024 * 1024).unwrap());
    codec.register_schema(GraphChangeCodec::schema())?;
    codec.register_schema(QueryChangeCodec::schema())?;
    let mut database = Emitter::new(StreamId::try_new("isolation/database")?);
    let mut corpus = Corpus {
        frames: Vec::new(),
        configuration: Value::Null,
        plan: Value::Null,
        workload_counts: BTreeSet::new(),
        workload_transitions: Vec::new(),
        plan_versions: BTreeSet::new(),
        report_batches: 0,
    };
    let mut sequences = BTreeMap::new();
    'records: for line in BufReader::new(File::open(path)?).lines() {
        let record: Value = serde_json::from_str(&line?)?;
        if record["port"] != "graph" {
            continue;
        }
        let observed_at_ms = record["observed_at_ms"]
            .as_u64()
            .context("missing observed time")?;
        let envelope = codec.decode(&serde_json::to_vec(&record["frame"])?)?;
        let stream = envelope.system().stream().to_string();
        let sequence = envelope.system().sequence();
        ensure!(
            sequences.get(&stream).is_none_or(|old| sequence > *old),
            "non-advancing corpus stream"
        );
        sequences.insert(stream, sequence);
        let native = GraphChangeCodec::decode_changes(&envelope)?;
        let mut has_reports = false;
        for change in &native {
            let (SourceChange::Insert { element } | SourceChange::Update { element }) = change
            else {
                continue;
            };
            let metadata = element.get_metadata();
            if metadata
                .labels
                .iter()
                .any(|label| label.as_ref() == "GpuSample")
            {
                has_reports = true;
            }
            if !metadata
                .labels
                .iter()
                .any(|label| label.as_ref() == "FleetConfiguration")
            {
                continue;
            }
            let snapshot = Value::Object(element.get_properties().into());
            if snapshot["settings"]
                .as_object()
                .context("missing settings snapshot")?
                .values()
                .any(|setting| setting["reporting_enabled"] == false)
            {
                break 'records;
            }
            let mut changes = snapshot_changes(&mut database, &snapshot)?;
            for change in &mut changes {
                match change {
                    SourceChange::Insert {
                        element: Element::Node { metadata: m, .. },
                    }
                    | SourceChange::Update {
                        element: Element::Node { metadata: m, .. },
                    }
                    | SourceChange::Delete { metadata: m } => {
                        m.effective_from = metadata.effective_from
                    }
                    _ => anyhow::bail!("unexpected database fixture change"),
                }
            }
            corpus.configuration = snapshot["configuration"].clone();
            corpus.plan = snapshot["plan"].clone();
            let count = corpus.configuration["workloads"]
                .as_object()
                .context("workloads missing")?
                .len();
            corpus.workload_counts.insert(count);
            if corpus.workload_transitions.last() != Some(&count) {
                corpus.workload_transitions.push(count);
            }
            corpus
                .plan_versions
                .insert(corpus.plan["plan_version"].to_string());
            for output in database.emit(changes, None)? {
                corpus.frames.push(Frame {
                    envelope: output.envelope,
                    observed_at_ms,
                });
            }
        }
        corpus.report_batches += usize::from(has_reports);
        corpus.frames.push(Frame {
            envelope,
            observed_at_ms,
        });
        ensure!(corpus.frames.len() <= MAX_FRAMES, "too many input frames");
    }
    ensure!(
        corpus.configuration.is_object(),
        "recording has no complete authoritative configuration"
    );
    ensure!(
        corpus.workload_counts.len() > 1,
        "recording lacks workload creation/deletion"
    );
    ensure!(
        corpus.workload_transitions.len() >= 3
            && corpus.workload_transitions.first() == corpus.workload_transitions.last(),
        "recording must return to its original workload count after creation/deletion"
    );
    ensure!(
        corpus.plan_versions.len() >= 2,
        "recording lacks saved-plan changes"
    );
    ensure!(
        corpus.report_batches >= 5,
        "recording lacks stable report batches"
    );
    Ok(corpus)
}

#[derive(Default)]
struct Times {
    ns: Vec<u128>,
    output_envelopes: usize,
    output_operations: usize,
    input_operations: usize,
}
impl Times {
    fn record(&mut self, elapsed: Duration, output: &[OutputEnvelope], inputs: usize) {
        self.ns.push(elapsed.as_nanos());
        self.input_operations += inputs;
        self.output_envelopes += output.len();
        self.output_operations += output
            .iter()
            .map(|o| o.envelope.changes().operations().len())
            .sum::<usize>();
    }
    fn report(&self) -> Value {
        let mut sorted = self.ns.clone();
        sorted.sort_unstable();
        let percentile = |numerator: usize| {
            sorted
                .get((sorted.len() * numerator).div_ceil(100).saturating_sub(1))
                .copied()
        };
        json!({
            "calls":self.ns.len(), "input_operations":self.input_operations,
            "output_envelopes":self.output_envelopes, "output_operations":self.output_operations,
            "total_ns":self.ns.iter().sum::<u128>().to_string(),
            "last_ns":self.ns.last().map(u128::to_string),
            "max_ns":sorted.last().map(u128::to_string),
            "p50_ns":percentile(50).map(|n|n.to_string()),
            "p95_ns":percentile(95).map(|n|n.to_string()),
        })
    }
}

fn cpu_ticks() -> Result<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat")
        .context("CPU accounting requires Linux /proc")?;
    let fields = stat
        .rsplit_once(") ")
        .context("invalid process stat")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let user: u64 = fields.get(11).context("missing user CPU")?.parse()?;
    let system: u64 = fields.get(12).context("missing system CPU")?.parse()?;
    user.checked_add(system).context("CPU counter overflow")
}

fn rows(query: &ContinuousQueryTransformer) -> Result<Vec<Value>> {
    query
        .results()
        .snapshot()?
        .rows
        .values()
        .map(|image| {
            let row = QueryChangeCodec::decode_row(image)?;
            Ok(QueryChangeCodec::row_values_to_json(&row.values))
        })
        .collect()
}

fn validate(id: &str, rows: &[Value], corpus: &Corpus, expired: bool) -> Result<()> {
    let gpus = corpus.configuration["gpus"]
        .as_object()
        .context("missing inventory")?;
    let workloads = corpus.configuration["workloads"]
        .as_object()
        .context("missing workload snapshot")?;
    let unique = |field: &str| -> Result<()> {
        let ids = rows
            .iter()
            .map(|row| {
                row[field]
                    .as_str()
                    .with_context(|| format!("{id}: missing {field}"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(ids.len() == rows.len(), "{id}: duplicate {field}");
        Ok(())
    };
    match id {
        "ui-gpus" => {
            unique("gpu_id")?;
            ensure!(
                rows.len() == gpus.len(),
                "GPU row count differs from authoritative inventory"
            );
            ensure!(
                rows.iter()
                    .all(|r| gpus.contains_key(r["gpu_id"].as_str().unwrap())),
                "unknown GPU"
            );
            if expired {
                ensure!(
                    rows.iter().all(|r| r["health"] == "unreachable"),
                    "due timers did not expire reports"
                );
            } else {
                ensure!(
                    rows.iter().all(|r| r["health"] == "healthy"),
                    "pre-pause corpus must finish with fresh reports"
                );
            }
        }
        "ui-workloads" => {
            unique("workload_id")?;
            ensure!(
                rows.len() == workloads.len(),
                "workload deletion left retained rows"
            );
            for row in rows {
                let configured = workloads
                    .get(row["workload_id"].as_str().unwrap())
                    .context("deleted workload remains")?;
                ensure!(
                    row["replicas"] == configured["replicas"],
                    "replica requirement changed"
                );
                for field in [
                    "ready_replicas",
                    "running_replicas",
                    "suspended_replicas",
                    "fenced_replicas",
                ] {
                    if !row[field].is_null() {
                        let count = row[field]
                            .as_f64()
                            .with_context(|| format!("invalid {field}"))?;
                        ensure!(
                            count >= 0.0
                                && count.fract() == 0.0
                                && count <= configured["replicas"].as_f64().unwrap(),
                            "invalid {field}: {count}"
                        );
                    }
                }
                if expired {
                    ensure!(
                        row["ready_replicas"].is_null()
                            || row["ready_replicas"].as_f64() == Some(0.0),
                        "expired report still confirms work"
                    );
                }
            }
        }
        "ui-placements" => {
            unique("fleet_id")?;
            ensure!(rows.len() == 1, "missing fleet plan");
            let expected = corpus.plan["plan_version"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| corpus.plan["plan_version"].to_string());
            ensure!(
                rows[0]["desired_plan_version"] == expected,
                "retained saved-plan version"
            );
            ensure!(
                rows[0]["desired"]
                    .as_array()
                    .context("missing desired assignments")?
                    .len()
                    == corpus.plan["assignments"]
                        .as_array()
                        .context("missing saved assignments")?
                        .len(),
                "partial saved plan"
            );
            if expired {
                ensure!(
                    rows[0]["status"] != "confirmed",
                    "expired fleet shown confirmed"
                );
            }
        }
        "ui-clusters" => {
            unique("cluster_id")?;
            ensure!(
                rows.len()
                    == corpus.configuration["clusters"]
                        .as_object()
                        .context("clusters missing")?
                        .len(),
                "cluster count differs"
            );
        }
        "ui-status" => {
            unique("fleet_id")?;
            ensure!(rows.len() == 1, "missing readiness");
        }
        "ui-policy" => {
            unique("id")?;
            ensure!(
                rows.len()
                    == workloads.len()
                        * corpus.configuration["clusters"]
                            .as_object()
                            .context("clusters missing")?
                            .len(),
                "incomplete policy pairs"
            );
        }
        "ui-decisions" => unique("decision_id")?,
        "ui-timeline" => {
            unique("event_id")?;
            ensure!(
                !rows.is_empty() && rows.len() <= 128,
                "invalid timeline count"
            );
        }
        "ui-resilience" => {
            unique("fleet_id")?;
            ensure!(rows.len() <= 1, "duplicate resilience result");
        }
        "simulation-inputs" => {
            ensure!(
                rows.len() == 1 && rows[0]["snapshot"]["configuration"] == corpus.configuration,
                "simulation retained old configuration"
            );
        }
        "runtime-context" => {
            ensure!(rows.len() == 1, "missing runtime context");
            let required: u64 = workloads
                .values()
                .map(|w| w["replicas"].as_u64().unwrap())
                .sum();
            ensure!(
                rows[0]["required_replicas"].as_u64() == Some(required),
                "stale required replica count"
            );
        }
        "scheduling-inputs" => {
            let current = rows
                .iter()
                .filter(|r| {
                    r["configuration"] == corpus.configuration
                        && r["capacities"].as_array().is_some_and(|c| !c.is_empty())
                })
                .collect::<Vec<_>>();
            ensure!(
                current.len() == 1,
                "expected one current nonempty scheduling aggregate"
            );
            let capacities = current[0]["capacities"].as_array().unwrap();
            let ids = capacities
                .iter()
                .map(|c| c["gpu_id"].as_str())
                .collect::<BTreeSet<_>>();
            ensure!(
                capacities.len() == gpus.len() && ids.len() == gpus.len() && !ids.contains(&None),
                "duplicate/missing capacities"
            );
            if expired {
                ensure!(
                    capacities.iter().all(|c| c["fresh"] == false),
                    "capacity remains fresh after due timers"
                );
            } else {
                ensure!(
                    capacities.iter().all(|c| c["fresh"] == true),
                    "pre-pause capacity is not fresh"
                );
            }
        }
        "plan-output" => {
            ensure!(rows.len() <= 1, "multiple candidate plans");
        }
        id if inputs::DATABASE_QUERIES.contains(&id) => {
            ensure!(rows.len() == 1, "{id}: expected one database aggregate");
            let records = rows[0]["records"]
                .as_array()
                .context("database records missing")?;
            let (key, expected) = match id {
                "input-clusters" => (
                    "cluster_id",
                    corpus.configuration["clusters"]
                        .as_object()
                        .context("clusters missing")?
                        .keys()
                        .cloned()
                        .collect::<BTreeSet<_>>(),
                ),
                "input-policies" => (
                    "policy_id",
                    corpus.configuration["policies"]
                        .as_object()
                        .context("policies missing")?
                        .keys()
                        .cloned()
                        .collect(),
                ),
                "input-data" => (
                    "data_profile_id",
                    corpus.configuration["data_profiles"]
                        .as_object()
                        .context("data profiles missing")?
                        .keys()
                        .cloned()
                        .collect(),
                ),
                "input-gpus" | "input-settings" => ("gpu_id", gpus.keys().cloned().collect()),
                "input-workloads" => ("workload_id", workloads.keys().cloned().collect()),
                "input-plan" => ("fleet_id", BTreeSet::from(["demo".to_owned()])),
                _ => unreachable!(),
            };
            let actual = records
                .iter()
                .map(|r| {
                    r[key]
                        .as_str()
                        .map(str::to_owned)
                        .context("missing database identity")
                })
                .collect::<Result<BTreeSet<_>>>()?;
            ensure!(
                actual == expected && records.len() == expected.len(),
                "{id}: incorrect database identities/counts"
            );
        }
        _ => anyhow::bail!("unknown query {id}"),
    }
    Ok(())
}

struct Measured {
    id: String,
    query: ContinuousQueryTransformer,
    input: Times,
    timers: Times,
    empty_timer_batches_with_pending: usize,
    cpu_ticks: u64,
}

fn accepts(id: &str, frame: &Frame) -> bool {
    let stream = frame.envelope.system().stream().as_str();
    if stream == "isolation/database" {
        return matches!(
            id,
            "ui-gpus" | "ui-clusters" | "ui-workloads" | "ui-placements" | "scheduling-inputs"
        ) || inputs::DATABASE_QUERIES.contains(&id);
    }
    if inputs::DATABASE_QUERIES.contains(&id) {
        return false;
    }
    match id {
        "simulation-inputs" => stream == "policy/out",
        "scheduling-inputs" => matches!(stream, "policy/out" | "simulator/out"),
        "plan-output" | "runtime-context" => stream == "placement/out",
        _ => true,
    }
}

fn shifted_time(value: u64, shift: i64) -> Result<u64> {
    u64::try_from(i128::from(value) + i128::from(shift)).context("rebased time out of range")
}

fn rebase_properties(value: &mut Value, shift: i64) -> Result<()> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "time_ms" | "report_time_ms" | "acknowledged_at_ms"
                ) && !value.is_null()
                {
                    *value =
                        shifted_time(value.as_u64().context("invalid UTC time")?, shift)?.into();
                } else if key == "payload" && value.is_string() {
                    let mut payload: Value = serde_json::from_str(value.as_str().unwrap())?;
                    rebase_properties(&mut payload, shift)?;
                    *value = serde_json::to_string(&payload)?.into();
                } else {
                    rebase_properties(value, shift)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                rebase_properties(value, shift)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn rebase_envelope(frame: &Frame, shift: i64) -> Result<ChangeEnvelope> {
    let mut changes = GraphChangeCodec::decode_changes(&frame.envelope)?;
    for change in &mut changes {
        match change {
            SourceChange::Insert { element } | SourceChange::Update { element } => {
                let (metadata, properties) = match element {
                    Element::Node {
                        metadata,
                        properties,
                    }
                    | Element::Relation {
                        metadata,
                        properties,
                        ..
                    } => (metadata, properties),
                };
                metadata.effective_from = shifted_time(metadata.effective_from, shift)?;
                let mut value = Value::Object((&*properties).into());
                rebase_properties(&mut value, shift)?;
                for (key, value) in value.as_object().context("properties are not an object")? {
                    properties.insert(key.as_str(), ElementValue::from(value));
                }
            }
            SourceChange::Delete { metadata } => {
                metadata.effective_from = shifted_time(metadata.effective_from, shift)?;
            }
            SourceChange::Future { .. } => anyhow::bail!("unexpected future in native recording"),
        }
    }
    Ok(GraphChangeCodec::encode_changes(
        &changes,
        frame.envelope.system().stream().clone(),
        frame.envelope.system().sequence(),
        Some(
            chrono::DateTime::from_timestamp_millis(i64::try_from(shifted_time(
                frame.observed_at_ms,
                shift,
            )?)?)
            .context("rebased envelope timestamp out of range")?,
        ),
    )?)
}

async fn execute(
    path: &Path,
    mode: &str,
    selection: &str,
    output: &Path,
    paced: bool,
) -> Result<()> {
    ensure!(
        matches!(mode, "bare" | "pipeline-memory"),
        "provider must be bare or pipeline-memory"
    );
    let loading = Instant::now();
    let corpus = load_corpus(path)?;
    let corpus_load_ns = loading.elapsed().as_nanos();
    let mut definitions = projections::definitions()
        .into_iter()
        .chain(inputs::processing_queries())
        .map(|(id, text, settings)| (id.to_owned(), text.to_owned(), settings))
        .collect::<Vec<_>>();
    definitions.extend(
        inputs::database_queries()
            .into_iter()
            .map(|(id, text)| (id.to_owned(), text, QueryExecutionSettings::default())),
    );
    definitions.retain(|(id, _, _)| selection == "all" || id == selection);
    ensure!(!definitions.is_empty(), "unknown query selection");
    let clock = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()?;
    ensure!(clock.status.success(), "getconf CLK_TCK failed");
    let ticks_per_second: u64 = std::str::from_utf8(&clock.stdout)?.trim().parse()?;
    ensure!(ticks_per_second > 0, "invalid CPU clock rate");
    let load = std::fs::read_to_string("/proc/loadavg")?;
    let (source, _handle) = drasi_source_application::ApplicationSource::new(
        "fixture",
        drasi_source_application::ApplicationSourceConfig {
            properties: Default::default(),
            durability: None,
        },
    )?;
    let core = DrasiLib::builder()
        .with_id("query-isolation")
        .with_source(source)
        .build()
        .await?;
    let mut batches = Vec::new();
    let mut measured = Vec::new();
    let construction = Instant::now();
    for (id, text, settings) in definitions {
        let provider: Arc<dyn ComputationIndexProvider> = if mode == "bare" {
            Arc::new(InMemoryComputationProvider)
        } else {
            let config = serde_json::from_value(json!({
                "id":id, "query":text, "sources":[{"source_id":"fixture"}], "joins":settings.joins,
                "enableBootstrap":false, "auto_start":false,
            }))?;
            let batch = core
                .computation_pipeline()?
                .source(
                    core.borrow_computation_source("fixture").await?,
                    SourceSubscriptionOptions::default(),
                )?
                .query(config)
                .build()?;
            let provider = batch
                .bindings
                .resources
                .values()
                .find(|resource| resource.role() == ResourceRole::IndexBackend)
                .context("pipeline index provider missing")?
                .get::<QueryIndexProviderResource>()?
                .0
                .clone();
            batches.push(batch);
            provider
        };
        ensure!(
            provider.is_volatile(),
            "persistent query provider is forbidden in this diagnostic"
        );
        let mut query = ContinuousQueryTransformer::new_configured(
            ContinuousQueryDefinition {
                graph_id: "query-isolation".into(),
                id: ComponentId::try_new(id.clone())?,
                query: text,
                language: ComputationQueryLanguage::Cypher,
                output_stream: StreamId::try_new(format!("{id}/out"))?,
                outbox_capacity: NonZeroUsize::new(256).unwrap(),
            },
            provider,
            QueryOptions {
                recovery: QueryRecoveryPolicy::Strict,
                publication: QueryPublicationMode::NonAtomic,
            },
            settings,
            None,
        )
        .await?;
        query.start().await?;
        measured.push(Measured {
            id,
            query,
            input: Times::default(),
            timers: Times::default(),
            empty_timer_batches_with_pending: 0,
            cpu_ticks: 0,
        });
    }
    let construction_ns = construction.elapsed().as_nanos();
    let process_cpu_start = cpu_ticks()?;
    let origin = corpus
        .frames
        .first()
        .context("empty corpus")?
        .observed_at_ms;
    let clock_shift = if paced {
        chrono::Utc::now()
            .timestamp_millis()
            .checked_sub(i64::try_from(origin)?)
            .context("clock shift out of range")?
    } else {
        0
    };
    let started = Instant::now();
    let mut arrival_lateness = Times::default();
    for frame in &corpus.frames {
        let envelope = if paced {
            let target = started
                + Duration::from_millis(
                    frame
                        .observed_at_ms
                        .checked_sub(origin)
                        .context("recording clock regressed")?,
                );
            tokio::time::sleep_until(target.into()).await;
            arrival_lateness.record(Instant::now().saturating_duration_since(target), &[], 0);
            rebase_envelope(frame, clock_shift)?
        } else {
            frame.envelope.clone()
        };
        for item in &mut measured {
            if !accepts(&item.id, frame) {
                continue;
            }
            let cpu = cpu_ticks()?;
            let tick = Instant::now();
            let result = item
                .query
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: envelope.clone(),
                })
                .await
                .with_context(|| {
                    format!(
                        "{} processing {} sequence {}",
                        item.id,
                        frame.envelope.system().stream(),
                        frame.envelope.system().sequence()
                    )
                })?;
            let elapsed = tick.elapsed();
            item.cpu_ticks += cpu_ticks()?
                .checked_sub(cpu)
                .context("CPU counter regressed")?;
            item.input.record(
                elapsed,
                &result,
                frame.envelope.changes().operations().len(),
            );
            item.query.delivery_completed(&result).await?;
        }
    }
    let mut snapshots = Vec::new();
    let mut failures = Vec::new();
    for item in &measured {
        let before = rows(&item.query)?;
        if let Err(error) = validate(&item.id, &before, &corpus, false) {
            failures.push(format!("{} before timers: {error:#}", item.id));
        }
        snapshots.push(json!({"query":item.id,"stage":"before-timers","rows":before}));
    }
    if paced {
        tokio::time::sleep(Duration::from_millis(5100)).await;
    }
    for item in &mut measured {
        let timer_limit = Instant::now() + Duration::from_secs(30);
        let mut due = Some(InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: GraphChangeCodec::encode_futures_due(
                &ComponentId::try_new(format!("{}/scheduled", item.id))?,
                StreamId::try_new(format!("{}/scheduled", item.id))?,
                1,
                chrono::Utc::now(),
            )?,
        });
        loop {
            let cpu = cpu_ticks()?;
            let tick = Instant::now();
            let result = match due.take() {
                Some(input) => item.query.transform(input).await?,
                None => item.query.continue_transform().await?,
            };
            item.timers.record(tick.elapsed(), &result, 0);
            item.cpu_ticks += cpu_ticks()?
                .checked_sub(cpu)
                .context("CPU counter regressed")?;
            item.query.delivery_completed(&result).await?;
            if !item.query.has_pending_emissions() {
                break;
            }
            item.empty_timer_batches_with_pending += usize::from(result.is_empty());
            ensure!(
                Instant::now() < timer_limit,
                "timer drain exceeded diagnostic bound"
            );
        }
        let after = rows(&item.query)?;
        if let Err(error) = validate(&item.id, &after, &corpus, true) {
            failures.push(format!("{} after timers: {error:#}", item.id));
        }
        snapshots.push(json!({"query":item.id,"stage":"after-timers","rows":after}));
    }
    let report = json!({
        "provider":mode,"selection":selection,"corpus":path,"input_frames":corpus.frames.len(),
        "paced":paced,"clock_shift_ms":clock_shift,"arrival_lateness":arrival_lateness.report(),
        "report_batches":corpus.report_batches,"observed_workload_counts":corpus.workload_counts,
        "workload_transitions":corpus.workload_transitions,"saved_plan_versions":corpus.plan_versions,
        "recording_span_ms":corpus.frames.last().unwrap().observed_at_ms - corpus.frames.first().unwrap().observed_at_ms,
        "host_load_before":load.trim(),"host_load_after":std::fs::read_to_string("/proc/loadavg")?.trim(),
        "corpus_load_ns":corpus_load_ns.to_string(),
        "construction_ns":construction_ns.to_string(),"execution_wall_ns":started.elapsed().as_nanos().to_string(),
        "execution_process_cpu_ticks":cpu_ticks()?.checked_sub(process_cpu_start).context("CPU counter regressed")?.to_string(),
        "cpu_clock_ticks_per_second":ticks_per_second,
        "scope":"Direct native transformer calls with actual provider; no graph input queue. Combined mode interleaves the same corpus across queries. Input and timer timings exclude validation and delivery completion. CPU uses process user+system ticks, including scoped I/O tasks, at the stated OS resolution.",
        "corpus_limitations":"Recorded native-envelope prefix before the first reporting-disable configuration, leaving fresh reports with pending timers. Database fixture rows are materialized from actual complete FleetConfiguration snapshots because the sibling recorder can miss initial database-query bootstrap. Recorded observer order is not any consumer's exact cross-stream merge order. Unpaced mode preserves original native envelopes and historical clocks. Paced mode reencodes graph changes with a common UTC shift (effective_from, time_ms, report_time_ms, acknowledged_at_ms, including payloads), preserving identities, values, monotonic times and observed arrival intervals. Due timers are explicitly drained after inputs (after 5.1 seconds without reports in paced mode), not concurrently with replay. Neither mode is full live graph acceptance.",
        "queries":measured.iter().map(|item|json!({
            "query":item.id,"input":item.input.report(),"timers":item.timers.report(),
            "empty_timer_batches_with_pending":item.empty_timer_batches_with_pending,
            "process_cpu_ticks_during_calls":item.cpu_ticks.to_string(),
        })).collect::<Vec<_>>(),
        "snapshots":snapshots,"correctness_failures":failures,
    });
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    for item in &mut measured {
        item.query.stop().await?;
    }
    drop(measured);
    drop(batches);
    core.shutdown().await?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"provider":mode,"selection":selection,"report":output,"queries":report["queries"],"correctness_failures":report["correctness_failures"]})
        )?
    );
    ensure!(
        failures.is_empty(),
        "query correctness assertions failed; see {}",
        output.display()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 4 || (args.len() == 5 && args[4] == "--paced"), "usage: query-isolation <recording.ndjson> <bare|pipeline-memory> <query-id|all> <report.json> [--paced]");
    tokio::time::timeout(
        Duration::from_secs(300),
        execute(
            Path::new(&args[0]),
            &args[1],
            &args[2],
            Path::new(&args[3]),
            args.len() == 5,
        ),
    )
    .await
    .context("query isolation exceeded five-minute execution bound")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paced_replay_shifts_only_utc_fields_and_payload_copies() -> Result<()> {
        let report = json!({
            "report_time_ms": 10000,
            "acknowledged_at_ms": 9000,
            "acknowledged_monotonic_ms": 700,
            "interval_ms": 1000,
            "elapsed_ms": 50,
            "unavailable": {"report_time_ms": null},
            "events": [{"time_ms": 8000}],
        });
        let mut properties = report.clone();
        properties["payload"] = serde_json::to_string(&report)?.into();
        rebase_properties(&mut properties, 2000)?;
        let payload: Value = serde_json::from_str(properties["payload"].as_str().unwrap())?;
        properties.as_object_mut().unwrap().remove("payload");
        assert_eq!(properties, payload);
        assert_eq!(properties["report_time_ms"], 12000);
        assert_eq!(properties["acknowledged_at_ms"], 11000);
        assert_eq!(properties["events"][0]["time_ms"], 10000);
        assert_eq!(properties["acknowledged_monotonic_ms"], 700);
        assert_eq!(properties["interval_ms"], 1000);
        assert_eq!(properties["elapsed_ms"], 50);
        rebase_properties(&mut properties, -2000)?;
        assert_eq!(properties, report);
        Ok(())
    }

    #[test]
    fn paced_replay_rejects_invalid_or_overflowing_clocks() {
        assert!(shifted_time(0, -1).is_err());
        assert!(shifted_time(u64::MAX, 1).is_err());
        assert!(rebase_properties(&mut json!({"report_time_ms": "invalid"}), 1).is_err());
        assert!(rebase_properties(&mut json!({"payload": "invalid json"}), 1).is_err());
    }
}
