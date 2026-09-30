use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const BOOTSTRAP_COMPLETE: &str = "gpu.lab/database-bootstrap-complete";
pub const RUNTIME_OBSERVATION: &str = "gpu.lab/runtime-observation";
pub const DATABASE_QUERIES: [&str; 7] = [
    "input-clusters",
    "input-policies",
    "input-data",
    "input-gpus",
    "input-settings",
    "input-workloads",
    "input-plan",
];

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
    #[serde(with = "crate::decimal")]
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeComponent {
    pub component_id: String,
    pub status: String,
    pub error: Option<String>,
}

/// Supplied by the lifecycle owner after observing source/query bootstrap and host statuses.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeObservation {
    #[serde(with = "crate::decimal")]
    pub sequence: u64,
    pub observation_epoch: Uuid,
    pub scenario: String,
    pub scheduling_signature: String,
    pub policy_signature: String,
    pub source_bootstrap_complete: bool,
    pub query_bootstrap_complete: bool,
    #[serde(default)]
    pub query_results_current: bool,
    pub reset_in_progress: bool,
    pub detail: String,
    pub required_components: BTreeSet<String>,
    pub components: Vec<RuntimeComponent>,
}

impl RuntimeObservation {
    pub fn inputs_ready(&self) -> bool {
        self.source_bootstrap_complete
            && self.query_bootstrap_complete
            && !self.reset_in_progress
            && !self.scheduling_signature.is_empty()
            && !self.policy_signature.is_empty()
            && !self.required_components.is_empty()
            && self.required_components.iter().all(|id| {
                self.components
                    .iter()
                    .any(|component| &component.component_id == id)
            })
            && self
                .components
                .iter()
                .all(|component| component.status == "running" && component.error.is_none())
    }

    pub fn projections_current(
        &self,
        required_replicas: u64,
        snapshots: &BTreeMap<&str, Vec<Value>>,
    ) -> Result<bool> {
        let get = |name| {
            snapshots
                .get(name)
                .with_context(|| format!("missing readiness query {name}"))
        };
        let placements = get("ui-placements")?;
        let resilience = get("ui-resilience")?;
        let policies = get("ui-policy")?;
        let workloads = get("ui-workloads")?;
        let decisions = get("ui-decisions")?;
        ensure!(
            placements.len() <= 1 && resilience.len() <= 1,
            "duplicate readiness context"
        );
        let epoch = self.observation_epoch.to_string();
        let same_policy = |row: &Value| {
            row["observation_epoch"].as_str() == Some(epoch.as_str())
                && row["policy_signature"].as_str() == Some(self.policy_signature.as_str())
        };
        let same_context = |row: &Value| {
            same_policy(row)
                && row["scheduling_signature"].as_str() == Some(self.scheduling_signature.as_str())
        };
        let Some(plan) = placements.first() else {
            return Ok(false);
        };
        let Some(assessment) = resilience.first() else {
            return Ok(false);
        };
        if !same_context(plan)
            || !same_context(assessment)
            || assessment["status"] != "current"
            || (required_replicas > 0 && policies.is_empty())
            || !policies.iter().all(|row| {
                same_policy(row)
                    && row["current"] == true
                    && matches!(row["authorization"].as_str(), Some("allow" | "deny"))
            })
        {
            return Ok(false);
        }
        let mut required = 0_u64;
        let mut all_confirmed = true;
        for workload in workloads {
            let replicas = u16::try_from(
                workload["replicas"]
                    .as_u64()
                    .context("invalid required replica count")?,
            )?;
            required = required
                .checked_add(u64::from(replicas))
                .context("replica count overflow")?;
            all_confirmed &= workload["ready_replicas"].as_f64() == Some(f64::from(replicas));
            if workload["fencing_pending_replicas"].as_f64() != Some(0.0)
                || workload["suspended_replicas"].as_f64() != Some(0.0)
            {
                return Ok(false);
            }
        }
        if required != required_replicas {
            return Ok(false);
        }
        let desired = plan["desired"]
            .as_array()
            .context("missing desired assignments")?;
        let confirmed = plan["status"] == "confirmed"
            && all_confirmed
            && u64::try_from(desired.len())? == required_replicas
            && plan["desired_plan_version"].is_string()
            && plan["desired_plan_version"] == plan["applied_plan_version"]
            && plan["applied_plan_version"] == plan["confirmed_plan_version"];
        let infeasible = decisions.iter().any(|row| {
            same_context(row) && row["stage"] == "diagnostic" && row["outcome"] == "infeasible"
        });
        Ok(confirmed || infeasible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bootstrap_requires_explicit_empty_results_and_exact_row_counts() {
        let mut boundary = BootstrapBoundary {
            epoch: Uuid::new_v4(),
            queries: DATABASE_QUERIES
                .into_iter()
                .map(|id| {
                    (
                        id.into(),
                        QueryBootstrapWatermark {
                            sequence: 0,
                            row_count: 0,
                            rows: Some(vec![]),
                        },
                    )
                })
                .collect(),
        };
        boundary.validate().unwrap();
        boundary.queries.get_mut("input-gpus").unwrap().row_count = 1;
        assert!(boundary.validate().is_err());
        boundary.queries.remove("input-gpus");
        assert!(boundary.validate().is_err());
    }

    #[test]
    fn bootstrap_signatures_preserve_all_u64_bits() {
        let row = QueryBootstrapRow {
            signature: u64::MAX,
            records: vec![json!({"plan_version": "9007199254740993"})],
        };
        let encoded = serde_json::to_value(&row).unwrap();
        assert_eq!(encoded["signature"], u64::MAX.to_string());
        assert_eq!(
            serde_json::from_value::<QueryBootstrapRow>(encoded).unwrap(),
            row
        );
    }

    #[test]
    fn runtime_readiness_requires_real_bootstrap_and_healthy_components() {
        let mut observation = RuntimeObservation {
            sequence: 1,
            observation_epoch: Uuid::new_v4(),
            scenario: "baseline".into(),
            scheduling_signature: "schedule".into(),
            policy_signature: "policy".into(),
            source_bootstrap_complete: true,
            query_bootstrap_complete: true,
            query_results_current: true,
            reset_in_progress: false,
            detail: "Observed".into(),
            required_components: BTreeSet::from(["policy".into()]),
            components: vec![RuntimeComponent {
                component_id: "policy".into(),
                status: "running".into(),
                error: None,
            }],
        };
        assert!(observation.inputs_ready());
        observation.query_bootstrap_complete = false;
        assert!(!observation.inputs_ready());
        observation.query_bootstrap_complete = true;
        observation.components[0].error = Some("failed".into());
        assert!(!observation.inputs_ready());
        observation.components.clear();
        assert!(!observation.inputs_ready());
    }
}
