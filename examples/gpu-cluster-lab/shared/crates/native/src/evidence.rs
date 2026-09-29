use anyhow::{ensure, Context, Result};
use gpu_contracts::{Candidate, Configuration, Plan};
use gpu_placement::ReplicaMove;
use gpu_policy::{pair_key, Assessment, Authorization};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionOutcome {
    Feasible,
    Infeasible,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionStage {
    Candidate,
    Committed,
    Diagnostic,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Facts {
    pub moved_replicas: Option<usize>,
    pub new_replicas: Option<usize>,
    pub retained_replicas: Option<usize>,
    pub pre_plan_free_memory_mib: Option<u32>,
    pub pre_plan_largest_gap_mib: Option<u32>,
    #[serde(default)]
    pub moves: Vec<ReplicaMove>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub decision_id: Uuid,
    pub observation_epoch: Uuid,
    pub scheduling_signature: String,
    pub policy_signature: String,
    pub config_fingerprint: String,
    pub outcome: DecisionOutcome,
    pub stage: DecisionStage,
    pub plan_version: Option<String>,
    pub summary: String,
    pub reason_codes: Vec<String>,
    #[serde(flatten)]
    pub facts: Facts,
}

#[derive(Deserialize)]
struct Details {
    observation_epoch: Option<Uuid>,
    scheduling_signature: Option<String>,
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(flatten)]
    facts: Facts,
}

impl Decision {
    pub fn candidate(candidate: &Candidate, epoch: Uuid) -> Result<Self> {
        let details: Details = serde_json::from_value(candidate.decision_details.clone())?;
        let moved = details
            .facts
            .moved_replicas
            .context("candidate movement evidence missing")?;
        let added = details
            .facts
            .new_replicas
            .context("candidate new-replica evidence missing")?;
        ensure!(
            details.facts.retained_replicas.is_some()
                && details.facts.pre_plan_free_memory_mib.is_some()
                && details.facts.pre_plan_largest_gap_mib.is_some()
                && details.facts.moves.len() == moved + added,
            "incomplete candidate evidence"
        );
        Ok(Self {
            decision_id: candidate.decision_id,
            observation_epoch: epoch,
            scheduling_signature: candidate.scheduling_signature.clone(),
            policy_signature: candidate.policy_signature.clone(),
            config_fingerprint: candidate.config_fingerprint.clone(),
            outcome: DecisionOutcome::Feasible,
            stage: DecisionStage::Candidate,
            plan_version: None,
            summary: format!(
                "Complete plan for {} replicas: {moved} existing replicas move and {added} new replicas are placed. Not yet saved.",
                candidate.assignments.len()
            ),
            reason_codes: details.reason_codes,
            facts: details.facts,
        })
    }

    pub fn committed(plan: &Plan, epoch: Uuid) -> Result<Self> {
        let details: Details = serde_json::from_value(plan.decision_details.clone())?;
        let summary = match (details.facts.moved_replicas, details.facts.new_replicas) {
            (Some(moved), Some(added)) => format!(
                "Plan v{} was observed in the committed allocation input: {moved} existing replicas move and {added} new replicas are placed. Application and fresh reports are separate.",
                plan.plan_version
            ),
            _ => format!(
                "Plan v{} was observed in the committed allocation input. Original detailed placement evidence is unavailable.",
                plan.plan_version
            ),
        };
        Ok(Self {
            decision_id: plan.decision_id,
            observation_epoch: details.observation_epoch.unwrap_or(epoch),
            scheduling_signature: details.scheduling_signature.unwrap_or_default(),
            policy_signature: plan.policy_signature.clone(),
            config_fingerprint: plan.config_fingerprint.clone(),
            outcome: DecisionOutcome::Feasible,
            stage: DecisionStage::Committed,
            plan_version: Some(plan.plan_version.to_string()),
            summary,
            reason_codes: details.reason_codes,
            facts: details.facts,
        })
    }

    pub fn diagnostic(
        epoch: Uuid,
        input: &crate::wire::SolveInput,
        outcome: DecisionOutcome,
        summary: String,
        reason: &str,
    ) -> Result<Self> {
        Ok(Self {
            decision_id: Uuid::new_v4(),
            observation_epoch: epoch,
            scheduling_signature: input.signature()?,
            policy_signature: input.policy.policy_signature.clone(),
            config_fingerprint: input.configuration.fingerprint()?,
            outcome,
            stage: DecisionStage::Diagnostic,
            plan_version: None,
            summary,
            reason_codes: vec![reason.into()],
            facts: Facts::default(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PolicyRow {
    pub id: String,
    pub workload_id: Uuid,
    pub cluster_id: String,
    pub region: String,
    pub data_profile_id: String,
    pub purpose: String,
    pub policy_id: String,
    pub authority_ref: String,
    pub policy_revision: Option<String>,
    pub allowed_regions: Vec<String>,
    pub observation_epoch: Uuid,
    pub input_fingerprint: String,
    pub policy_signature: String,
    pub authorization: Authorization,
    pub reasons: Vec<String>,
    pub current: bool,
    pub error: Option<String>,
}

pub fn policy_rows(
    configuration: &Configuration,
    assessment: Option<&Assessment>,
    epoch: Uuid,
) -> Result<Vec<PolicyRow>> {
    if let Some(assessment) = assessment {
        assessment.validate_batch(configuration)?;
    }
    let mut rows = Vec::new();
    for workload in configuration.workloads.values() {
        let data = configuration.data_profiles.get(&workload.data_profile_id);
        let policy = data.and_then(|data| configuration.policies.get(&data.policy_id));
        for cluster in configuration.clusters.values() {
            let id = pair_key(workload.workload_id, &cluster.cluster_id);
            let pair = assessment.and_then(|assessment| assessment.pairs.get(&id));
            rows.push(PolicyRow {
                id,
                workload_id: workload.workload_id,
                cluster_id: cluster.cluster_id.clone(),
                region: cluster.region.clone(),
                data_profile_id: workload.data_profile_id.clone(),
                purpose: workload.purpose.clone(),
                policy_id: data
                    .map(|data| data.policy_id.clone())
                    .unwrap_or_else(|| "unresolved".into()),
                authority_ref: policy
                    .map(|policy| policy.authority_ref.clone())
                    .unwrap_or_default(),
                policy_revision: policy.map(|policy| policy.revision.to_string()),
                allowed_regions: policy
                    .map(|policy| policy.allowed_regions.iter().cloned().collect())
                    .unwrap_or_default(),
                observation_epoch: epoch,
                input_fingerprint: pair
                    .map(|pair| pair.input_fingerprint.clone())
                    .unwrap_or_default(),
                policy_signature: assessment
                    .map(|assessment| assessment.policy_signature.clone())
                    .unwrap_or_default(),
                authorization: pair
                    .map(|pair| pair.authorization.clone())
                    .unwrap_or(Authorization::Unknown),
                reasons: pair
                    .map(|pair| pair.reasons.clone())
                    .unwrap_or_else(|| vec!["policy-input-pending".into()]),
                current: pair.is_some_and(|pair| pair.error.is_none()),
                error: pair.map(|pair| pair.error.clone()).unwrap_or_else(|| {
                    Some("Waiting for a complete current policy assessment".into())
                }),
            });
        }
    }
    Ok(rows)
}
