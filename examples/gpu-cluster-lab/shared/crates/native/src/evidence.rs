use anyhow::{ensure, Context, Result};
use gpu_contracts::{planning::ReplicaMove, Candidate, Configuration, Plan};
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
        configuration: &Configuration,
        policy_signature: &str,
        scheduling_signature: &str,
        outcome: DecisionOutcome,
        summary: String,
        reason: &str,
    ) -> Result<Self> {
        Ok(Self {
            decision_id: Uuid::new_v4(),
            observation_epoch: epoch,
            scheduling_signature: scheduling_signature.into(),
            policy_signature: policy_signature.into(),
            config_fingerprint: configuration.fingerprint()?,
            outcome,
            stage: DecisionStage::Diagnostic,
            plan_version: None,
            summary,
            reason_codes: vec![reason.into()],
            facts: Facts::default(),
        })
    }
}
