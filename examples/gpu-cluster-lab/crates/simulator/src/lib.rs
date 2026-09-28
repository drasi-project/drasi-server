use anyhow::{ensure, Context, Result};
use gpu_contracts::*;
use gpu_policy::{input_fingerprint, pair_key, Assessment, Authorization};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Execution {
    Running,
    Suspended,
    Fenced,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enforcement {
    pub id: String,
    pub assignment: Assignment,
    pub state: Execution,
    pub input_fingerprint: Option<String>,
    pub reason: String,
    pub acknowledged_monotonic_ms: u64,
    pub policy_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub schema_version: u32,
    pub gpu_id: Uuid,
    pub observation_epoch: Uuid,
    #[serde(with = "decimal")]
    pub report_sequence: u64,
    pub report_time_ms: u64,
    #[serde(with = "decimal")]
    pub inventory_revision: u64,
    #[serde(with = "decimal")]
    pub telemetry_revision: u64,
    pub applied_plan_version: Option<String>,
    pub background_compute_units: u32,
    pub managed_compute_units: u32,
    pub total_compute_units: u32,
    pub busy_percent: u32,
    pub managed_memory_mib: u32,
    pub background_memory_requested_mib: u32,
    pub background_memory_allocated_mib: u32,
    pub modeled_memory_used_mib: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Application {
    pub applied_plan_version: Option<String>,
    pub attempted_plan_version: Option<String>,
    pub error: Option<String>,
}

pub struct Simulator {
    pub epoch: Uuid,
    config: Configuration,
    settings: BTreeMap<Uuid, Settings>,
    policy: Option<Assessment>,
    desired: Option<Plan>,
    execution: BTreeMap<(Uuid, u32), Enforcement>,
    samples: BTreeMap<Uuid, Sample>,
    deadlines: BTreeMap<Uuid, u64>,
    sequences: BTreeMap<Uuid, u64>,
    initialized: bool,
    source_available: bool,
    pub application: Application,
}

impl Default for Simulator {
    fn default() -> Self {
        Self {
            epoch: Uuid::new_v4(),
            config: Configuration::default(),
            settings: BTreeMap::new(),
            policy: None,
            desired: None,
            execution: BTreeMap::new(),
            samples: BTreeMap::new(),
            deadlines: BTreeMap::new(),
            sequences: BTreeMap::new(),
            initialized: false,
            source_available: false,
            application: Application {
                applied_plan_version: None,
                attempted_plan_version: None,
                error: None,
            },
        }
    }
}

impl Simulator {
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn execution(&self) -> &BTreeMap<(Uuid, u32), Enforcement> {
        &self.execution
    }
    pub fn samples(&self) -> &BTreeMap<Uuid, Sample> {
        &self.samples
    }

    // now_ms is elapsed monotonic time; only tick's report_time_ms is UTC epoch time.
    pub fn initialize(
        &mut self,
        config: Configuration,
        settings: BTreeMap<Uuid, Settings>,
        policy: Assessment,
        desired: Plan,
        now_ms: u64,
    ) -> Result<()> {
        ensure!(
            !self.initialized,
            "initialization requires a fresh simulator instance"
        );
        config.validate()?;
        validate_settings(&config, &settings)?;
        policy.validate_batch(&config)?;
        self.config = config;
        self.settings = settings;
        self.policy = Some(policy);
        self.initialized = true;
        self.source_available = true;
        self.deadlines = self.config.gpus.keys().map(|id| (*id, now_ms)).collect();
        self.accept_plan(desired, now_ms)
    }

    pub fn observe_configuration(
        &mut self,
        config: Configuration,
        settings: BTreeMap<Uuid, Settings>,
        now_ms: u64,
    ) -> Result<Vec<Uuid>> {
        ensure!(
            self.initialized,
            "configuration bootstrap has not completed"
        );
        if let Err(error) = config
            .validate()
            .and_then(|_| validate_settings(&config, &settings))
        {
            self.invalidate_source(now_ms, "invalid-configuration");
            return Err(error);
        }
        let removed = self
            .config
            .gpus
            .keys()
            .filter(|id| !config.gpus.contains_key(id))
            .copied()
            .collect();
        self.samples.retain(|id, _| config.gpus.contains_key(id));
        self.sequences.retain(|id, _| config.gpus.contains_key(id));
        self.deadlines.retain(|id, _| config.gpus.contains_key(id));
        for (id, s) in &settings {
            let before = self.settings.get(id);
            if before.is_none_or(|b| {
                b.interval_ms != s.interval_ms
                    || (!b.reporting_enabled && s.reporting_enabled)
                    || (!b.powered_on && s.powered_on)
            }) {
                self.deadlines.insert(*id, now_ms);
            }
        }
        self.config = config;
        self.settings = settings;
        self.retire();
        self.enforce(now_ms)?;
        Ok(removed)
    }

    fn retire(&mut self) {
        self.execution.retain(|(id, i), e| {
            self.config
                .workloads
                .get(id)
                .is_some_and(|w| *i < w.replicas)
                && self
                    .settings
                    .get(&e.assignment.gpu_id)
                    .is_some_and(|s| s.powered_on)
        });
    }

    pub fn invalidate_source(&mut self, now_ms: u64, reason: &str) {
        self.source_available = false;
        self.policy = None;
        for e in self.execution.values_mut() {
            if e.state != Execution::Fenced {
                e.state = Execution::Suspended;
                e.reason = reason.into();
                e.acknowledged_monotonic_ms = now_ms;
                e.input_fingerprint = None;
            }
        }
    }

    pub fn observe_policy(&mut self, policy: Assessment, now_ms: u64) -> Result<()> {
        if let Err(e) = policy.validate_batch(&self.config) {
            self.policy = None;
            self.enforce(now_ms)?;
            return Err(e);
        }
        self.policy = Some(policy);
        self.enforce(now_ms)
    }

    pub fn invalidate_policy(&mut self, now_ms: u64) -> Result<()> {
        self.policy = None;
        self.enforce(now_ms)
    }

    // Only an explicit fresh source/bootstrap boundary may reopen this gate.
    pub fn complete_source_recovery(&mut self, now_ms: u64) -> Result<()> {
        ensure!(self.initialized, "bootstrap incomplete");
        self.config.validate()?;
        validate_settings(&self.config, &self.settings)?;
        self.policy
            .as_ref()
            .context("recovery policy batch missing")?
            .validate_batch(&self.config)?;
        self.source_available = true;
        self.enforce(now_ms)
    }

    fn enforce(&mut self, now_ms: u64) -> Result<()> {
        let mut resumes = Vec::new();
        for (key, e) in &mut self.execution {
            let a = &e.assignment;
            let w = self
                .config
                .workloads
                .get(&a.workload_id)
                .context("execution workload disappeared")?;
            let g = self
                .config
                .gpus
                .get(&a.gpu_id)
                .context("execution GPU disappeared")?;
            let c = self
                .config
                .clusters
                .get(&g.cluster_id)
                .context("execution cluster disappeared")?;
            let expected = match input_fingerprint(&self.config, w, c) {
                Ok(value) => {
                    e.policy_error = None;
                    Some(value)
                }
                Err(error) => {
                    e.policy_error = Some(format!("{error:#}"));
                    None
                }
            };
            let current = self
                .policy
                .as_ref()
                .and_then(|p| p.pairs.get(&pair_key(a.workload_id, &g.cluster_id)))
                .filter(|p| {
                    self.source_available
                        && expected.as_ref() == Some(&p.input_fingerprint)
                        && p.error.is_none()
                });
            let (state, reason) = match current.map(|p| &p.authorization) {
                Some(Authorization::Deny) => (Execution::Fenced, "policy-denied"),
                Some(Authorization::Allow)
                    if a.data_profile_id != w.data_profile_id || a.purpose != w.purpose =>
                {
                    (
                        if e.state == Execution::Fenced {
                            Execution::Fenced
                        } else {
                            Execution::Suspended
                        },
                        "new-data-context-requires-plan",
                    )
                }
                Some(Authorization::Allow) => {
                    if e.state != Execution::Running {
                        resumes.push(*key);
                    }
                    (e.state.clone(), e.reason.as_str())
                }
                _ if e.state == Execution::Fenced => {
                    (Execution::Fenced, "waiting-for-current-plan")
                }
                _ => (Execution::Suspended, "policy-pending"),
            };
            if state != e.state || expected != e.input_fingerprint || reason != e.reason {
                e.reason = reason.into();
                e.state = state;
                e.acknowledged_monotonic_ms = now_ms;
                e.input_fingerprint = expected;
            }
        }
        for key in resumes {
            let assignment = &self.execution[&key].assignment;
            let result = self.validate_resume(assignment);
            let e = self
                .execution
                .get_mut(&key)
                .context("resume execution disappeared")?;
            let (state, reason) = match result {
                Ok(()) => (
                    Execution::Running,
                    "current-assignment-authorized".to_owned(),
                ),
                Err(error) => (e.state.clone(), format!("resume-blocked: {error:#}")),
            };
            if state != e.state || reason != e.reason {
                e.state = state;
                e.reason = reason;
                e.acknowledged_monotonic_ms = now_ms;
            }
        }
        Ok(())
    }

    fn validate_resume(&self, assignment: &Assignment) -> Result<()> {
        ensure!(
            self.desired
                .as_ref()
                .is_some_and(|plan| plan.assignments.contains(assignment)),
            "execution no longer matches the latest desired assignment"
        );
        let gpu = self
            .config
            .gpus
            .get(&assignment.gpu_id)
            .context("resume GPU missing")?;
        let settings = self
            .settings
            .get(&assignment.gpu_id)
            .context("resume settings missing")?;
        let workload = self
            .config
            .workloads
            .get(&assignment.workload_id)
            .context("resume workload missing")?;
        let mut capacity = Capacity::from_observation(
            gpu,
            settings.powered_on,
            settings.background_memory_mib,
            settings.background_compute_units,
        );
        for other in self
            .execution
            .values()
            .filter(|e| e.assignment.key() != assignment.key() && e.state != Execution::Fenced)
        {
            let other_gpu = &self.config.gpus[&other.assignment.gpu_id];
            ensure!(
                !workload.spread_across_domains
                    || other.assignment.workload_id != assignment.workload_id
                    || other_gpu.failure_domain != gpu.failure_domain,
                "replica separation violated by existing execution"
            );
            if other.assignment.gpu_id == assignment.gpu_id {
                capacity.memory_mib = capacity
                    .memory_mib
                    .saturating_sub(other.assignment.memory_mib);
                if other.state == Execution::Running {
                    capacity.compute_units = capacity
                        .compute_units
                        .saturating_sub(other.assignment.compute_units);
                }
            }
        }
        validate_execution_subset(
            &self.config,
            &BTreeMap::from([(gpu.gpu_id, capacity)]),
            std::slice::from_ref(assignment),
        )
    }

    pub fn accept_plan(&mut self, plan: Plan, now_ms: u64) -> Result<()> {
        ensure!(self.initialized, "bootstrap incomplete");
        ensure!(plan.fleet_id == "demo", "unknown fleet");
        if let Some(old) = &self.desired {
            ensure!(plan.plan_version >= old.plan_version, "superseded plan");
            if plan.plan_version == old.plan_version {
                ensure!(
                    hash("plan", old)? == hash("plan", &plan)?,
                    "same version has different content"
                );
            }
        }
        self.desired = Some(plan);
        self.retry_desired(now_ms)
    }

    pub fn retry_desired(&mut self, now_ms: u64) -> Result<()> {
        self.retire();
        self.enforce(now_ms)?;
        let plan = self.desired.as_ref().context("no desired plan")?;
        self.application.attempted_plan_version = Some(plan.plan_version.to_string());
        let validate = || -> Result<()> {
            ensure!(self.source_available, "source/policy unavailable");
            let policy = self.policy.as_ref().context("policy uninitialized")?;
            ensure!(
                plan.config_fingerprint == self.config.fingerprint()?
                    && plan.policy_signature == policy.policy_signature
                    && plan.policy_bundle_hash == policy.policy_bundle_hash,
                "stale plan configuration/policy"
            );
            policy.validate_plan(&self.config, &plan.assignments)?;
            let capacities = self
                .config
                .gpus
                .values()
                .map(|g| {
                    let s = &self.settings[&g.gpu_id];
                    (
                        g.gpu_id,
                        Capacity::from_observation(
                            g,
                            s.powered_on,
                            s.background_memory_mib,
                            s.background_compute_units,
                        ),
                    )
                })
                .collect();
            validate_assignments(&self.config, &capacities, &plan.assignments)
        };
        if let Err(error) = validate() {
            self.application.error = Some(format!("{error:#}"));
            return Err(error);
        }
        let mut next = BTreeMap::new();
        let already_applied = self.application.applied_plan_version.as_deref()
            == Some(&plan.plan_version.to_string());
        for a in &plan.assignments {
            let w = &self.config.workloads[&a.workload_id];
            let c = &self.config.clusters[&self.config.gpus[&a.gpu_id].cluster_id];
            let fingerprint = input_fingerprint(&self.config, w, c)?;
            if let Some(previous) = self.execution.get(&a.key()).filter(|previous| {
                already_applied
                    && previous.assignment == *a
                    && previous.state == Execution::Running
                    && previous.input_fingerprint.as_ref() == Some(&fingerprint)
            }) {
                next.insert(a.key(), previous.clone());
                continue;
            }
            next.insert(
                a.key(),
                Enforcement {
                    id: format!("{}/{}", a.workload_id, a.replica_index),
                    assignment: a.clone(),
                    state: Execution::Running,
                    input_fingerprint: Some(fingerprint),
                    reason: "current-plan-allowed".into(),
                    acknowledged_monotonic_ms: now_ms,
                    policy_error: None,
                },
            );
        }
        self.execution = next;
        self.application.applied_plan_version = Some(plan.plan_version.to_string());
        self.application.error = None;
        Ok(())
    }

    pub fn tick(&mut self, monotonic_ms: u64, report_time_ms: u64) -> Result<Vec<Sample>> {
        ensure!(self.initialized, "bootstrap incomplete");
        self.retire();
        self.enforce(monotonic_ms)?;
        let mut emitted = Vec::new();
        for (id, s) in &self.settings {
            if !s.powered_on || !s.reporting_enabled {
                continue;
            }
            let deadline = self.deadlines.get_mut(id).context("GPU timer missing")?;
            if monotonic_ms < *deadline {
                continue;
            }
            *deadline = monotonic_ms
                .checked_add(s.interval_ms)
                .context("timer overflow")?;
            let resident = self
                .execution
                .values()
                .filter(|e| e.assignment.gpu_id == *id && e.state != Execution::Fenced);
            let memory: u32 = resident.clone().map(|e| e.assignment.memory_mib).sum();
            let demand: u32 = resident
                .filter(|e| e.state == Execution::Running)
                .map(|e| e.assignment.compute_units)
                .sum();
            let sequence = self.sequences.entry(*id).or_default();
            *sequence = sequence
                .checked_add(1)
                .context("sample sequence exhausted")?;
            let background = s
                .background_memory_mib
                .min(MEMORY_MIB.saturating_sub(memory));
            let sample = Sample {
                schema_version: 1,
                gpu_id: *id,
                observation_epoch: self.epoch,
                report_sequence: *sequence,
                report_time_ms,
                inventory_revision: self.config.gpus[id].revision,
                telemetry_revision: s.revision,
                applied_plan_version: self.application.applied_plan_version.clone(),
                background_compute_units: s.background_compute_units,
                managed_compute_units: demand,
                total_compute_units: s.background_compute_units + demand,
                busy_percent: (s.background_compute_units + demand).min(100),
                managed_memory_mib: memory,
                background_memory_requested_mib: s.background_memory_mib,
                background_memory_allocated_mib: background,
                modeled_memory_used_mib: memory + background,
            };
            self.samples.insert(*id, sample.clone());
            emitted.push(sample);
        }
        Ok(emitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_policy::Evaluator;
    fn setup(name: &str) -> Result<(Simulator, fixtures::Fixture)> {
        let fixture = fixtures::load(name)?;
        let p = Evaluator::new()?.evaluate(&fixture.configuration)?;
        let plan = Plan {
            fleet_id: "demo".into(),
            plan_version: 1,
            decision_id: Uuid::new_v4(),
            config_fingerprint: fixture.configuration.fingerprint()?,
            policy_signature: p.policy_signature.clone(),
            policy_bundle_hash: p.policy_bundle_hash.clone(),
            assignments: fixture.assignments.clone(),
            decision_details: serde_json::json!({"reason_codes":["fixture-setup"]}),
        };
        let mut sim = Simulator::default();
        sim.initialize(
            fixture.configuration.clone(),
            fixture.settings.clone(),
            p,
            plan,
            0,
        )?;
        Ok((sim, fixture))
    }
    #[test]
    fn reporting_loss_preserves_execution_and_sample() -> Result<()> {
        let (mut sim, mut f) = setup("baseline")?;
        sim.tick(0, 100)?;
        let id = f.assignments[0].gpu_id;
        f.settings.get_mut(&id).unwrap().reporting_enabled = false;
        sim.observe_configuration(f.configuration, f.settings, 0)?;
        sim.tick(6000, 6100)?;
        assert_eq!(sim.samples[&id].report_time_ms, 100);
        assert_eq!(
            sim.execution
                .values()
                .filter(|e| e.state == Execution::Running)
                .count(),
            8
        );
        Ok(())
    }
    #[test]
    fn unchanged_plan_preserves_execution_acknowledgements() -> Result<()> {
        let (mut sim, _) = setup("baseline")?;
        sim.tick(10, 1_000_010)?;
        sim.retry_desired(100)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.acknowledged_monotonic_ms == 0));
        let mut next = sim.desired.clone().unwrap();
        next.plan_version += 1;
        sim.accept_plan(next, 200)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.acknowledged_monotonic_ms == 200));
        assert!(sim.samples.values().all(|s| s.report_time_ms == 1_000_010));
        Ok(())
    }
    #[test]
    fn power_off_removes_execution_without_refreshing_heartbeat() -> Result<()> {
        let (mut sim, mut f) = setup("baseline")?;
        sim.tick(0, 100)?;
        let id = f.assignments[0].gpu_id;
        f.settings.get_mut(&id).unwrap().powered_on = false;
        sim.observe_configuration(f.configuration, f.settings, 0)?;
        assert!(sim.execution.values().all(|e| e.assignment.gpu_id != id));
        assert_eq!(sim.samples[&id].report_time_ms, 100);
        Ok(())
    }
    #[test]
    fn policy_tightening_suspends_then_fences_without_a_new_plan() -> Result<()> {
        let (mut sim, mut f) = setup("regional-boundary")?;
        sim.tick(0, 100)?;
        let p = f
            .configuration
            .policies
            .get_mut("customer-eu-processing")
            .unwrap();
        p.allowed_regions.clear();
        p.revision += 1;
        sim.observe_configuration(f.configuration.clone(), f.settings, 200)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.state == Execution::Suspended));
        assert!(sim.samples.values().all(|s| s.report_time_ms == 100));
        sim.observe_policy(Evaluator::new()?.evaluate(&f.configuration)?, 210)?;
        assert!(sim.execution.values().all(|e| e.state == Execution::Fenced));
        assert!(sim.retry_desired(220).is_err());
        let reports = sim.tick(1000, 1100)?;
        assert!(reports
            .iter()
            .all(|s| s.managed_memory_mib == 0 && s.managed_compute_units == 0));
        Ok(())
    }
    #[test]
    fn source_failure_suspends_processing_but_not_reporting() -> Result<()> {
        let (mut sim, _) = setup("baseline")?;
        sim.invalidate_source(20, "source-unavailable");
        let reports = sim.tick(1000, 1000)?;
        assert_eq!(
            reports.iter().map(|s| s.managed_memory_mib).sum::<u32>(),
            216 * 1024
        );
        assert_eq!(
            reports.iter().map(|s| s.managed_compute_units).sum::<u32>(),
            0
        );
        Ok(())
    }
    #[test]
    fn missed_ticks_are_skipped_and_busy_is_bounded() -> Result<()> {
        let (mut sim, mut f) = setup("baseline")?;
        for s in f.settings.values_mut() {
            s.background_compute_units = 200;
        }
        sim.observe_configuration(f.configuration, f.settings, 0)?;
        assert_eq!(sim.tick(1_000_000, 1_000_000)?.len(), 6);
        assert!(sim.tick(1_000_001, 1_000_001)?.is_empty());
        assert!(sim.samples.values().all(|s| s.busy_percent <= 100));
        Ok(())
    }
    #[test]
    fn delayed_policy_cannot_clear_a_source_failure() -> Result<()> {
        let (mut sim, f) = setup("baseline")?;
        let delayed = Evaluator::new()?.evaluate(&f.configuration)?;
        sim.invalidate_source(20, "source-unavailable");
        sim.observe_policy(delayed, 30)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.state == Execution::Suspended));
        assert!(sim.retry_desired(40).is_err());
        sim.observe_configuration(f.configuration, f.settings, 50)?;
        assert!(sim.retry_desired(55).is_err());
        sim.complete_source_recovery(56)?;
        sim.retry_desired(60)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.state == Execution::Running));
        Ok(())
    }
    #[test]
    fn current_permissions_resume_survivors_without_a_partial_plan() -> Result<()> {
        let (mut sim, mut f) = setup("baseline")?;
        sim.tick(0, 100)?;
        let affected = *f.configuration.workloads.keys().next().unwrap();
        let workload = f.configuration.workloads.get_mut(&affected).unwrap();
        workload.purpose = "customer-support".into();
        workload.revision += 1;
        sim.observe_configuration(f.configuration.clone(), f.settings.clone(), 10)?;
        sim.invalidate_policy(20)?;
        assert!(sim
            .execution
            .values()
            .all(|e| e.state == Execution::Suspended));
        sim.observe_policy(Evaluator::new()?.evaluate(&f.configuration)?, 30)?;
        assert!(sim.retry_desired(40).is_err());
        for e in sim.execution.values() {
            assert_eq!(
                e.state,
                if e.assignment.workload_id == affected {
                    Execution::Fenced
                } else {
                    Execution::Running
                }
            );
        }
        assert_eq!(sim.application.applied_plan_version.as_deref(), Some("1"));
        assert!(sim.samples.values().all(|s| s.report_time_ms == 100));

        let policy = f.configuration.policies.get_mut("demo-permissive").unwrap();
        policy.allowed_regions = ["westeurope".into()].into();
        policy.allowed_purposes.insert("customer-support".into());
        policy.revision += 1;
        sim.observe_configuration(f.configuration.clone(), f.settings, 50)?;
        sim.observe_policy(Evaluator::new()?.evaluate(&f.configuration)?, 60)?;
        assert!(
            sim.execution
                .values()
                .filter(|e| e.assignment.workload_id == affected)
                .all(|e| e.state == Execution::Fenced),
            "a new purpose must not reinterpret or allocate the old assignment"
        );
        Ok(())
    }

    #[test]
    fn resuming_execution_checks_actual_resident_memory_and_running_demand() -> Result<()> {
        let (mut sim, mut f) = setup("baseline")?;
        let gpu = f
            .configuration
            .gpus
            .values()
            .find(|g| g.host_id == "inference-a" && g.gpu_index == 0)
            .unwrap()
            .gpu_id;
        sim.invalidate_policy(10)?;
        f.settings.get_mut(&gpu).unwrap().background_memory_mib = 4097;
        sim.observe_configuration(f.configuration.clone(), f.settings, 20)?;
        sim.observe_policy(Evaluator::new()?.evaluate(&f.configuration)?, 30)?;
        assert!(sim
            .execution
            .values()
            .filter(|e| e.assignment.gpu_id == gpu)
            .all(|e| e.state == Execution::Suspended));
        assert!(sim
            .execution
            .values()
            .filter(|e| e.assignment.gpu_id != gpu)
            .all(|e| e.state == Execution::Running));
        Ok(())
    }

    #[test]
    fn suspended_peers_keep_memory_but_do_not_consume_running_demand() -> Result<()> {
        for (memory, can_resume) in [(0, true), (73 * 1024, false)] {
            let (mut sim, mut f) = setup("baseline")?;
            let gpu = f
                .configuration
                .gpus
                .values()
                .find(|g| g.host_id == "inference-b" && g.gpu_index == 1)
                .unwrap()
                .gpu_id;
            sim.invalidate_policy(10)?;
            let settings = f.settings.get_mut(&gpu).unwrap();
            settings.background_compute_units = 55;
            settings.background_memory_mib = memory;
            sim.observe_configuration(f.configuration.clone(), f.settings, 20)?;
            let mut policy = Evaluator::new()?.evaluate(&f.configuration)?;
            let embeddings = f
                .configuration
                .workloads
                .values()
                .find(|w| w.profile_id == "embeddings-v1")
                .unwrap()
                .workload_id;
            let pair = policy
                .pairs
                .get_mut(&pair_key(embeddings, "eu-primary"))
                .unwrap();
            pair.authorization = Authorization::Unknown;
            pair.error = Some("evaluation unavailable".into());
            policy.policy_signature = hash(
                "gpu-policy-assessment-v1",
                &(&policy.policy_bundle_hash, &policy.pairs),
            )?;
            sim.observe_policy(policy, 30)?;
            let resident: Vec<_> = sim
                .execution
                .values()
                .filter(|e| e.assignment.gpu_id == gpu)
                .collect();
            assert_eq!(resident.len(), 2);
            for e in resident {
                assert_eq!(
                    e.state,
                    if e.assignment.workload_id != embeddings && can_resume {
                        Execution::Running
                    } else {
                        Execution::Suspended
                    }
                );
            }
        }
        Ok(())
    }

    #[test]
    fn partial_unknown_batch_does_not_suspend_unaffected_pairs() -> Result<()> {
        let (mut sim, f) = setup("baseline")?;
        let mut policy = Evaluator::new()?.evaluate(&f.configuration)?;
        let pair = policy.pairs.values_mut().next().unwrap();
        let affected = pair.workload_id;
        pair.authorization = Authorization::Unknown;
        pair.error = Some("evaluation deadline exceeded".into());
        policy.policy_signature = hash(
            "gpu-policy-assessment-v1",
            &(&policy.policy_bundle_hash, &policy.pairs),
        )?;
        sim.observe_policy(policy, 30)?;
        for e in sim.execution.values() {
            assert_eq!(
                e.state,
                if e.assignment.workload_id == affected {
                    Execution::Suspended
                } else {
                    Execution::Running
                }
            );
        }
        Ok(())
    }

    #[test]
    fn rejected_bootstrap_plan_does_not_prevent_fresh_device_reports() -> Result<()> {
        let f = fixtures::load("baseline")?;
        let policy = Evaluator::new()?.evaluate(&f.configuration)?;
        let plan = Plan {
            fleet_id: "demo".into(),
            plan_version: 1,
            decision_id: Uuid::new_v4(),
            config_fingerprint: "obsolete-configuration".into(),
            policy_signature: policy.policy_signature.clone(),
            policy_bundle_hash: policy.policy_bundle_hash.clone(),
            assignments: f.assignments,
            decision_details: serde_json::json!({}),
        };
        let mut simulator = Simulator::default();
        assert!(simulator
            .initialize(f.configuration, f.settings, policy, plan, 0)
            .is_err());
        assert!(simulator.is_initialized());
        let samples = simulator.tick(0, 100)?;
        assert_eq!(samples.len(), 6);
        assert!(samples
            .iter()
            .all(|s| s.managed_memory_mib == 0 && s.managed_compute_units == 0));
        assert!(simulator.application.applied_plan_version.is_none());
        assert!(simulator.application.error.is_some());
        Ok(())
    }
}
