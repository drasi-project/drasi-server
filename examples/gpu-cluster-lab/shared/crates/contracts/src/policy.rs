//! Placement eligibility decided by the `drasi.standard/policy` transformer.
//!
//! The policy plugin emits one `PlacementEligibility` row per workload/cluster
//! pair. Consumers rebuild a complete, current [`Assessment`] from those rows
//! with [`Assessment::from_decisions`] before acting on it.

use crate::{hash, Assignment, Cluster, Configuration, Workload};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const PLACEMENT_REGO: &str = include_str!("../../../policies/placement.rego");
pub const ELIGIBILITY_REGO: &str = include_str!("../../../policies/eligibility.rego");
pub const REASONS: [&str; 5] = [
    "region-not-permitted",
    "purpose-not-permitted",
    "classification-not-permitted",
    "customer-mismatch",
    "invalid-policy-context",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Authorization {
    Allow,
    Deny,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Eligibility {
    pub id: String,
    pub workload_id: Uuid,
    pub cluster_id: String,
    /// The pair's [`context_key`] when the decision was made.
    pub input_fingerprint: String,
    pub authorization: Authorization,
    pub reasons: Vec<String>,
    pub error: Option<String>,
}

/// A `PlacementEligibility` row as published by the policy transformer.
#[derive(Clone, Debug, Deserialize)]
pub struct Decision {
    pub id: String,
    pub workload_id: Uuid,
    pub cluster_id: String,
    pub context_key: String,
    pub authorization: Option<String>,
    #[serde(default)]
    pub reasons: Option<Vec<String>>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub policy_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assessment {
    pub policy_signature: String,
    pub policy_bundle_hash: String,
    pub pairs: BTreeMap<String, Eligibility>,
}

pub fn pair_key(workload: Uuid, cluster: &str) -> String {
    format!("{workload}/{cluster}")
}

/// Identifies the revisions of every input a pair's policy decision reads.
///
/// `placement-pairs.cypher` computes the same key, so a decision whose key
/// differs from the current configuration's is stale. Revisions increase on
/// every row change. The key starts with the pair key, so it also identifies
/// the pair (execution evidence joins on it).
pub fn context_key(config: &Configuration, w: &Workload, c: &Cluster) -> String {
    let profile = config.data_profiles.get(&w.data_profile_id);
    let policy = profile.and_then(|d| config.policies.get(&d.policy_id));
    format!(
        "{}@{}:{}:{}:{}:{}:{}",
        pair_key(w.workload_id, &c.cluster_id),
        w.revision,
        w.data_profile_id,
        profile.map_or("-".into(), |d| d.revision.to_string()),
        profile.map_or("-", |d| d.policy_id.as_str()),
        policy.map_or("-".into(), |p| p.revision.to_string()),
        c.revision
    )
}

impl Assessment {
    /// Rebuilds the complete assessment from per-pair decisions, rejecting
    /// incomplete or stale batches.
    pub fn from_decisions<'a>(
        config: &Configuration,
        decisions: impl IntoIterator<Item = &'a Decision>,
    ) -> Result<Self> {
        let mut pairs = BTreeMap::new();
        let mut bundle = None;
        for d in decisions {
            let key = pair_key(d.workload_id, &d.cluster_id);
            ensure!(d.id == key, "policy pair identity mismatch");
            let (Some(w), Some(c)) = (
                config.workloads.get(&d.workload_id),
                config.clusters.get(&d.cluster_id),
            ) else {
                continue;
            };
            ensure!(
                d.context_key == context_key(config, w, c),
                "stale policy pair"
            );
            let hash = d.policy_hash.clone().context("policy hash missing")?;
            ensure!(
                bundle.get_or_insert_with(|| hash.clone()) == &hash,
                "mixed policy bundles"
            );
            let reasons = d.reasons.clone().unwrap_or_default();
            ensure!(
                reasons.iter().all(|r| REASONS.contains(&r.as_str())),
                "invalid policy reason"
            );
            let (authorization, error) = match (d.status.as_deref(), d.authorization.as_deref()) {
                (Some("decided"), Some("allow")) if reasons.is_empty() => {
                    (Authorization::Allow, None)
                }
                (Some("decided"), Some("deny")) if !reasons.is_empty() => {
                    (Authorization::Deny, None)
                }
                (Some("decided"), Some("unknown")) => (
                    Authorization::Unknown,
                    Some(reasons.join(", ")).filter(|s| !s.is_empty()),
                ),
                (status, _) => (
                    Authorization::Unknown,
                    Some(
                        d.error
                            .clone()
                            .unwrap_or_else(|| format!("policy {}", status.unwrap_or("missing"))),
                    ),
                ),
            };
            let error = match authorization {
                Authorization::Unknown => {
                    Some(error.unwrap_or_else(|| "invalid-policy-context".into()))
                }
                _ => None,
            };
            ensure!(
                pairs
                    .insert(
                        key.clone(),
                        Eligibility {
                            id: key,
                            workload_id: d.workload_id,
                            cluster_id: d.cluster_id.clone(),
                            input_fingerprint: d.context_key.clone(),
                            authorization,
                            reasons,
                            error,
                        },
                    )
                    .is_none(),
                "duplicate policy pair"
            );
        }
        let bundle = bundle.unwrap_or_default();
        let assessment = Self {
            policy_signature: signature(&bundle, &pairs)?,
            policy_bundle_hash: bundle,
            pairs,
        };
        assessment.validate_batch(config)?;
        Ok(assessment)
    }

    pub fn validate_batch(&self, config: &Configuration) -> Result<()> {
        ensure!(
            self.policy_signature == signature(&self.policy_bundle_hash, &self.pairs)?,
            "policy signature mismatch"
        );
        ensure!(
            self.pairs.len() == config.workloads.len() * config.clusters.len(),
            "incomplete policy batch"
        );
        for w in config.workloads.values() {
            for c in config.clusters.values() {
                let key = pair_key(w.workload_id, &c.cluster_id);
                let p = self.pairs.get(&key).context("missing policy pair")?;
                ensure!(
                    p.id == key && p.workload_id == w.workload_id && p.cluster_id == c.cluster_id,
                    "policy pair identity mismatch"
                );
                ensure!(
                    p.input_fingerprint == context_key(config, w, c),
                    "stale policy pair"
                );
                if p.authorization == Authorization::Unknown {
                    ensure!(p.error.is_some(), "unknown policy must include its failure");
                } else {
                    ensure!(p.error.is_none(), "known policy has an error");
                }
            }
        }
        Ok(())
    }

    pub fn validate_current(&self, config: &Configuration) -> Result<()> {
        self.validate_batch(config)?;
        ensure!(
            self.pairs
                .values()
                .all(|p| p.authorization != Authorization::Unknown),
            "policy batch contains unknown authorization"
        );
        Ok(())
    }

    pub fn allows(&self, workload: Uuid, cluster: &str) -> bool {
        self.pairs
            .get(&pair_key(workload, cluster))
            .is_some_and(|p| p.authorization == Authorization::Allow && p.error.is_none())
    }

    pub fn validate_plan(&self, config: &Configuration, assignments: &[Assignment]) -> Result<()> {
        self.validate_current(config)?;
        for a in assignments {
            let g = config.gpus.get(&a.gpu_id).context("unknown GPU")?;
            ensure!(
                self.allows(a.workload_id, &g.cluster_id),
                "target prohibited by policy"
            );
        }
        Ok(())
    }

    /// Recomputes the signature after a test or caller edits `pairs`.
    pub fn resign(&mut self) -> Result<()> {
        self.policy_signature = signature(&self.policy_bundle_hash, &self.pairs)?;
        Ok(())
    }
}

pub const POLICY_QUERY: &str = "data.gpu.eligibility.result";

/// The policy part of the `drasi.standard/policy` instance configuration:
/// exactly the fields the transformer hashes into `policy_hash`.
pub fn policy_bundle() -> serde_json::Value {
    serde_json::json!({
        "rego_version": "v1",
        "modules": [
            {"name": "placement.rego", "source": PLACEMENT_REGO},
            {"name": "eligibility.rego", "source": ELIGIBILITY_REGO},
        ],
        "query": POLICY_QUERY,
        "data": {},
    })
}

/// The `policy_hash` the policy transformer stamps on decisions made with
/// [`policy_bundle`]: SHA-256 over its canonical (key-sorted) JSON.
pub fn bundle_hash() -> String {
    use sha2::{Digest, Sha256};
    fn canonical(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort_unstable();
                out.push('{');
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::String(key.clone()).to_string());
                    out.push(':');
                    canonical(&map[key], out);
                }
                out.push('}');
            }
            serde_json::Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    canonical(item, out);
                }
                out.push(']');
            }
            scalar => out.push_str(&scalar.to_string()),
        }
    }
    let mut text = String::new();
    canonical(&policy_bundle(), &mut text);
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::from("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The lab's batch signature over every pair decision and the policy bundle.
pub fn signature(bundle: &str, pairs: &BTreeMap<String, Eligibility>) -> Result<String> {
    hash("gpu-policy-assessment-v2", &(bundle, pairs))
}

/// A reference model of `eligibility.rego`, used to build expected decisions
/// in tests. The live demo always decides through the policy transformer.
pub fn reference_decisions(config: &Configuration, bundle: &str) -> Vec<Decision> {
    let mut rows = Vec::new();
    for w in config.workloads.values() {
        for c in config.clusters.values() {
            let profile = config.data_profiles.get(&w.data_profile_id);
            let policy = profile.and_then(|d| config.policies.get(&d.policy_id));
            let (authorization, reasons) = match (profile, policy) {
                (Some(d), Some(p))
                    if ["synthetic", "restricted"].contains(&d.classification.as_str())
                        && d.revision > 0
                        && !d.authority_ref.is_empty()
                        && ["demo", "customer-support"].contains(&w.purpose.as_str()) =>
                {
                    let mut reasons = Vec::new();
                    if !(p.allowed_regions.contains(&c.region)
                        || p.allowed_regions.iter().eq(["*"].iter()))
                    {
                        reasons.push("region-not-permitted");
                    }
                    if !p.allowed_purposes.contains(&w.purpose) {
                        reasons.push("purpose-not-permitted");
                    }
                    if !p.allowed_classifications.contains(&d.classification) {
                        reasons.push("classification-not-permitted");
                    }
                    if d.customer_id != p.customer_id {
                        reasons.push("customer-mismatch");
                    }
                    reasons.sort_unstable();
                    (if reasons.is_empty() { "allow" } else { "deny" }, reasons)
                }
                _ => ("unknown", vec!["invalid-policy-context"]),
            };
            rows.push(Decision {
                id: pair_key(w.workload_id, &c.cluster_id),
                workload_id: w.workload_id,
                cluster_id: c.cluster_id.clone(),
                context_key: context_key(config, w, c),
                authorization: Some(authorization.into()),
                reasons: Some(reasons.into_iter().map(Into::into).collect()),
                status: Some("decided".into()),
                error: None,
                policy_hash: Some(bundle.into()),
            });
        }
    }
    rows
}

/// The reference assessment of `config`, for tests and seed data. It carries
/// the live [`bundle_hash`], so it equals the transformer's assessment.
pub fn reference_assessment(config: &Configuration) -> Result<Assessment> {
    Assessment::from_decisions(config, &reference_decisions(config, &bundle_hash()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn reference_allows_only_permitted_destinations() -> Result<()> {
        let config = fixtures::load("regional-boundary")?.configuration;
        let a = reference_assessment(&config)?;
        a.validate_current(&config)?;
        assert_eq!(a.pairs.len(), 12);
        for w in config.workloads.keys() {
            assert!(a.allows(*w, "eu-primary"));
            assert!(a.allows(*w, "eu-recovery"));
            assert!(!a.allows(*w, "us-spare"));
        }
        Ok(())
    }

    #[test]
    fn missing_context_is_unknown_and_never_allow() -> Result<()> {
        let mut config = fixtures::load("baseline")?.configuration;
        config.policies.clear();
        let a = Assessment::from_decisions(&config, &reference_decisions(&config, "h"))?;
        assert!(a
            .pairs
            .values()
            .all(|p| p.authorization == Authorization::Unknown && p.error.is_some()));
        assert!(a.validate_current(&config).is_err());
        Ok(())
    }

    #[test]
    fn stale_incomplete_and_mixed_batches_are_rejected() -> Result<()> {
        let mut config = fixtures::load("regional-boundary")?.configuration;
        let old = reference_decisions(&config, "h");
        assert!(Assessment::from_decisions(&config, &old[1..]).is_err());
        let mut mixed = old.clone();
        mixed[0].policy_hash = Some("other".into());
        assert!(Assessment::from_decisions(&config, &mixed).is_err());
        let p = config.policies.get_mut("customer-eu-processing").unwrap();
        p.allowed_regions.clear();
        p.revision += 1;
        assert!(Assessment::from_decisions(&config, &old).is_err());
        let current = Assessment::from_decisions(&config, &reference_decisions(&config, "h"))?;
        current.validate_current(&config)?;
        assert!(current
            .pairs
            .values()
            .all(|p| p.authorization == Authorization::Deny));
        Ok(())
    }

    #[test]
    fn errors_and_undefined_decisions_are_unknown() -> Result<()> {
        let config = fixtures::load("regional-boundary")?.configuration;
        let mut rows = reference_decisions(&config, "h");
        rows[0].status = Some("error".into());
        rows[0].error = Some("timeout".into());
        rows[1].status = Some("undefined".into());
        let a = Assessment::from_decisions(&config, &rows)?;
        assert_eq!(a.pairs[&rows[0].id].error.as_deref(), Some("timeout"));
        assert_eq!(a.pairs[&rows[1].id].authorization, Authorization::Unknown);
        assert!(a.validate_current(&config).is_err());
        Ok(())
    }
}
