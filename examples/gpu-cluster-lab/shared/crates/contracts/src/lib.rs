use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const MEMORY_MIB: u32 = 81920;
pub const PLANNING_UNITS: u32 = 85;
pub const REGIONS: [&str; 3] = ["westeurope", "northeurope", "eastus"];
pub const UI_QUERIES: [&str; 9] = [
    "ui-gpus",
    "ui-workloads",
    "ui-placements",
    "ui-resilience",
    "ui-decisions",
    "ui-status",
    "ui-timeline",
    "ui-clusters",
    "ui-policy",
];

pub mod decimal {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Number {
            Text(String),
            Integer(u64),
        }
        match Number::deserialize(deserializer)? {
            Number::Integer(value) => Ok(value),
            Number::Text(value)
                if !value.is_empty() && value.bytes().all(|v| v.is_ascii_digit()) =>
            {
                value.parse().map_err(D::Error::custom)
            }
            Number::Text(_) => Err(D::Error::custom("expected an unsigned decimal integer")),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub cluster_id: String,
    pub name: String,
    pub region: String,
    #[serde(with = "decimal")]
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub policy_id: String,
    pub name: String,
    pub customer_id: String,
    pub allowed_regions: BTreeSet<String>,
    pub allowed_purposes: BTreeSet<String>,
    pub allowed_classifications: BTreeSet<String>,
    pub authority_ref: String,
    #[serde(with = "decimal")]
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DataProfile {
    pub data_profile_id: String,
    pub customer_id: String,
    pub classification: String,
    pub policy_id: String,
    pub authority_ref: String,
    #[serde(with = "decimal")]
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gpu {
    pub gpu_id: Uuid,
    pub name: String,
    pub cluster_id: String,
    pub host_id: String,
    pub gpu_index: u8,
    pub model: String,
    pub vm_size: String,
    pub nominal_vram_gb: u32,
    pub memory_mib: u32,
    pub compute_units: u32,
    pub failure_domain: String,
    pub scheduling_enabled: bool,
    #[serde(with = "decimal")]
    pub revision: u64,
}

impl Gpu {
    pub fn new(host: &str, cluster: &str, slot: u8) -> Self {
        Self {
            gpu_id: Uuid::new_v4(),
            name: format!("{host}/{slot}"),
            cluster_id: cluster.into(),
            host_id: host.into(),
            gpu_index: slot,
            model: "NVIDIA H100 NVL".into(),
            vm_size: "Standard_NC80adis_H100_v5".into(),
            nominal_vram_gb: 94,
            memory_mib: MEMORY_MIB,
            compute_units: 100,
            failure_domain: host.into(),
            scheduling_enabled: true,
            revision: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub gpu_id: Uuid,
    pub powered_on: bool,
    pub reporting_enabled: bool,
    pub interval_ms: u64,
    pub background_compute_units: u32,
    pub background_memory_mib: u32,
    #[serde(with = "decimal")]
    pub revision: u64,
}

impl Settings {
    pub fn baseline(gpu_id: Uuid) -> Self {
        Self {
            gpu_id,
            powered_on: true,
            reporting_enabled: true,
            interval_ms: 1000,
            background_compute_units: 10,
            background_memory_mib: 0,
            revision: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Workload {
    pub workload_id: Uuid,
    pub name: String,
    pub model_ref: String,
    pub profile_id: String,
    pub data_profile_id: String,
    pub purpose: String,
    pub replicas: u32,
    pub memory_mib_per_replica: u32,
    pub compute_units_per_replica: u32,
    pub allowed_gpu_models: BTreeSet<String>,
    pub spread_across_domains: bool,
    #[serde(with = "decimal")]
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServingProfile {
    pub profile_id: String,
    pub model_ref: String,
    pub memory_mib: u32,
    pub compute_units: u32,
    pub bounds: String,
}

pub fn serving_profiles() -> Vec<ServingProfile> {
    [
        (
            "assistant-v1",
            "Qwen/Qwen2.5-32B-Instruct",
            77824,
            55,
            "BF16; 8 sequences; 4096 total tokens each",
        ),
        (
            "chat-v1",
            "meta-llama/Llama-3.1-8B-Instruct",
            24576,
            30,
            "BF16; 4 sequences; 8192 total tokens each",
        ),
        (
            "embeddings-v1",
            "BAAI/bge-m3",
            4096,
            20,
            "FP16; 512 tokens; batch 16",
        ),
        (
            "reranker-v1",
            "BAAI/bge-reranker-v2-m3",
            4096,
            25,
            "FP16; 512 tokens per pair; batch 8",
        ),
    ]
    .into_iter()
    .map(|(id, model, memory, demand, bounds)| ServingProfile {
        profile_id: id.into(),
        model_ref: model.into(),
        memory_mib: memory,
        compute_units: demand,
        bounds: bounds.into(),
    })
    .collect()
}

impl Workload {
    pub fn from_profile(
        name: &str,
        profile: &str,
        replicas: u32,
        data: &str,
        purpose: &str,
    ) -> Result<Self> {
        let p = serving_profiles()
            .into_iter()
            .find(|p| p.profile_id == profile)
            .ok_or_else(|| anyhow::anyhow!("unknown serving profile {profile}"))?;
        Ok(Self {
            workload_id: Uuid::new_v4(),
            name: name.into(),
            model_ref: p.model_ref,
            profile_id: p.profile_id,
            data_profile_id: data.into(),
            purpose: purpose.into(),
            replicas,
            memory_mib_per_replica: p.memory_mib,
            compute_units_per_replica: p.compute_units,
            allowed_gpu_models: BTreeSet::from(["NVIDIA H100 NVL".into()]),
            spread_across_domains: true,
            revision: 1,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub clusters: BTreeMap<String, Cluster>,
    pub policies: BTreeMap<String, Policy>,
    pub data_profiles: BTreeMap<String, DataProfile>,
    pub gpus: BTreeMap<Uuid, Gpu>,
    pub workloads: BTreeMap<Uuid, Workload>,
}

pub fn hash<T: Serialize + ?Sized>(format: &str, value: &T) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(format.as_bytes());
    digest.update([0]);
    digest.update(serde_json::to_vec(value)?);
    Ok(format!("{:x}", digest.finalize()))
}

pub fn nonempty(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 256,
        "expected 1-256 nonblank characters"
    );
    ensure!(
        !value.chars().any(char::is_control),
        "control characters are not permitted"
    );
    Ok(())
}

impl Configuration {
    pub fn fingerprint(&self) -> Result<String> {
        hash("gpu-config-v1", self)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.clusters.len() <= 8
                && self.gpus.len() <= 16
                && self.workloads.len() <= 32
                && self.policies.len() <= 32
                && self.data_profiles.len() <= 32,
            "fleet record limit exceeded"
        );
        ensure!(
            self.workloads
                .values()
                .map(|w| u64::from(w.replicas))
                .sum::<u64>()
                <= 32,
            "replica limit exceeded"
        );
        let mut hosts = BTreeMap::new();
        let mut slots = BTreeSet::new();
        for (id, c) in &self.clusters {
            nonempty(id)?;
            nonempty(&c.name)?;
            ensure!(
                id == &c.cluster_id && REGIONS.contains(&c.region.as_str()) && c.revision > 0,
                "invalid cluster"
            );
        }
        for (id, p) in &self.policies {
            nonempty(id)?;
            nonempty(&p.name)?;
            nonempty(&p.customer_id)?;
            nonempty(&p.authority_ref)?;
            ensure!(
                id == &p.policy_id && p.revision > 0,
                "invalid policy identity"
            );
            ensure!(
                p.allowed_regions
                    .iter()
                    .all(|r| REGIONS.contains(&r.as_str()) || r == "*"),
                "unknown region"
            );
            ensure!(
                p.allowed_purposes
                    .iter()
                    .all(|v| ["demo", "customer-support"].contains(&v.as_str())),
                "unknown purpose"
            );
            ensure!(
                p.allowed_classifications
                    .iter()
                    .all(|v| ["synthetic", "restricted"].contains(&v.as_str())),
                "unknown classification"
            );
            if p.allowed_regions.contains("*") {
                ensure!(
                    p.policy_id == "demo-permissive"
                        && p.customer_id == "demo"
                        && p.allowed_regions.len() == 1
                        && p.allowed_purposes == BTreeSet::from(["demo".into()])
                        && p.allowed_classifications == BTreeSet::from(["synthetic".into()]),
                    "invalid wildcard scope"
                );
            }
        }
        for (id, d) in &self.data_profiles {
            nonempty(id)?;
            nonempty(&d.customer_id)?;
            nonempty(&d.authority_ref)?;
            ensure!(
                id == &d.data_profile_id
                    && d.revision > 0
                    && ["synthetic", "restricted"].contains(&d.classification.as_str()),
                "invalid data profile"
            );
            let p = self
                .policies
                .get(&d.policy_id)
                .ok_or_else(|| anyhow::anyhow!("missing policy {}", d.policy_id))?;
            ensure!(
                p.customer_id == d.customer_id,
                "data/policy customer mismatch"
            );
        }
        for (id, g) in &self.gpus {
            nonempty(&g.name)?;
            nonempty(&g.host_id)?;
            ensure!(
                id == &g.gpu_id && g.revision > 0 && self.clusters.contains_key(&g.cluster_id),
                "invalid GPU identity/cluster"
            );
            ensure!(
                g.gpu_index < 2 && slots.insert((&g.host_id, g.gpu_index)),
                "invalid or duplicate GPU slot"
            );
            ensure!(
                g.model == "NVIDIA H100 NVL"
                    && g.vm_size == "Standard_NC80adis_H100_v5"
                    && g.nominal_vram_gb == 94
                    && g.memory_mib == MEMORY_MIB
                    && g.compute_units == 100
                    && g.failure_domain == g.host_id,
                "unsupported hardware profile"
            );
            if let Some(cluster) = hosts.insert(&g.host_id, &g.cluster_id) {
                ensure!(cluster == &g.cluster_id, "worker cannot span clusters");
            }
        }
        ensure!(hosts.len() <= 8, "worker limit exceeded");
        for (id, w) in &self.workloads {
            nonempty(&w.name)?;
            ensure!(
                id == &w.workload_id && w.revision > 0 && w.replicas <= 32,
                "invalid workload identity/count"
            );
            ensure!(
                self.data_profiles.contains_key(&w.data_profile_id),
                "missing data profile"
            );
            ensure!(
                ["demo", "customer-support"].contains(&w.purpose.as_str()),
                "unknown purpose"
            );
            ensure!(
                !w.allowed_gpu_models.is_empty()
                    && w.allowed_gpu_models
                        .iter()
                        .all(|m| m == "*" || m == "NVIDIA H100 NVL"),
                "invalid hardware compatibility"
            );
            ensure!(
                (1..=MEMORY_MIB).contains(&w.memory_mib_per_replica)
                    && (1..=200).contains(&w.compute_units_per_replica),
                "invalid workload reservation"
            );
            if w.profile_id != "custom" {
                let p = serving_profiles()
                    .into_iter()
                    .find(|p| p.profile_id == w.profile_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown serving profile"))?;
                ensure!(
                    w.model_ref == p.model_ref
                        && w.memory_mib_per_replica == p.memory_mib
                        && w.compute_units_per_replica == p.compute_units,
                    "serving profile/reservation mismatch"
                );
            } else {
                nonempty(&w.model_ref)?;
            }
        }
        Ok(())
    }
}

pub fn validate_settings(
    config: &Configuration,
    settings: &BTreeMap<Uuid, Settings>,
) -> Result<()> {
    ensure!(
        settings.len() == config.gpus.len(),
        "missing/extra generation settings"
    );
    for (id, s) in settings {
        ensure!(
            id == &s.gpu_id && config.gpus.contains_key(id) && s.revision > 0,
            "invalid settings identity"
        );
        ensure!(
            (250..=2000).contains(&s.interval_ms)
                && s.background_compute_units <= 200
                && s.background_memory_mib <= MEMORY_MIB,
            "invalid generation settings"
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub workload_id: Uuid,
    pub replica_index: u32,
    pub gpu_id: Uuid,
    pub model_ref: String,
    pub profile_id: String,
    pub data_profile_id: String,
    pub purpose: String,
    pub memory_mib: u32,
    pub compute_units: u32,
    #[serde(with = "decimal")]
    pub workload_revision: u64,
}

impl Assignment {
    pub fn new(w: &Workload, replica_index: u32, gpu_id: Uuid) -> Self {
        Self {
            workload_id: w.workload_id,
            replica_index,
            gpu_id,
            model_ref: w.model_ref.clone(),
            profile_id: w.profile_id.clone(),
            data_profile_id: w.data_profile_id.clone(),
            purpose: w.purpose.clone(),
            memory_mib: w.memory_mib_per_replica,
            compute_units: w.compute_units_per_replica,
            workload_revision: w.revision,
        }
    }
    pub fn key(&self) -> (Uuid, u32) {
        (self.workload_id, self.replica_index)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    pub gpu_id: Uuid,
    pub eligible: bool,
    pub memory_mib: u32,
    pub compute_units: u32,
}

impl Capacity {
    pub fn from_observation(
        gpu: &Gpu,
        fresh: bool,
        background_memory: u32,
        background_demand: u32,
    ) -> Self {
        Self {
            gpu_id: gpu.gpu_id,
            eligible: gpu.scheduling_enabled && fresh,
            memory_mib: gpu.memory_mib.saturating_sub(background_memory),
            compute_units: PLANNING_UNITS.saturating_sub(background_demand),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub fleet_id: String,
    #[serde(with = "decimal")]
    pub plan_version: u64,
    pub decision_id: Uuid,
    pub config_fingerprint: String,
    pub policy_signature: String,
    pub policy_bundle_hash: String,
    pub assignments: Vec<Assignment>,
    pub decision_details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub decision_id: Uuid,
    #[serde(with = "decimal")]
    pub expected_plan_version: u64,
    pub config_fingerprint: String,
    pub policy_signature: String,
    pub policy_bundle_hash: String,
    pub scheduling_signature: String,
    pub assignments: Vec<Assignment>,
    pub decision_details: serde_json::Value,
}

pub fn validate_assignments(
    config: &Configuration,
    capacities: &BTreeMap<Uuid, Capacity>,
    assignments: &[Assignment],
) -> Result<()> {
    validate_assignment_set(config, capacities, assignments, true)
}

/// Validates existing execution, not a complete plan eligible for persistence.
pub fn validate_execution_subset(
    config: &Configuration,
    capacities: &BTreeMap<Uuid, Capacity>,
    assignments: &[Assignment],
) -> Result<()> {
    validate_assignment_set(config, capacities, assignments, false)
}

fn validate_assignment_set(
    config: &Configuration,
    capacities: &BTreeMap<Uuid, Capacity>,
    assignments: &[Assignment],
    complete: bool,
) -> Result<()> {
    config.validate()?;
    let requested: BTreeSet<_> = config
        .workloads
        .values()
        .flat_map(|w| (0..w.replicas).map(move |i| (w.workload_id, i)))
        .collect();
    let mut actual = BTreeSet::new();
    let mut totals: BTreeMap<Uuid, (u64, u64)> = BTreeMap::new();
    let mut domains = BTreeSet::new();
    for a in assignments {
        ensure!(actual.insert(a.key()), "duplicate replica");
        let w = config
            .workloads
            .get(&a.workload_id)
            .ok_or_else(|| anyhow::anyhow!("unknown workload"))?;
        let g = config
            .gpus
            .get(&a.gpu_id)
            .ok_or_else(|| anyhow::anyhow!("unknown GPU"))?;
        let c = capacities
            .get(&a.gpu_id)
            .ok_or_else(|| anyhow::anyhow!("missing GPU capacity"))?;
        ensure!(
            a == &Assignment::new(w, a.replica_index, a.gpu_id),
            "outdated assignment context/resources"
        );
        ensure!(
            c.gpu_id == a.gpu_id && c.eligible && g.scheduling_enabled,
            "ineligible target"
        );
        ensure!(
            w.allowed_gpu_models.contains("*") || w.allowed_gpu_models.contains(&g.model),
            "incompatible target"
        );
        ensure!(
            !w.spread_across_domains || domains.insert((w.workload_id, &g.failure_domain)),
            "replica separation violated"
        );
        let t = totals.entry(a.gpu_id).or_default();
        t.0 += u64::from(a.memory_mib);
        t.1 += u64::from(a.compute_units);
        ensure!(
            t.0 <= u64::from(c.memory_mib.min(g.memory_mib))
                && t.1 <= u64::from(c.compute_units.min(PLANNING_UNITS)),
            "capacity exceeded"
        );
    }
    if complete {
        ensure!(
            actual == requested,
            "plan must cover every requested replica exactly once"
        );
    } else {
        ensure!(
            actual.is_subset(&requested),
            "execution includes an unrequested replica"
        );
    }
    Ok(())
}

pub mod fixtures;

pub fn require_schema_version(value: u32) -> Result<()> {
    if value != 1 {
        bail!("unsupported schema version {value}");
    }
    Ok(())
}
