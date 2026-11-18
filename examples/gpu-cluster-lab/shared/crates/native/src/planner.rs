//! Turns the optimization transformer's solution and the policy transformer's
//! decisions into validated candidate plans.
use crate::inputs::DatabaseSnapshot;
use anyhow::{Context as _, Result};
use drasi_computation_contracts::{ChangeEnvelope, ChangeOperation, QueryChangeCodec};
use gpu_contracts::{
    planning,
    policy::{self, Assessment},
    Assignment, Candidate, Capacity, Configuration,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Inputs settle for this long before a plan is evaluated.
const SETTLE: Duration = Duration::from_millis(250);
/// Longest wait for the solver to answer an input change; its outputs are
/// unchanged when a change does not affect the model.
const SOLVER_WAIT: Duration = Duration::from_millis(1500);

pub const INFEASIBLE: &str =
    "No complete assignment satisfies resources, compatibility, policy and worker separation";

/// Current rows of one continuous query, keyed by row identity.
#[derive(Default)]
pub(crate) struct QueryRows(BTreeMap<Vec<u8>, Value>);

impl QueryRows {
    pub fn apply(&mut self, envelope: &ChangeEnvelope) -> Result<bool> {
        let projection = QueryChangeCodec::projection(envelope)?;
        let operations = envelope.changes().operations();
        anyhow::ensure!(
            operations.len() == projection.operations().len(),
            "query projection does not match its operations"
        );
        let mut changed = false;
        for (operation, meta) in operations.iter().zip(projection.operations()) {
            let identity = meta.row_identity.to_vec();
            match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    let row = serde_json::to_value(QueryChangeCodec::decode_row(after)?.values)?;
                    changed |= self.0.insert(identity, row.clone()) != Some(row);
                }
                ChangeOperation::Deleted { .. } => changed |= self.0.remove(&identity).is_some(),
            }
        }
        Ok(changed)
    }
    pub fn decode<T: for<'de> Deserialize<'de>>(&self) -> Result<Vec<T>> {
        self.0
            .values()
            .map(|row| Ok(serde_json::from_value(row.clone())?))
            .collect()
    }
}

/// Builds the complete assessment from `planner-policy` rows.
pub(crate) fn assessment(config: &Configuration, rows: &QueryRows) -> Result<Assessment> {
    let decisions: Vec<policy::Decision> = rows.decode()?;
    Assessment::from_decisions(config, &decisions)
}

#[derive(Deserialize)]
struct GpuRow {
    gpu_id: Uuid,
    eligible: Option<bool>,
    memory_mib: Option<f64>,
    compute_units: Option<f64>,
}

#[derive(Deserialize)]
struct SolutionRow {
    workload_id: Uuid,
    replica_index: f64,
    gpu_id: Uuid,
}

#[derive(Deserialize)]
struct StatusRow {
    status: String,
    detail: Option<String>,
    signature: Option<String>,
    provenance: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Context {
    pub scheduling_signature: String,
    pub policy_signature: String,
    pub policy_bundle_hash: String,
    pub config_fingerprint: String,
    pub required_replicas: u64,
}

pub(crate) enum Outcome {
    Waiting(&'static str, Option<String>),
    Current,
    Candidate(Box<Candidate>),
    Infeasible(String),
    Unknown(String),
}

#[derive(Default)]
pub(crate) struct Planner {
    pub snapshot: Option<DatabaseSnapshot>,
    pub gpus: QueryRows,
    pub policy: QueryRows,
    pub solution: QueryRows,
    pub status: QueryRows,
    pub context: Option<Context>,
    changed: Option<Instant>,
    solver_answered: bool,
    last: Option<String>,
}

fn capacities(config: &Configuration, rows: &QueryRows) -> Result<BTreeMap<Uuid, Capacity>> {
    let observed: BTreeMap<Uuid, GpuRow> = rows
        .decode::<GpuRow>()?
        .into_iter()
        .map(|row| (row.gpu_id, row))
        .collect();
    let units = |value: Option<f64>| -> Result<u32> {
        let value = value.unwrap_or(0.0).max(0.0);
        anyhow::ensure!(value.fract() == 0.0, "capacity must be integral");
        Ok(u32::try_from(value as u64)?)
    };
    config
        .gpus
        .values()
        .map(|gpu| {
            let row = observed.get(&gpu.gpu_id);
            Ok((
                gpu.gpu_id,
                Capacity {
                    gpu_id: gpu.gpu_id,
                    eligible: row.and_then(|row| row.eligible).unwrap_or(false),
                    memory_mib: units(row.and_then(|row| row.memory_mib))?,
                    compute_units: units(row.and_then(|row| row.compute_units))?,
                },
            ))
        })
        .collect()
}

impl Planner {
    /// Records an input change; `solver` marks a solver answer.
    pub fn touch(&mut self, solver: bool) {
        if solver {
            self.solver_answered = true;
        } else {
            self.changed = Some(Instant::now());
            self.solver_answered = false;
        }
        self.changed.get_or_insert_with(Instant::now);
    }

    /// Forgets the configuration and the last outcome; query rows are
    /// incremental state and stay.
    pub fn invalidate(&mut self) {
        self.snapshot = None;
        self.context = None;
        self.last = None;
    }

    /// Evaluates the settled inputs once per distinct outcome.
    pub fn evaluate(&mut self) -> Result<Option<Outcome>> {
        let Some(changed) = self.changed else {
            return Ok(None);
        };
        let waited = changed.elapsed();
        if waited < SETTLE || (!self.solver_answered && waited < SOLVER_WAIT) {
            return Ok(None);
        }
        self.changed = None;
        let Some(snapshot) = &self.snapshot else {
            self.context = None;
            return Ok(Some(Outcome::Waiting("awaiting-configuration", None)));
        };
        let config = &snapshot.configuration;
        let plan = &snapshot.plan;
        let assessment = match assessment(config, &self.policy) {
            Ok(assessment) => assessment,
            Err(error) => {
                self.context = None;
                self.last = None;
                return Ok(Some(Outcome::Waiting(
                    "policy-pending",
                    Some(format!("{error:#}")),
                )));
            }
        };
        let capacities = capacities(config, &self.gpus)?;
        let fingerprint = config.fingerprint()?;
        let required = config
            .workloads
            .values()
            .map(|w| u64::from(w.replicas))
            .sum();
        self.context = Some(Context {
            scheduling_signature: planning::signature(config, &assessment, &capacities)?,
            policy_signature: assessment.policy_signature.clone(),
            policy_bundle_hash: assessment.policy_bundle_hash.clone(),
            config_fingerprint: fingerprint.clone(),
            required_replicas: required,
        });
        let statuses: Vec<StatusRow> = self.status.decode()?;
        let Some(status) = statuses.into_iter().next() else {
            return Ok(Some(Outcome::Waiting("solver-pending", None)));
        };
        let provenance = status.provenance.unwrap_or(Value::Null);
        if provenance["config_fingerprint"].as_str() != Some(fingerprint.as_str())
            || provenance["plan_version"].as_str() != Some(plan.plan_version.to_string().as_str())
        {
            return Ok(Some(Outcome::Waiting("solver-pending", None)));
        }
        let mut assignments = Vec::new();
        if status.status == "optimal" {
            for row in self.solution.decode::<SolutionRow>()? {
                let Some(workload) = config.workloads.get(&row.workload_id) else {
                    return Ok(Some(Outcome::Waiting("solver-pending", None)));
                };
                assignments.push(Assignment::new(
                    workload,
                    row.replica_index as u32,
                    row.gpu_id,
                ));
            }
            assignments.sort();
        }
        let context = self
            .context
            .as_ref()
            .context("scheduling context missing")?;
        let key = serde_json::to_string(&(
            &context.scheduling_signature,
            &status.status,
            &status.signature,
            &assignments,
            plan.plan_version,
        ))?;
        if self.last.as_ref() == Some(&key) {
            return Ok(None);
        }
        let outcome = match status.status.as_str() {
            "optimal" => {
                if u64::try_from(assignments.len())? != required {
                    return Ok(Some(Outcome::Waiting("solver-pending", None)));
                }
                if assignments == plan.assignments
                    && plan.config_fingerprint == fingerprint
                    && plan.policy_signature == assessment.policy_signature
                {
                    Outcome::Current
                } else {
                    match planning::candidate(
                        config,
                        &assessment,
                        &capacities,
                        plan,
                        assignments,
                        vec!["scheduling-input-changed".into()],
                    ) {
                        Ok(candidate) => Outcome::Candidate(Box::new(candidate)),
                        Err(error) => {
                            return Ok(Some(Outcome::Waiting(
                                "solver-pending",
                                Some(format!("{error:#}")),
                            )))
                        }
                    }
                }
            }
            "infeasible" => Outcome::Infeasible(INFEASIBLE.into()),
            other => Outcome::Unknown(
                status
                    .detail
                    .unwrap_or_else(|| format!("solver status {other}")),
            ),
        };
        self.last = Some(key);
        Ok(Some(outcome))
    }
}
