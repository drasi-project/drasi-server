use anyhow::{ensure, Context, Result};
use gpu_contracts::*;
use regorus::{utils::limits::ExecutionTimerConfig, Engine};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, num::NonZeroU32, time::Duration};
use uuid::Uuid;

pub const BUNDLE: &str = include_str!("../../../policies/placement.rego");
pub fn bundle_hash() -> Result<String> {
    hash("gpu-policy-bundle-v1", BUNDLE)
}

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
    pub input_fingerprint: String,
    pub authorization: Authorization,
    pub reasons: Vec<String>,
    pub error: Option<String>,
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

pub fn context(config: &Configuration, w: &Workload, c: &Cluster) -> Result<serde_json::Value> {
    let d = config
        .data_profiles
        .get(&w.data_profile_id)
        .context("missing data profile")?;
    let p = config
        .policies
        .get(&d.policy_id)
        .context("missing policy")?;
    // Validate policy parameters independently so unrelated bad inventory cannot authorize a pair.
    let mut policy_config = Configuration::default();
    policy_config
        .policies
        .insert(p.policy_id.clone(), p.clone());
    policy_config
        .clusters
        .insert(c.cluster_id.clone(), c.clone());
    policy_config.validate()?;
    ensure!(
        ["synthetic", "restricted"].contains(&d.classification.as_str())
            && d.revision > 0
            && !d.authority_ref.is_empty()
            && ["demo", "customer-support"].contains(&w.purpose.as_str()),
        "invalid policy context"
    );
    Ok(serde_json::json!({
        "workload": {"workload_id": w.workload_id, "data_profile_id": w.data_profile_id, "purpose": w.purpose},
        "data_profile": d, "policy": p, "cluster": c,
    }))
}

pub fn input_fingerprint(config: &Configuration, w: &Workload, c: &Cluster) -> Result<String> {
    hash(
        "gpu-policy-input-v1",
        &(bundle_hash()?, context(config, w, c)?),
    )
}

pub struct Evaluator {
    template: Engine,
}

impl Evaluator {
    pub fn new() -> Result<Self> {
        let mut engine = Engine::new();
        engine.set_rego_v0(false);
        engine.set_execution_timer_config(ExecutionTimerConfig {
            limit: Duration::from_millis(50),
            check_interval: NonZeroU32::new(64).context("zero interval")?,
        });
        engine.add_policy("placement.rego".into(), BUNDLE.into())?;
        Ok(Self { template: engine })
    }

    fn evaluate_pair(
        &self,
        config: &Configuration,
        w: &Workload,
        c: &Cluster,
    ) -> Result<Eligibility> {
        let input = context(config, w, c)?;
        let fingerprint = hash("gpu-policy-input-v1", &(bundle_hash()?, &input))?;
        let mut engine = self.template.clone();
        engine.set_input(regorus::Value::from_json_str(&serde_json::to_string(
            &input,
        )?)?);
        let result = engine.eval_rule("data.gpu.placement.decision".into())?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Decision {
            allow: bool,
            reasons: Vec<String>,
        }
        let decision: Decision = serde_json::from_str(&result.to_json_str()?)?;
        ensure!(
            decision.allow == decision.reasons.is_empty(),
            "inconsistent policy result"
        );
        ensure!(
            decision.reasons.iter().all(|s| [
                "region-not-permitted",
                "purpose-not-permitted",
                "classification-not-permitted",
                "customer-mismatch"
            ]
            .contains(&s.as_str())),
            "invalid policy reason"
        );
        Ok(Eligibility {
            id: pair_key(w.workload_id, &c.cluster_id),
            workload_id: w.workload_id,
            cluster_id: c.cluster_id.clone(),
            input_fingerprint: fingerprint,
            authorization: if decision.allow {
                Authorization::Allow
            } else {
                Authorization::Deny
            },
            reasons: decision.reasons,
            error: None,
        })
    }

    pub fn evaluate(&self, config: &Configuration) -> Result<Assessment> {
        ensure!(
            config.workloads.len() <= 32 && config.clusters.len() <= 8,
            "policy input bound exceeded"
        );
        let mut pairs = BTreeMap::new();
        for w in config.workloads.values() {
            for c in config.clusters.values() {
                let id = pair_key(w.workload_id, &c.cluster_id);
                let entry = self
                    .evaluate_pair(config, w, c)
                    .unwrap_or_else(|e| Eligibility {
                        id: id.clone(),
                        workload_id: w.workload_id,
                        cluster_id: c.cluster_id.clone(),
                        input_fingerprint: String::new(),
                        authorization: Authorization::Unknown,
                        reasons: vec!["invalid-policy-context".into()],
                        error: Some(format!("{e:#}")),
                    });
                pairs.insert(id, entry);
            }
        }
        let bundle = bundle_hash()?;
        let signature = hash("gpu-policy-assessment-v1", &(&bundle, &pairs))?;
        Ok(Assessment {
            policy_signature: signature,
            policy_bundle_hash: bundle,
            pairs,
        })
    }
}

impl Assessment {
    pub fn validate_batch(&self, config: &Configuration) -> Result<()> {
        ensure!(
            self.policy_bundle_hash == bundle_hash()?,
            "policy bundle mismatch"
        );
        ensure!(
            self.policy_signature
                == hash(
                    "gpu-policy-assessment-v1",
                    &(&self.policy_bundle_hash, &self.pairs)
                )?,
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
                if p.authorization == Authorization::Unknown {
                    ensure!(p.error.is_some(), "unknown policy must include its failure");
                } else {
                    ensure!(p.error.is_none(), "known policy has an error");
                    ensure!(
                        p.input_fingerprint == input_fingerprint(config, w, c)?,
                        "stale policy pair"
                    );
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_rego_allows_only_permitted_destinations() -> Result<()> {
        let config = fixtures::load("regional-boundary")?.configuration;
        let a = Evaluator::new()?.evaluate(&config)?;
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
    fn synthetic_and_eu_data_coexist_with_distinct_region_and_purpose_decisions() -> Result<()> {
        let mut config = fixtures::load("regional-boundary")?.configuration;
        let synthetic = *config.workloads.keys().next().unwrap();
        let workload = config.workloads.get_mut(&synthetic).unwrap();
        workload.data_profile_id = "demo-open".into();
        workload.purpose = "demo".into();
        config.validate()?;
        let rules = config.policies.clone();
        let engine = Evaluator::new()?;
        let assessment = engine.evaluate(&config)?;
        assessment.validate_current(&config)?;
        for id in config.workloads.keys() {
            assert!(assessment.allows(*id, "eu-primary"));
            assert!(assessment.allows(*id, "eu-recovery"));
            assert_eq!(assessment.allows(*id, "us-spare"), *id == synthetic);
        }
        config.workloads.get_mut(&synthetic).unwrap().purpose = "customer-support".into();
        assert!(assessment.validate_current(&config).is_err());
        let changed = engine.evaluate(&config)?;
        for pair in changed
            .pairs
            .values()
            .filter(|pair| pair.workload_id == synthetic)
        {
            assert_eq!(pair.authorization, Authorization::Deny);
            assert_eq!(pair.reasons, vec!["purpose-not-permitted"]);
        }
        assert_eq!(
            config.data_profiles["demo-open"].classification,
            "synthetic"
        );
        assert_eq!(config.policies, rules);
        Ok(())
    }
    #[test]
    fn missing_context_is_unknown_and_never_allow() -> Result<()> {
        let mut config = fixtures::load("baseline")?.configuration;
        config.policies.clear();
        let a = Evaluator::new()?.evaluate(&config)?;
        assert!(a
            .pairs
            .values()
            .all(|p| p.authorization == Authorization::Unknown && p.error.is_some()));
        assert!(a.validate_current(&config).is_err());
        Ok(())
    }
    #[test]
    fn revocation_and_stale_assessments_are_rejected() -> Result<()> {
        let mut config = fixtures::load("regional-boundary")?.configuration;
        let engine = Evaluator::new()?;
        let old = engine.evaluate(&config)?;
        let p = config.policies.get_mut("customer-eu-processing").unwrap();
        p.allowed_regions.clear();
        p.revision += 1;
        assert!(old.validate_current(&config).is_err());
        let current = engine.evaluate(&config)?;
        current.validate_current(&config)?;
        assert!(current
            .pairs
            .values()
            .all(|p| p.authorization == Authorization::Deny));
        Ok(())
    }

    #[test]
    fn shared_rules_check_workload_facts_and_region_edits_recompute_every_workload() -> Result<()> {
        let mut config = fixtures::load("regional-boundary")?.configuration;
        let engine = Evaluator::new()?;
        let original_rules = config.policies.clone();
        let first = *config.workloads.keys().next().unwrap();
        config.workloads.get_mut(&first).unwrap().purpose = "demo".into();
        let assessment = engine.evaluate(&config)?;
        assessment.validate_current(&config)?;
        for pair in assessment.pairs.values() {
            if pair.workload_id == first {
                assert_eq!(pair.authorization, Authorization::Deny);
                assert!(pair.reasons.contains(&"purpose-not-permitted".into()));
            } else if pair.cluster_id == "eu-primary" {
                assert_eq!(pair.authorization, Authorization::Allow);
            }
        }
        assert_eq!(
            config.policies, original_rules,
            "facts do not edit shared rules"
        );
        config.workloads.get_mut(&first).unwrap().purpose = "customer-support".into();
        config
            .data_profiles
            .get_mut("customer-eu-documents")
            .unwrap()
            .classification = "synthetic".into();
        let assessment = engine.evaluate(&config)?;
        assert!(assessment
            .pairs
            .values()
            .all(|pair| pair.authorization == Authorization::Deny
                && pair
                    .reasons
                    .contains(&"classification-not-permitted".into())));
        config
            .data_profiles
            .get_mut("customer-eu-documents")
            .unwrap()
            .classification = "restricted".into();
        config
            .data_profiles
            .get_mut("customer-eu-documents")
            .unwrap()
            .customer_id = "different-customer".into();
        let mismatched = engine.evaluate(&config)?;
        assert!(mismatched
            .pairs
            .values()
            .all(|pair| pair.authorization == Authorization::Deny
                && pair.reasons.contains(&"customer-mismatch".into())));
        config
            .data_profiles
            .get_mut("customer-eu-documents")
            .unwrap()
            .customer_id = "customer-eu".into();
        let previous = engine.evaluate(&config)?;
        let rule = config.policies.get_mut("customer-eu-processing").unwrap();
        rule.allowed_regions = ["northeurope".into()].into_iter().collect();
        rule.revision += 1;
        assert!(previous.validate_current(&config).is_err());
        let current = engine.evaluate(&config)?;
        current.validate_current(&config)?;
        for workload in config.workloads.keys() {
            assert!(previous.allows(*workload, "eu-primary"));
            assert!(!current.allows(*workload, "eu-primary"));
            assert!(current.allows(*workload, "eu-recovery"));
            assert!(!current.allows(*workload, "us-spare"));
        }
        Ok(())
    }
}
