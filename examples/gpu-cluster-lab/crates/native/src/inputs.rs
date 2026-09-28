use anyhow::{ensure, Context, Result};
use drasi_lib::computation::v1::{
    ChangeOperation, QueryChangeCodec, QueryRowKind, RecordId, RecordImage,
};
use gpu_contracts::{Configuration, Plan, Settings};
use gpu_control::db::Table;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

pub const SIMULATION_QUERY: &str = include_str!("../../../queries/simulation-inputs.cypher");
pub const SCHEDULING_QUERY: &str = include_str!("../../../queries/scheduling-inputs.cypher");

pub fn processing_queries() -> Vec<(
    &'static str,
    &'static str,
    drasi_lib::computation::v1::QueryExecutionSettings,
)> {
    use drasi_lib::{
        computation::v1::QueryExecutionSettings,
        config::{QueryJoinConfig, QueryJoinKeyConfig},
    };
    let policy_join = QueryJoinConfig {
        id: "POLICY_CONFIG".into(),
        keys: ["FleetConfiguration", "PolicyAssessment"]
            .into_iter()
            .map(|label| QueryJoinKeyConfig {
                label: label.into(),
                property: "config_fingerprint".into(),
            })
            .collect(),
    };
    let mut scheduling = crate::projections::cluster_settings();
    scheduling.joins.push(policy_join.clone());
    vec![
        ("simulation-inputs", SIMULATION_QUERY, QueryExecutionSettings {
            joins: vec![policy_join], ..Default::default()
        }),
        ("scheduling-inputs", SCHEDULING_QUERY, scheduling),
        ("plan-output", "MATCH (p:CandidatePlan) RETURN p.payload AS payload", QueryExecutionSettings::default()),
        ("runtime-context", "MATCH (c:SchedulingContext) RETURN c.scheduling_signature AS scheduling_signature, c.policy_signature AS policy_signature, c.required_replicas AS required_replicas, c.current AS current", QueryExecutionSettings::default()),
    ]
}

pub const DATABASE_QUERIES: [&str; 7] = [
    "input-clusters",
    "input-policies",
    "input-data",
    "input-gpus",
    "input-settings",
    "input-workloads",
    "input-plan",
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
    let mut queries: Vec<_> = tables
        .into_iter()
        .enumerate()
        .map(|(index, table)| {
            let fields = table
                .columns()
                .iter()
                .copied()
                .chain(["revision"])
                .map(|column| format!("{column}: n.{column}"))
                .collect::<Vec<_>>()
                .join(", ");
            (
                DATABASE_QUERIES[index],
                format!(
                    "MATCH (n:{}) RETURN collect({{{fields}}}) AS records",
                    table.name()
                ),
            )
        })
        .collect();
    queries.push((
        "input-plan",
        "MATCH (n:gpu_placements) RETURN collect({fleet_id:n.fleet_id, plan_version:n.plan_version, decision_id:n.decision_id, config_fingerprint:n.config_fingerprint, policy_signature:n.policy_signature, policy_bundle_hash:n.policy_bundle_hash, assignments:n.assignments, decision_details:n.decision_details}) AS records".into(),
    ));
    queries
}

/// Query bootstrap watermarks, not PostgreSQL transaction boundaries.
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
    pub policy: Option<gpu_policy::Assessment>,
    pub policy_epoch: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SchedulingRow {
    pub epoch: Uuid,
    pub configuration: Configuration,
    pub plan: Plan,
    pub policy: gpu_policy::Assessment,
    pub policy_epoch: Uuid,
    pub capacities: Vec<Option<ObservedCapacity>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservedCapacity {
    gpu_id: Uuid,
    fresh: bool,
    background_memory_mib: Option<u32>,
    background_compute_units: Option<u32>,
}

impl SchedulingRow {
    pub fn message(self) -> Result<crate::wire::Message> {
        ensure!(self.epoch == self.policy_epoch, "policy epoch is stale");
        let mut capacities = BTreeMap::new();
        for observed in self.capacities.into_iter().flatten() {
            let gpu = self
                .configuration
                .gpus
                .get(&observed.gpu_id)
                .context("capacity references unknown inventory")?;
            let capacity = match (
                observed.background_memory_mib,
                observed.background_compute_units,
            ) {
                (Some(memory), Some(demand)) => {
                    gpu_contracts::Capacity::from_observation(gpu, observed.fresh, memory, demand)
                }
                (None, None) if !observed.fresh => gpu_contracts::Capacity {
                    gpu_id: gpu.gpu_id,
                    eligible: false,
                    memory_mib: 0,
                    compute_units: 0,
                },
                _ => anyhow::bail!("incomplete reported capacity"),
            };
            ensure!(
                capacities.insert(gpu.gpu_id, capacity).is_none(),
                "duplicate GPU capacity"
            );
        }
        let input = crate::wire::SolveInput {
            configuration: self.configuration,
            policy: self.policy,
            plan: self.plan,
            capacities,
        };
        input.validate()?;
        Ok(crate::wire::Message::Schedule {
            epoch: self.epoch,
            input,
        })
    }
}

#[derive(Default)]
pub(crate) struct DatabaseInputs {
    rows: BTreeMap<String, Vec<Value>>,
    aggregates: BTreeMap<String, HashMap<RecordId, Vec<Value>>>,
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
                    ensure!(row.records.len() <= 64, "database input row bound exceeded");
                    let record = QueryChangeCodec::encode_row(
                        query,
                        row.signature,
                        &BTreeMap::from([(
                            "records".into(),
                            drasi_core::evaluation::variable_value::VariableValue::from(
                                Value::Array(row.records.clone()),
                            ),
                        )]),
                        QueryRowKind::Aggregation {
                            grouping_keys: vec![],
                            default_before: false,
                            default_after: false,
                        },
                        RecordImage::Full,
                    )?;
                    aggregates.insert(record.identity().clone(), row.records.clone());
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
    ) -> Result<()> {
        ensure!(
            DATABASE_QUERIES.contains(&query),
            "unknown database input query"
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
        for operation in operations {
            match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    let row = QueryChangeCodec::decode_row(after)?;
                    ensure!(row.query_id == query, "database query identity mismatch");
                    ensure!(row.values.len() == 1, "unexpected database query columns");
                    let records: Vec<Value> = serde_json::from_value(serde_json::to_value(
                        row.values
                            .get("records")
                            .context("database query requires records")?,
                    )?)?;
                    ensure!(records.len() <= 64, "database input row bound exceeded");
                    aggregates.insert(operation.identity().clone(), records);
                }
                ChangeOperation::Deleted { .. } => {
                    aggregates.remove(operation.identity());
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
        let rows =
            |id: &str| -> Value { Value::Array(self.rows.get(id).cloned().unwrap_or_default()) };
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
                self.rows.get(query).map_or(0, Vec::len) == count,
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
        Ok(Some(DatabaseSnapshot {
            epoch: boundary.epoch,
            configuration,
            settings,
            plan,
        }))
    }
}
