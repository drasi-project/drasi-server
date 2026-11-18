use anyhow::{ensure, Context, Result};
use drasi_computation_contracts::{
    ChangeOperation, QueryChangeCodec, ResultNodeProjector, ResultOperationMetadata,
};
use gpu_contracts::{Configuration, Plan, Settings};
use gpu_control::db::Table;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

pub const SIMULATION_QUERY: &str = include_str!("../../../queries/simulation-inputs.cypher");
pub const PAIRS_QUERY: &str = include_str!("../../../queries/placement-pairs.cypher");
pub const GPUS_QUERY: &str = include_str!("../../../queries/placement-gpus.cypher");
pub const CANDIDATES_QUERY: &str = include_str!("../../../queries/placement-candidates.cypher");
pub const PLANNER_POLICY_QUERY: &str = include_str!("../../../queries/planner-policy.cypher");

/// Continuous queries feeding the native, policy and optimization transformers.
#[cfg(any(test, feature = "runtime", feature = "query-isolation"))]
pub fn processing_queries() -> Vec<(
    &'static str,
    &'static str,
    drasi_lib::computation::v1::QueryExecutionSettings,
)> {
    use drasi_lib::computation::v1::QueryExecutionSettings;
    let labelled = |label: &str, mut settings: QueryExecutionSettings| {
        settings.result.label = Some(label.into());
        settings
    };
    vec![
        ("simulation-inputs", SIMULATION_QUERY, QueryExecutionSettings::default()),
        ("placement-pairs", PAIRS_QUERY, labelled("PlacementPair", crate::projections::pair_settings())),
        ("placement-gpus", GPUS_QUERY, labelled("PlacementGpu", crate::projections::cluster_settings())),
        ("placement-candidates", CANDIDATES_QUERY, labelled("PlacementCandidate", crate::projections::cluster_settings())),
        ("planner-policy", PLANNER_POLICY_QUERY, QueryExecutionSettings::default()),
        ("planner-solution", "MATCH (a:PlacementAssignment) RETURN a.workload_id AS workload_id, a.replica_index AS replica_index, a.gpu_id AS gpu_id", QueryExecutionSettings::default()),
        ("planner-status", "MATCH (s:PlacementStatus) RETURN s.status AS status, s.detail AS detail, s.signature AS signature, s.provenance AS provenance", QueryExecutionSettings::default()),
        ("plan-output", "MATCH (p:CandidatePlan) RETURN p.payload AS payload", QueryExecutionSettings::default()),
        ("runtime-context", "MATCH (c:SchedulingContext) RETURN c.scheduling_signature AS scheduling_signature, c.policy_signature AS policy_signature, c.required_replicas AS required_replicas, c.current AS current", QueryExecutionSettings::default()),
    ]
}

pub const DATABASE_QUERY: &str = "input-configuration";
pub const DATABASE_QUERIES: [&str; 1] = [DATABASE_QUERY];
pub const TABLE_INPUTS: [(&str, &str); 7] = [
    ("input-clusters", "regional_clusters"),
    ("input-policies", "placement_policies"),
    ("input-data", "data_profiles"),
    ("input-gpus", "gpu_inventory"),
    ("input-settings", "gpu_telemetry"),
    ("input-workloads", "workload_requirements"),
    ("input-plan", "gpu_placements"),
];

pub fn database_queries() -> Vec<(&'static str, String)> {
    let tables = [
        Table::Clusters,
        Table::Policies,
        Table::Data,
        Table::Gpus,
        Table::Settings,
        Table::Workloads,
    ];
    let mut cases: Vec<_> = tables
        .into_iter()
        .map(|table| {
            let fields = table
                .columns()
                .iter()
                .copied()
                .chain(["revision"])
                .map(|column| format!("{column}: n.{column}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("WHEN '{}' THEN {{{fields}}}", table.name())
        })
        .collect();
    cases.push("WHEN 'gpu_placements' THEN {fleet_id:n.fleet_id, plan_version:n.plan_version, decision_id:n.decision_id, config_fingerprint:n.config_fingerprint, policy_signature:n.policy_signature, policy_bundle_hash:n.policy_bundle_hash, assignments:n.assignments, decision_details:n.decision_details}".into());
    vec![(DATABASE_QUERY, format!(
        "MATCH (n) RETURN collect({{table: n.`__gpu_table`, value: CASE n.`__gpu_table` {} END}}) AS records",
        cases.join(" ")
    ))]
}

/// Snapshot of the single transactional input query, including its output watermark.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapBoundary {
    pub epoch: Uuid,
    pub queries: BTreeMap<String, QueryBootstrapWatermark>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryBootstrapWatermark {
    pub sequence: u64,
    pub row_count: usize,
    #[serde(default)]
    pub rows: Option<Vec<QueryBootstrapRow>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryBootstrapRow {
    #[serde(with = "gpu_contracts::decimal")]
    pub signature: u64,
    pub records: Vec<Value>,
}

impl BootstrapBoundary {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.queries.len() == DATABASE_QUERIES.len()
                && DATABASE_QUERIES
                    .iter()
                    .all(|id| self.queries.contains_key(*id)),
            "bootstrap must explicitly observe every database query, including empty results"
        );
        ensure!(
            self.queries
                .values()
                .all(|watermark| watermark.row_count <= 1),
            "database queries must have at most one aggregate result row"
        );
        ensure!(
            self.queries.values().all(|watermark| watermark
                .rows
                .as_ref()
                .is_none_or(|rows| rows.len() == watermark.row_count)),
            "bootstrap snapshot row count differs from its watermark"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSnapshot {
    pub epoch: Uuid,
    pub configuration: Configuration,
    pub settings: BTreeMap<Uuid, Settings>,
    pub plan: Plan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SimulationRow {
    pub snapshot: DatabaseSnapshot,
}

pub fn partition_records(records: &[Value]) -> Result<BTreeMap<&'static str, Vec<Value>>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        table: String,
        value: Value,
    }
    let mut tables: BTreeMap<_, Vec<Value>> = TABLE_INPUTS
        .iter()
        .map(|(id, _)| (*id, Vec::new()))
        .collect();
    for value in records {
        let record: Record = serde_json::from_value(value.clone())?;
        let (id, _) = TABLE_INPUTS
            .iter()
            .find(|(_, table)| *table == record.table)
            .context("unknown database input table")?;
        let rows = tables.get_mut(id).context("missing input table")?;
        ensure!(record.value.is_object(), "database input must be an object");
        ensure!(rows.len() < 64, "database table row bound exceeded");
        rows.push(record.value);
    }
    Ok(tables)
}

#[derive(Default)]
pub(crate) struct DatabaseInputs {
    rows: BTreeMap<String, Vec<Value>>,
    /// Aggregate rows keyed by their opaque row identity, which is independent
    /// of the output scope that carried them.
    aggregates: BTreeMap<String, HashMap<Vec<u8>, Vec<Value>>>,
    sequences: BTreeMap<String, u64>,
    snapshot_floors: BTreeMap<String, u64>,
    boundary: Option<BootstrapBoundary>,
    epoch: Option<Uuid>,
    dirty: bool,
}

impl DatabaseInputs {
    pub fn bootstrap(&mut self, boundary: BootstrapBoundary) -> Result<()> {
        boundary.validate()?;
        if let Some(previous) = self.epoch {
            ensure!(
                previous == boundary.epoch,
                "bootstrap belongs to another runtime epoch"
            );
        }
        let mut seeds = Vec::new();
        for (query, watermark) in &boundary.queries {
            if let Some(rows) = &watermark.rows {
                let mut aggregates = HashMap::new();
                for row in rows {
                    ensure!(
                        row.records.len() <= 448,
                        "database input row bound exceeded"
                    );
                    aggregates.insert(
                        ResultNodeProjector::signature_identity(row.signature).to_vec(),
                        row.records.clone(),
                    );
                }
                seeds.push((query.clone(), watermark.sequence, aggregates));
            }
        }
        for (query, sequence, aggregates) in seeds {
            if self
                .sequences
                .get(&query)
                .is_none_or(|current| *current <= sequence)
            {
                self.rows.insert(
                    query.clone(),
                    aggregates.values().next().cloned().unwrap_or_default(),
                );
                self.aggregates.insert(query.clone(), aggregates);
                self.sequences.insert(query.clone(), sequence);
            }
            let floor = self.snapshot_floors.entry(query).or_default();
            *floor = (*floor).max(sequence);
        }
        self.epoch = Some(boundary.epoch);
        self.boundary = Some(boundary);
        self.dirty = true;
        Ok(())
    }

    pub fn unavailable(&mut self) {
        self.boundary = None;
        self.dirty = false;
    }

    pub fn update(
        &mut self,
        query: &str,
        sequence: u64,
        operations: &[ChangeOperation],
        projection: &[ResultOperationMetadata],
    ) -> Result<()> {
        ensure!(
            DATABASE_QUERIES.contains(&query),
            "unknown database input query"
        );
        ensure!(
            operations.len() == projection.len(),
            "database query projection does not match its operations"
        );
        if self
            .snapshot_floors
            .get(query)
            .is_some_and(|floor| sequence < *floor)
        {
            return Ok(());
        }
        ensure!(
            self.sequences
                .get(query)
                .is_none_or(|previous| sequence >= *previous),
            "database query output sequence regressed"
        );
        let mut aggregates = self.aggregates.get(query).cloned().unwrap_or_default();
        for (operation, meta) in operations.iter().zip(projection) {
            let identity = meta.row_identity.to_vec();
            match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    let row = QueryChangeCodec::decode_row(after)?;
                    ensure!(
                        row.identity == meta.row_identity,
                        "database query row identity mismatch"
                    );
                    ensure!(row.values.len() == 1, "unexpected database query columns");
                    let records: Vec<Value> = serde_json::from_value(serde_json::to_value(
                        row.values
                            .get("records")
                            .context("database query requires records")?,
                    )?)?;
                    ensure!(records.len() <= 448, "database input row bound exceeded");
                    aggregates.insert(identity, records);
                }
                ChangeOperation::Deleted { .. } => {
                    aggregates.remove(&identity);
                }
            }
        }
        ensure!(
            aggregates.len() <= 1,
            "multiple database aggregate result identities"
        );
        if self.sequences.get(query) == Some(&sequence) {
            ensure!(
                self.aggregates.get(query) == Some(&aggregates),
                "database query changed without advancing its sequence"
            );
            return Ok(());
        }
        let rows = aggregates.values().next().cloned().unwrap_or_default();
        self.aggregates.insert(query.into(), aggregates);
        self.rows.insert(query.into(), rows);
        self.sequences.insert(query.into(), sequence);
        self.dirty = true;
        Ok(())
    }

    pub fn take(&mut self) -> Result<Option<DatabaseSnapshot>> {
        let Some(boundary) = &self.boundary else {
            return Ok(None);
        };
        if !self.dirty
            || boundary
                .queries
                .iter()
                .any(|(id, required)| match self.sequences.get(id) {
                    Some(sequence) => {
                        *sequence < required.sequence
                            || (*sequence == required.sequence
                                && self.aggregates.get(id).map_or(0, HashMap::len)
                                    != required.row_count)
                    }
                    None => required.row_count != 0,
                })
        {
            return Ok(None);
        }
        self.dirty = false;
        Ok(Some(DatabaseSnapshot::from_records(
            boundary.epoch,
            self.rows
                .get(DATABASE_QUERY)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        )?))
    }
}

impl DatabaseSnapshot {
    pub fn from_records(epoch: Uuid, records: &[Value]) -> Result<Self> {
        let tables = partition_records(records)?;
        let rows = |id: &str| -> Value { Value::Array(tables[id].clone()) };
        let configuration = Configuration {
            clusters: serde_json::from_value::<Vec<gpu_contracts::Cluster>>(rows(
                "input-clusters",
            ))?
            .into_iter()
            .map(|row| (row.cluster_id.clone(), row))
            .collect(),
            policies: serde_json::from_value::<Vec<gpu_contracts::Policy>>(rows("input-policies"))?
                .into_iter()
                .map(|row| (row.policy_id.clone(), row))
                .collect(),
            data_profiles: serde_json::from_value::<Vec<gpu_contracts::DataProfile>>(rows(
                "input-data",
            ))?
            .into_iter()
            .map(|row| (row.data_profile_id.clone(), row))
            .collect(),
            gpus: serde_json::from_value::<Vec<gpu_contracts::Gpu>>(rows("input-gpus"))?
                .into_iter()
                .map(|row| (row.gpu_id, row))
                .collect(),
            workloads: serde_json::from_value::<Vec<gpu_contracts::Workload>>(rows(
                "input-workloads",
            ))?
            .into_iter()
            .map(|row| (row.workload_id, row))
            .collect(),
        };
        let settings = serde_json::from_value::<Vec<Settings>>(rows("input-settings"))?
            .into_iter()
            .map(|row| (row.gpu_id, row))
            .collect::<BTreeMap<_, _>>();
        for (query, count) in [
            ("input-clusters", configuration.clusters.len()),
            ("input-policies", configuration.policies.len()),
            ("input-data", configuration.data_profiles.len()),
            ("input-gpus", configuration.gpus.len()),
            ("input-settings", settings.len()),
            ("input-workloads", configuration.workloads.len()),
        ] {
            ensure!(
                tables[query].len() == count,
                "duplicate database record identity in {query}"
            );
        }

        let mut plans: Vec<Plan> = serde_json::from_value(rows("input-plan"))?;
        ensure!(
            plans.len() == 1,
            "database must contain exactly one fleet plan"
        );
        let plan = plans.pop().context("fleet plan missing")?;
        ensure!(plan.fleet_id == "demo", "unexpected fleet plan");
        configuration.validate()?;
        gpu_contracts::validate_settings(&configuration, &settings)?;
        Ok(Self {
            epoch,
            configuration,
            settings,
            plan,
        })
    }
}
