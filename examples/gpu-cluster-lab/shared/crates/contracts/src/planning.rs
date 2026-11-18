//! Candidate plans built from an optimizer solution.
//!
//! The `drasi.standard/optimization` instance solves the placement model; this
//! module re-validates its assignments against the current inputs and turns
//! them into a bounded, explainable candidate for the plan writer.

use crate::{policy::Assessment, *};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicaMove {
    pub replica_id: String,
    pub from_gpu_id: Option<Uuid>,
    pub to_gpu_id: Uuid,
    pub reason: String,
}

/// Identity of the scheduling inputs: configuration, policy decisions and
/// observed capacities.
pub fn signature(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
) -> Result<String> {
    hash(
        "gpu-scheduling-v1",
        &(config.fingerprint()?, &policy.policy_signature, capacities),
    )
}

/// The replica moves from `previous` to `assignments`.
pub fn moves(previous: &Plan, assignments: &[Assignment]) -> Vec<ReplicaMove> {
    assignments
        .iter()
        .filter_map(|assignment| {
            let previous = previous
                .assignments
                .iter()
                .find(|old| old.key() == assignment.key());
            if previous.is_some_and(|old| old.gpu_id == assignment.gpu_id) {
                return None;
            }
            Some(ReplicaMove {
                replica_id: format!("{}/{}", assignment.workload_id, assignment.replica_index),
                from_gpu_id: previous.map(|old| old.gpu_id),
                to_gpu_id: assignment.gpu_id,
                reason: if previous.is_some() {
                    "moved-for-complete-feasible-plan"
                } else {
                    "new-replica"
                }
                .into(),
            })
        })
        .collect()
}

/// Validates a complete solution against current inputs and builds a candidate.
pub fn candidate(
    config: &Configuration,
    policy: &Assessment,
    capacities: &BTreeMap<Uuid, Capacity>,
    previous: &Plan,
    assignments: Vec<Assignment>,
    reasons: Vec<String>,
) -> Result<Candidate> {
    validate_assignments(config, capacities, &assignments)?;
    policy.validate_plan(config, &assignments)?;
    let free: Vec<_> = capacities
        .values()
        .filter(|c| c.eligible)
        .map(|c| {
            let used: u32 = previous
                .assignments
                .iter()
                .filter(|a| a.gpu_id == c.gpu_id)
                .map(|a| a.memory_mib)
                .sum();
            c.memory_mib.saturating_sub(used)
        })
        .collect();
    let moves = moves(previous, &assignments);
    let moved_replicas = moves.iter().filter(|m| m.from_gpu_id.is_some()).count();
    let new_replicas = moves.len() - moved_replicas;
    ensure!(
        moves.len() <= assignments.len(),
        "movement evidence exceeds assignments"
    );
    let scheduling_signature = signature(config, policy, capacities)?;
    let details = serde_json::json!({
        "schema_version":1, "reason_codes":reasons, "moved_replicas":moved_replicas,
        "new_replicas":new_replicas, "retained_replicas":assignments.len()-moved_replicas-new_replicas,
        "pre_plan_free_memory_mib":free.iter().sum::<u32>(),
        "pre_plan_largest_gap_mib":free.iter().max().copied().unwrap_or(0),
        "moves":moves, "scheduling_signature":scheduling_signature,
        "capacities":capacities.values().collect::<Vec<_>>(), "policy":policy,
    });
    ensure!(
        serde_json::to_vec(&details)?.len() <= 128 * 1024,
        "decision evidence exceeds limit"
    );
    Ok(Candidate {
        decision_id: Uuid::new_v4(),
        expected_plan_version: previous.plan_version,
        config_fingerprint: config.fingerprint()?,
        policy_signature: policy.policy_signature.clone(),
        policy_bundle_hash: policy.policy_bundle_hash.clone(),
        scheduling_signature,
        assignments,
        decision_details: details,
    })
}
