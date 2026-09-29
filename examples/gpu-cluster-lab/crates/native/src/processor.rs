use crate::{
    evidence::{self, Decision, DecisionOutcome, DecisionStage},
    inputs::{DatabaseInputs, DatabaseSnapshot, SchedulingRow, SimulationRow, DATABASE_QUERIES},
    lifecycle::Signals,
    wire::{self, Emitter, Message, SolveInput},
    Kind, Workers,
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_core::models::SourceChange;
use drasi_lib::computation::v1::{
    ChangeOperation, ComponentDescriptor, ComputationComponent, InputEnvelope, OutputEnvelope,
    QueryChangeCodec, StreamId, Transformer, WakeupSource,
};
use gpu_contracts::*;
use gpu_placement::{LossKind, Outcome, Resilience, Scenario};
use gpu_policy::{Assessment, Evaluator};
use gpu_simulator::Simulator;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

struct Timer {
    running: AtomicBool,
}
#[async_trait]
impl WakeupSource for Timer {
    async fn wait(&self) -> Result<()> {
        ensure!(self.running.load(Ordering::Acquire), "timer stopped");
        tokio::time::sleep(Duration::from_millis(25)).await;
        Ok(())
    }
    async fn has_pending(&self) -> Result<bool> {
        Ok(false)
    }
}
enum Job {
    Policy(Assessment),
    Placement(Outcome),
    Scenario(LossKind, Scenario),
    CapacityOnly(Scenario),
}
struct Active {
    signature: String,
    generation: u64,
    task: JoinHandle<Result<Job>>,
}
#[derive(Clone)]
struct SimulationInputs {
    configuration: Configuration,
    settings: BTreeMap<Uuid, Settings>,
    plan: Plan,
    policy: Option<Assessment>,
}

pub struct Processor {
    pub(crate) signals: Arc<Mutex<Signals>>,
    database: DatabaseInputs,
    kind: Kind,
    descriptor: ComponentDescriptor,
    configuration: serde_json::Value,
    emitter: Emitter,
    workers: Arc<Workers>,
    timer: Arc<Timer>,
    started: Option<Instant>,
    started_utc_ms: Option<u64>,
    stopped: bool,
    epoch: Option<Uuid>,
    simulation: Simulator,
    simulation_inputs: Option<SimulationInputs>,
    initialized: bool,
    source_ready: bool,
    recovering_source: bool,
    policy_input: Option<Configuration>,
    policy_signature: Option<String>,
    solve_input: Option<SolveInput>,
    signature: Option<String>,
    job: Option<Active>,
    due: Instant,
    scenarios: VecDeque<(LossKind, String)>,
    assessment: Option<Resilience>,
    completed: Option<String>,
    generation: u64,
    decisions: VecDeque<String>,
    diagnostic_pending: bool,
    published_plan: Option<String>,
    published_locations: BTreeMap<String, (Uuid, String)>,
}
impl Processor {
    pub fn new(
        kind: Kind,
        descriptor: ComponentDescriptor,
        configuration: serde_json::Value,
        workers: Arc<Workers>,
    ) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Config {
            stream: StreamId,
        }
        let config: Config = serde_json::from_value(configuration.clone())?;
        Ok(Self {
            signals: Arc::new(Mutex::new(Signals::default())),
            database: DatabaseInputs::default(),
            kind,
            descriptor,
            configuration,
            emitter: Emitter::new(config.stream),
            workers,
            timer: Arc::new(Timer {
                running: AtomicBool::new(false),
            }),
            started: None,
            started_utc_ms: None,
            stopped: false,
            epoch: None,
            simulation: Simulator::default(),
            simulation_inputs: None,
            initialized: false,
            source_ready: false,
            recovering_source: false,
            policy_input: None,
            policy_signature: None,
            solve_input: None,
            signature: None,
            job: None,
            due: Instant::now(),
            scenarios: VecDeque::new(),
            assessment: None,
            completed: None,
            generation: 0,
            decisions: VecDeque::new(),
            diagnostic_pending: false,
            published_plan: None,
            published_locations: BTreeMap::new(),
        })
    }
    fn now(&self) -> Result<u64> {
        Ok(u64::try_from(
            self.started
                .context("component not started")?
                .elapsed()
                .as_millis(),
        )?)
    }
    fn observe_inputs(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        let signals = {
            let mut signals = self
                .signals
                .lock()
                .map_err(|_| anyhow::anyhow!("lifecycle signal lock poisoned"))?;
            std::mem::take(&mut *signals)
        };
        if let Some(reason) = signals.unavailable {
            self.database.unavailable();
            self.invalidate(&reason, changes)?;
        }
        if let Some(boundary) = signals.bootstrap {
            self.database.bootstrap(boundary)?;
        }
        let snapshot = match self.database.take() {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return Ok(()),
            Err(error) => {
                self.invalidate(
                    &format!("Database query inputs are incomplete: {error:#}"),
                    changes,
                )?;
                return Ok(());
            }
        };
        self.accept_database(snapshot, changes)
    }
    fn accept_database(
        &mut self,
        snapshot: DatabaseSnapshot,
        changes: &mut Vec<SourceChange>,
    ) -> Result<()> {
        let epoch = snapshot.epoch;
        let bootstrap = !self.source_ready
            || match self.kind {
                Kind::Policy => self.policy_input.is_none(),
                _ => self.simulation_inputs.is_none(),
            };
        let message = if bootstrap {
            Message::Bootstrap {
                epoch,
                commit: "observed-query-bootstrap".into(),
                configuration: snapshot.configuration.clone(),
                settings: snapshot.settings.clone(),
                plan: snapshot.plan.clone(),
            }
        } else {
            Message::Configuration {
                epoch,
                commit: "observed-query-row-update".into(),
                configuration: snapshot.configuration.clone(),
                settings: snapshot.settings.clone(),
            }
        };
        self.accept(message, changes)?;
        if self.kind == Kind::Simulator && !bootstrap {
            self.accept(
                Message::Allocation {
                    epoch,
                    plan: snapshot.plan.clone(),
                },
                changes,
            )?;
        }
        if self.kind == Kind::Policy {
            let mut value = serde_json::to_value(&snapshot)?;
            value["config_fingerprint"] = json!(snapshot.configuration.fingerprint()?);
            self.add(changes, "FleetConfiguration", "demo", &value)?;
        }
        Ok(())
    }
    fn utc_at(&self, monotonic_ms: u64) -> Result<u64> {
        self.started_utc_ms
            .context("component clock not started")?
            .checked_add(monotonic_ms)
            .context("component clock overflow")
    }
    fn decision(&mut self, changes: &mut Vec<SourceChange>, decision: &Decision) -> Result<()> {
        let id = decision.decision_id.to_string();
        if let Some(change) = self.emitter.record("DecisionExplanation", &id, decision)? {
            changes.push(change);
            if matches!(decision.stage, DecisionStage::Committed) {
                self.workers.hub.event(
                    self.epoch.context("missing epoch")?,
                    self.descriptor.id().as_str(),
                    "committed",
                    decision.summary.clone(),
                    Some(decision.decision_id),
                    decision.plan_version.clone(),
                )?;
            }
        }
        if !self.decisions.contains(&id) {
            self.decisions.push_back(id);
        }
        while self.decisions.len() > 64 {
            let old = self
                .decisions
                .pop_front()
                .context("decision history missing")?;
            if let Some(change) = self.emitter.remove("DecisionExplanation", &old)? {
                changes.push(change);
            }
        }
        Ok(())
    }
    fn policy_rows(
        &mut self,
        changes: &mut Vec<SourceChange>,
        assessment: Option<&Assessment>,
    ) -> Result<()> {
        let rows = evidence::policy_rows(
            self.policy_input.as_ref().context("policy input missing")?,
            assessment,
            self.epoch.context("missing epoch")?,
        )?;
        changes.extend(self.emitter.retain(
            "PlacementEligibility",
            &rows.iter().map(|row| row.id.clone()).collect(),
        )?);
        for row in rows {
            self.add(changes, "PlacementEligibility", &row.id, &row)?;
        }
        Ok(())
    }
    fn add<T: serde::Serialize>(
        &mut self,
        changes: &mut Vec<SourceChange>,
        label: &str,
        key: &str,
        value: &T,
    ) -> Result<()> {
        if let Some(change) = self.emitter.record(label, key, value)? {
            changes.push(change);
        }
        Ok(())
    }
    fn status(
        &mut self,
        changes: &mut Vec<SourceChange>,
        status: &str,
        error: Option<String>,
    ) -> Result<()> {
        self.workers.hub.publish(
            self.descriptor.id().as_str(),
            status,
            error.clone(),
            self.epoch,
        )?;
        if self.kind == Kind::Placement {
            let input = self.solve_input.as_ref();
            let context = json!({
                "fleet_id":"demo", "observation_epoch":self.epoch,
                "scheduling_signature":self.signature,
                "policy_signature":input.map(|input| &input.policy.policy_signature),
                "config_fingerprint":input.map(|input| input.configuration.fingerprint()).transpose()?,
                "required_replicas":input.map(|input| input.configuration.workloads.values().map(|w| u64::from(w.replicas)).sum::<u64>()),
                "current":self.source_ready && input.is_some(),
            });
            self.add(changes, "SchedulingContext", "demo", &context)?;
        }
        self.add(changes,"RuntimeStatus",&self.descriptor.id().to_string(),&json!({
            "schema_version":1,"component_id":self.descriptor.id().as_str(),"status":status,"error":error,
            "observation_epoch":self.epoch,"scheduling_signature":self.signature,
            "policy_signature":self.solve_input.as_ref().map(|input| &input.policy.policy_signature),
        }))
    }
    fn invalidate(&mut self, reason: &str, changes: &mut Vec<SourceChange>) -> Result<()> {
        self.generation = self
            .generation
            .checked_add(1)
            .context("input generation exhausted")?;
        self.source_ready = false;
        self.recovering_source = false;
        self.policy_input = None;
        self.policy_signature = None;
        self.solve_input = None;
        self.signature = None;
        self.completed = None;
        self.diagnostic_pending = false;
        self.scenarios.clear();
        self.assessment = None;
        if self.kind == Kind::Placement {
            self.workers
                .placement_pending
                .store(false, Ordering::Release);
        }
        if self.initialized {
            self.simulation.invalidate_source(self.now()?, reason);
        }
        for label in [
            "CandidatePlan",
            "CapacityDiagnostic",
            "ResilienceAssessment",
        ] {
            if let Some(change) = self.emitter.remove(label, "demo")? {
                changes.push(change);
            }
        }
        if self.kind == Kind::Policy {
            if let Some(change) = self.emitter.remove("FleetConfiguration", "demo")? {
                changes.push(change);
            }
            if let Some(epoch) = self.epoch {
                self.add(
                    changes,
                    "PolicyAssessment",
                    "demo",
                    &Message::Unavailable {
                        epoch,
                        reason: reason.into(),
                    },
                )?;
            }
            changes.extend(
                self.emitter
                    .retain("PlacementEligibility", &BTreeSet::new())?,
            );
        }
        self.status(changes, "unavailable", Some(reason.into()))?;
        if self.kind == Kind::Simulator && self.initialized {
            self.apply_simulation(changes)?;
        }
        Ok(())
    }
    fn accept(&mut self, message: Message, changes: &mut Vec<SourceChange>) -> Result<()> {
        if let Some(epoch) = self.epoch {
            ensure!(
                epoch == message.epoch(),
                "new observation epoch requires fresh component construction"
            );
        } else {
            self.epoch = Some(message.epoch());
            self.simulation.epoch = message.epoch();
            self.workers
                .hub
                .publish(self.descriptor.id().as_str(), "running", None, self.epoch)?;
        }
        if let Message::Unavailable { reason, .. } = message {
            return self.invalidate(&reason, changes);
        }
        match (self.kind, message) {
            (
                Kind::Policy,
                Message::Bootstrap {
                    commit,
                    configuration,
                    ..
                },
            )
            | (
                Kind::Policy,
                Message::Configuration {
                    commit,
                    configuration,
                    ..
                },
            ) => {
                nonempty(&commit)?;
                let signature = configuration.fingerprint()?;
                if self.policy_signature.as_ref() != Some(&signature) {
                    self.generation = self
                        .generation
                        .checked_add(1)
                        .context("input generation exhausted")?;
                    self.policy_signature = Some(signature);
                    self.policy_input = Some(configuration);
                    self.source_ready = true;
                    self.completed = None;
                    self.policy_rows(changes, None)?;
                    self.status(changes, "policy-pending", None)?;
                }
            }
            (
                Kind::Simulator,
                Message::Bootstrap {
                    commit,
                    configuration,
                    settings,
                    plan,
                    ..
                },
            ) => {
                nonempty(&commit)?;
                ensure!(
                    !self.initialized || !self.source_ready,
                    "bootstrap may not overwrite a healthy live simulator"
                );
                configuration.validate()?;
                validate_settings(&configuration, &settings)?;
                if self.initialized {
                    self.observe_simulation_configuration(
                        configuration.clone(),
                        settings.clone(),
                        changes,
                    )?;
                    if let Err(error) = self.simulation.accept_plan(plan.clone(), self.now()?) {
                        self.status(
                            changes,
                            "awaiting-source-reauthorization",
                            Some(format!("{error:#}")),
                        )?;
                    }
                    self.recovering_source = true;
                }
                self.simulation_inputs = Some(SimulationInputs {
                    configuration,
                    settings,
                    plan,
                    policy: None,
                });
                self.source_ready = !self.recovering_source;
            }
            (
                Kind::Simulator,
                Message::Configuration {
                    commit,
                    configuration,
                    settings,
                    ..
                },
            ) => {
                nonempty(&commit)?;
                let input = self
                    .simulation_inputs
                    .as_mut()
                    .context("configuration arrived before bootstrap")?;
                input.configuration = configuration.clone();
                input.settings = settings.clone();
                if self.initialized {
                    self.observe_simulation_configuration(configuration, settings, changes)?;
                }
            }
            (
                Kind::Simulator,
                Message::Policy {
                    config_fingerprint,
                    assessment,
                    ..
                },
            ) => {
                let input = self
                    .simulation_inputs
                    .as_mut()
                    .context("policy arrived before bootstrap")?;
                ensure!(
                    input.configuration.fingerprint()? == config_fingerprint,
                    "stale policy configuration"
                );
                assessment.validate_batch(&input.configuration)?;
                input.policy = Some(assessment.clone());
                if self.initialized {
                    self.simulation.observe_policy(assessment, self.now()?)?;
                    if self.recovering_source {
                        self.simulation.complete_source_recovery(self.now()?)?;
                        self.recovering_source = false;
                        self.source_ready = true;
                    }
                }
            }
            (Kind::Simulator, Message::Allocation { plan, .. }) => {
                let input = self
                    .simulation_inputs
                    .as_mut()
                    .context("allocation arrived before bootstrap")?;
                input.plan = plan.clone();
                if self.initialized {
                    if let Err(error) = self.simulation.accept_plan(plan, self.now()?) {
                        self.status(changes, "application-rejected", Some(format!("{error:#}")))?;
                    }
                }
            }
            (Kind::Placement | Kind::Resilience, Message::Schedule { input, .. }) => {
                input.validate()?;
                let signature = input.signature()?;
                if self.kind == Kind::Placement {
                    let decision =
                        Decision::committed(&input.plan, self.epoch.context("missing epoch")?)?;
                    self.decision(changes, &decision)?;
                }
                let unchanged = self.signature.as_ref() == Some(&signature)
                    && self
                        .solve_input
                        .as_ref()
                        .is_some_and(|old| old.plan.plan_version == input.plan.plan_version);
                if !unchanged {
                    self.generation = self
                        .generation
                        .checked_add(1)
                        .context("input generation exhausted")?;
                    self.solve_input = Some(input.clone());
                    self.signature = Some(signature.clone());
                    self.source_ready = true;
                    self.completed = None;
                    self.due = Instant::now() + Duration::from_millis(100);
                    self.diagnostic_pending = false;
                    if let Some(change) = self.emitter.remove("CapacityDiagnostic", "demo")? {
                        changes.push(change);
                    }
                    if self.kind == Kind::Placement {
                        self.workers
                            .placement_pending
                            .store(true, Ordering::Release);
                        if let Some(change) = self.emitter.remove("CandidatePlan", "demo")? {
                            changes.push(change);
                        }
                    } else {
                        if let Some(change) = self.emitter.remove("ResilienceAssessment", "demo")? {
                            changes.push(change);
                        }
                        self.prepare_scenarios(&input, signature)?;
                    }
                    self.status(changes, "pending", None)?;
                }
            }
            _ => anyhow::bail!("message is incompatible with this transformer"),
        }
        if self.kind == Kind::Simulator {
            self.apply_simulation(changes)?;
        }
        Ok(())
    }
    fn observe_simulation_configuration(
        &mut self,
        configuration: Configuration,
        settings: BTreeMap<Uuid, Settings>,
        changes: &mut Vec<SourceChange>,
    ) -> Result<()> {
        for id in self
            .simulation
            .observe_configuration(configuration, settings, self.now()?)?
        {
            if let Some(change) = self.emitter.remove("GpuSample", &id.to_string())? {
                changes.push(change);
            }
        }
        Ok(())
    }
    fn apply_simulation(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        let now = self.now()?;
        if let Some(input) = self.simulation_inputs.clone() {
            if !self.initialized {
                if !self.source_ready {
                    return self.status(changes, "awaiting-source-bootstrap", None);
                }
                if let Some(policy) = input.policy {
                    match self.simulation.initialize(
                        input.configuration,
                        input.settings,
                        policy,
                        input.plan,
                        now,
                    ) {
                        Ok(()) => {
                            self.initialized = true;
                            self.status(changes, "running", None)?;
                        }
                        Err(error) => {
                            self.initialized = self.simulation.is_initialized();
                            self.status(
                                changes,
                                if self.initialized {
                                    "application-rejected"
                                } else {
                                    "initialization-error"
                                },
                                Some(format!("{error:#}")),
                            )?;
                            return Ok(());
                        }
                    }
                } else {
                    self.status(changes, "awaiting-policy-bootstrap", None)?;
                    return Ok(());
                }
            } else if self.source_ready {
                match self.simulation.retry_desired(now) {
                    Ok(()) => self.status(changes, "running", None)?,
                    Err(error) => {
                        self.status(changes, "application-rejected", Some(format!("{error:#}")))?
                    }
                }
            }
        }
        self.emit_simulation(changes)
    }
    fn emit_simulation(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        if self.initialized {
            let epoch = self.epoch.context("missing epoch")?;
            let executions = self
                .simulation
                .execution()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let keys = executions
                .iter()
                .map(|e| e.id.clone())
                .collect::<BTreeSet<_>>();
            changes.extend(self.emitter.retain("PolicyEnforcement", &keys)?);
            self.published_locations.retain(|id, _| keys.contains(id));
            let mut actual = Vec::new();
            for execution in executions {
                let mut value = serde_json::to_value(&execution.assignment)?;
                let object = value
                    .as_object_mut()
                    .context("assignment must be an object")?;
                object.extend(
                    serde_json::to_value(&execution)?
                        .as_object()
                        .context("enforcement must be an object")?
                        .clone(),
                );
                object.insert("observation_epoch".into(), json!(epoch));
                object.insert(
                    "applied_plan_version".into(),
                    json!(self.simulation.application.applied_plan_version),
                );
                object.insert(
                    "acknowledged_at_ms".into(),
                    json!(self.utc_at(execution.acknowledged_monotonic_ms)?),
                );
                if let Some(change) =
                    self.emitter
                        .record("PolicyEnforcement", &execution.id, &value)?
                {
                    changes.push(change);
                    let config = &self
                        .simulation_inputs
                        .as_ref()
                        .context("execution inputs missing")?
                        .configuration;
                    let assignment = &execution.assignment;
                    let name = config
                        .workloads
                        .get(&assignment.workload_id)
                        .map(|workload| workload.name.clone())
                        .unwrap_or_else(|| assignment.workload_id.to_string());
                    let destination = config
                        .gpus
                        .get(&assignment.gpu_id)
                        .map(|gpu| format!("{} / GPU {}", gpu.host_id, gpu.gpu_index))
                        .unwrap_or_else(|| assignment.gpu_id.to_string());
                    let previous = self.published_locations.insert(
                        execution.id.clone(),
                        (assignment.gpu_id, destination.clone()),
                    );
                    let replica = format!("{name} replica {}", assignment.replica_index + 1);
                    let (kind, message) = match execution.state {
                        gpu_simulator::Execution::Running => match previous {
                            Some((gpu, origin)) if gpu != assignment.gpu_id => (
                                "replica-moved",
                                format!("{replica} moved from {origin} to {destination}."),
                            ),
                            _ => (
                                "execution-running",
                                format!("{replica} is running on {destination}."),
                            ),
                        },
                        gpu_simulator::Execution::Suspended => (
                            "suspended",
                            format!("{replica} paused on {destination}; memory retained."),
                        ),
                        gpu_simulator::Execution::Fenced => (
                            "fenced",
                            format!(
                                "{replica} stopped by policy on {destination}; resources released."
                            ),
                        ),
                    };
                    let decision = self
                        .simulation_inputs
                        .as_ref()
                        .filter(|input| {
                            self.simulation.application.applied_plan_version.as_deref()
                                == Some(input.plan.plan_version.to_string().as_str())
                        })
                        .map(|input| input.plan.decision_id);
                    self.workers.hub.event(
                        epoch,
                        self.descriptor.id().as_str(),
                        kind,
                        message,
                        decision,
                        self.simulation.application.applied_plan_version.clone(),
                    )?;
                }
                actual.push(value);
            }
            self.add(
                changes,
                "AppliedPlan",
                "demo",
                &json!({"schema_version":1,"fleet_id":"demo",
                "observation_epoch":epoch,
                "source_ready":self.source_ready,
                "config_fingerprint":self.simulation_inputs.as_ref().map(|input| input.configuration.fingerprint()).transpose()?,
                "application":self.simulation.application,"execution":actual}),
            )?;
            let applied = self.simulation.application.applied_plan_version.clone();
            if applied != self.published_plan {
                self.workers.hub.event(
                    epoch,
                    self.descriptor.id().as_str(),
                    "applied",
                    "Simulator applied the complete plan. Fresh GPU reports are still required."
                        .into(),
                    self.simulation_inputs
                        .as_ref()
                        .map(|input| input.plan.decision_id),
                    applied.clone(),
                )?;
                self.published_plan = applied;
            }
        }
        Ok(())
    }
    fn prepare_scenarios(&mut self, input: &SolveInput, signature: String) -> Result<()> {
        let mut workers = BTreeSet::new();
        let mut regions = BTreeSet::new();
        for g in input
            .configuration
            .gpus
            .values()
            .filter(|g| input.capacities[&g.gpu_id].eligible)
        {
            workers.insert(g.host_id.clone());
            regions.insert(input.configuration.clusters[&g.cluster_id].region.clone());
        }
        self.scenarios = workers
            .into_iter()
            .map(|s| (LossKind::Worker, s))
            .chain(regions.into_iter().map(|s| (LossKind::Region, s)))
            .collect();
        self.assessment = Some(Resilience {
            scheduling_signature: signature,
            policy_signature: input.policy.policy_signature.clone(),
            workers: vec![],
            regions: vec![],
        });
        Ok(())
    }
    async fn finish_job(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        if !self.job.as_ref().is_some_and(|job| job.task.is_finished()) {
            return Ok(());
        }
        let active = self.job.take().context("worker disappeared")?;
        let result = active.task.await.context("native worker panicked")?;
        let current = if self.kind == Kind::Policy {
            self.policy_signature.as_ref()
        } else {
            self.signature.as_ref()
        };
        if current != Some(&active.signature)
            || active.generation != self.generation
            || !self.source_ready
        {
            return Ok(());
        }
        if self.kind == Kind::Placement {
            self.workers
                .placement_pending
                .store(false, Ordering::Release);
        }
        let job = match result {
            Ok(job) => job,
            Err(error) => {
                self.completed = Some(active.signature);
                return self.invalidate(&format!("worker failed: {error:#}"), changes);
            }
        };
        match job {
            Job::Policy(assessment) => {
                self.completed = Some(active.signature.clone());
                let message = Message::Policy {
                    epoch: self.epoch.context("missing epoch")?,
                    config_fingerprint: active.signature,
                    assessment: assessment.clone(),
                };
                self.add(changes, "PolicyAssessment", "demo", &message)?;
                self.policy_rows(changes, Some(&assessment))?;
                if assessment
                    .pairs
                    .values()
                    .any(|p| p.authorization == gpu_policy::Authorization::Unknown)
                {
                    self.status(
                        changes,
                        "policy-unknown",
                        Some("One or more policy contexts are unknown".into()),
                    )?;
                } else {
                    self.status(changes, "policy-current", None)?;
                }
            }
            Job::Placement(outcome) => {
                self.workers
                    .placement_pending
                    .store(false, Ordering::Release);
                self.completed = Some(active.signature);
                let input = self
                    .solve_input
                    .clone()
                    .context("placement input missing")?;
                match outcome {
                    Outcome::Feasible {
                        ref assignments, ..
                    } if assignments == &input.plan.assignments
                        && input.plan.config_fingerprint
                            == input.configuration.fingerprint()?
                        && input.plan.policy_signature == input.policy.policy_signature =>
                    {
                        self.status(changes, "plan-current", None)?;
                    }
                    feasible @ Outcome::Feasible { .. } => {
                        let mut candidate = gpu_placement::candidate(
                            &input.configuration,
                            &input.policy,
                            &input.capacities,
                            &input.plan,
                            feasible,
                            vec!["scheduling-input-changed".into()],
                        )?;
                        let epoch = self.epoch.context("missing epoch")?;
                        candidate.decision_details["observation_epoch"] = json!(epoch);
                        let decision = Decision::candidate(&candidate, epoch)?;
                        self.add(changes, "CandidatePlan", "demo", &candidate)?;
                        self.decision(changes, &decision)?;
                        self.workers.hub.event(
                            epoch,
                            self.descriptor.id().as_str(),
                            "candidate",
                            decision.summary,
                            Some(candidate.decision_id),
                            None,
                        )?;
                        self.status(changes, "candidate-produced", None)?;
                    }
                    Outcome::Infeasible { reason } => {
                        self.diagnostic_pending = true;
                        let epoch = self.epoch.context("missing epoch")?;
                        let decision = Decision::diagnostic(
                            epoch,
                            &input,
                            DecisionOutcome::Infeasible,
                            reason.clone(),
                            "no-complete-plan",
                        )?;
                        self.decision(changes, &decision)?;
                        self.workers.hub.event(
                            epoch,
                            self.descriptor.id().as_str(),
                            "infeasible",
                            reason.clone(),
                            Some(decision.decision_id),
                            None,
                        )?;
                        self.status(changes, "infeasible", Some(reason))?
                    }
                    Outcome::Unknown { error } => {
                        let epoch = self.epoch.context("missing epoch")?;
                        let decision = Decision::diagnostic(
                            epoch,
                            &input,
                            DecisionOutcome::Unknown,
                            error.clone(),
                            "solver-unknown",
                        )?;
                        self.decision(changes, &decision)?;
                        self.workers.hub.event(
                            epoch,
                            self.descriptor.id().as_str(),
                            "solver-unknown",
                            error.clone(),
                            Some(decision.decision_id),
                            None,
                        )?;
                        self.status(changes, "unknown", Some(error))?;
                    }
                }
            }
            Job::Scenario(kind, scenario) => {
                let assessment = self
                    .assessment
                    .as_mut()
                    .context("resilience batch missing")?;
                match kind {
                    LossKind::Worker => assessment.workers.push(scenario),
                    LossKind::Region => assessment.regions.push(scenario),
                }
            }
            Job::CapacityOnly(scenario) => {
                self.add(
                    changes,
                    "CapacityDiagnostic",
                    "demo",
                    &json!({"schema_version":1,
                    "observation_epoch":self.epoch,
                    "policy_signature":self.solve_input.as_ref().map(|input| &input.policy.policy_signature),
                    "scheduling_signature":self.signature,"diagnostic":scenario}),
                )?;
            }
        }
        Ok(())
    }
    fn launch_job(&mut self) -> Result<()> {
        if self.job.is_some() || !self.source_ready || Instant::now() < self.due {
            return Ok(());
        }
        let signature = if self.kind == Kind::Policy {
            self.policy_signature.clone()
        } else {
            self.signature.clone()
        };
        let Some(signature) = signature else {
            return Ok(());
        };
        if self.completed.as_ref() == Some(&signature) && !self.diagnostic_pending {
            return Ok(());
        }
        if self.kind == Kind::Policy {
            let Ok(permit) = self.workers.policy.clone().try_acquire_owned() else {
                return Ok(());
            };
            let config = self.policy_input.clone().context("policy input missing")?;
            self.job = Some(Active {
                signature,
                generation: self.generation,
                task: tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    Ok(Job::Policy(Evaluator::new()?.evaluate(&config)?))
                }),
            });
        } else if self.kind == Kind::Placement || self.kind == Kind::Resilience {
            if self.kind == Kind::Resilience
                && self.workers.placement_pending.load(Ordering::Acquire)
            {
                return Ok(());
            }
            let Ok(permit) = self.workers.solver.clone().try_acquire_owned() else {
                return Ok(());
            };
            let input = self
                .solve_input
                .clone()
                .context("scheduling input missing")?;
            let diagnostic = self.kind == Kind::Placement && self.diagnostic_pending;
            self.diagnostic_pending = false;
            let scenario = if self.kind == Kind::Resilience {
                match self.scenarios.pop_front() {
                    Some(s) => Some(s),
                    None => return Ok(()),
                }
            } else {
                None
            };
            self.job = Some(Active {
                signature,
                generation: self.generation,
                task: tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    Ok(if diagnostic {
                        Job::CapacityOnly(gpu_placement::capacity_only(
                            &input.configuration,
                            &input.capacities,
                            Duration::from_secs(2),
                        )?)
                    } else if let Some((kind, excluded)) = scenario {
                        Job::Scenario(
                            kind,
                            gpu_placement::assess_loss(
                                &input.configuration,
                                &input.policy,
                                &input.capacities,
                                kind,
                                &excluded,
                                Duration::from_secs(2),
                            )?,
                        )
                    } else {
                        Job::Placement(gpu_placement::solve(
                            &input.configuration,
                            &input.policy,
                            &input.capacities,
                            &input.plan.assignments,
                            Duration::from_secs(2),
                        )?)
                    })
                }),
            });
        }
        Ok(())
    }
}
#[async_trait]
impl ComputationComponent for Processor {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(self.configuration.clone())
    }
    async fn start(&mut self) -> Result<()> {
        ensure!(
            !self.stopped,
            "stopped transformer requires fresh factory construction and bootstrap"
        );
        ensure!(self.started.is_none(), "component already started");
        let started_utc_ms = wire::utc_millis()?;
        self.started = Some(Instant::now());
        self.started_utc_ms = Some(started_utc_ms);
        self.timer.running.store(true, Ordering::Release);
        self.workers
            .hub
            .publish(self.descriptor.id().as_str(), "running", None, self.epoch)
    }
    async fn stop(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.timer.running.store(false, Ordering::Release);
        self.source_ready = false;
        self.recovering_source = false;
        if self.initialized {
            self.simulation
                .invalidate_source(self.now()?, "component-stopped");
        }
        self.started = None;
        self.started_utc_ms = None;
        self.stopped = true;
        if self.kind == Kind::Placement {
            self.workers
                .placement_pending
                .store(false, Ordering::Release);
        }
        if let Some(job) = self.job.take() {
            job.task.await.context("worker failed during shutdown")??;
        }
        self.workers
            .hub
            .publish(self.descriptor.id().as_str(), "stopped", None, self.epoch)
    }
}
#[async_trait]
impl Transformer for Processor {
    fn wakeup_source(&self) -> Option<Arc<dyn WakeupSource>> {
        Some(self.timer.clone())
    }
    async fn transform(&mut self, input: InputEnvelope) -> Result<Vec<OutputEnvelope>> {
        self.now()?;
        let mut changes = Vec::new();
        let query = QueryChangeCodec::metadata(&input.envelope)?.query_id;
        if DATABASE_QUERIES.contains(&query.as_str()) {
            ensure!(
                matches!(self.kind, Kind::Simulator | Kind::Policy),
                "unexpected database input"
            );
            self.database.update(
                &query,
                QueryChangeCodec::query_sequence(&input.envelope)?,
                input.envelope.changes().operations(),
            )?;
            self.observe_inputs(&mut changes)?;
            self.launch_job()?;
            return self.emitter.emit(changes, Some(&input.envelope));
        }
        if query == "simulation-inputs" || query == "scheduling-inputs" {
            for operation in input.envelope.changes().operations() {
                let image = match operation {
                    ChangeOperation::Added { after, .. }
                    | ChangeOperation::Updated { after, .. } => after,
                    ChangeOperation::Deleted { .. } => {
                        self.invalidate("Current input query context was removed", &mut changes)?;
                        continue;
                    }
                };
                let row = QueryChangeCodec::decode_row(image)?;
                let result = (|| -> Result<()> {
                    let value = serde_json::to_value(row.values)?;
                    if query == "scheduling-inputs" {
                        let row: SchedulingRow = serde_json::from_value(value)?;
                        self.accept(row.message()?, &mut changes)
                    } else {
                        ensure!(
                            self.kind == Kind::Simulator,
                            "simulation input sent to another transformer"
                        );
                        let row: SimulationRow = serde_json::from_value(value)?;
                        let epoch = row.snapshot.epoch;
                        self.accept_database(row.snapshot, &mut changes)?;
                        if let Some(assessment) = row.policy {
                            ensure!(row.policy_epoch == Some(epoch), "policy epoch is stale");
                            let config_fingerprint = self
                                .simulation_inputs
                                .as_ref()
                                .context("simulation input missing")?
                                .configuration
                                .fingerprint()?;
                            self.accept(
                                Message::Policy {
                                    epoch,
                                    config_fingerprint,
                                    assessment,
                                },
                                &mut changes,
                            )?;
                        } else {
                            ensure!(row.policy_epoch.is_none(), "policy context is incomplete");
                            if let Some(inputs) = &mut self.simulation_inputs {
                                inputs.policy = None;
                            }
                            if self.initialized {
                                self.simulation.invalidate_policy(self.now()?)?;
                                self.emit_simulation(&mut changes)?;
                            }
                            self.status(&mut changes, "awaiting-policy", None)?;
                        }
                        Ok(())
                    }
                })();
                if let Err(error) = result {
                    self.invalidate(&format!("{error:#}"), &mut changes)?;
                }
            }
            self.launch_job()?;
            return self.emitter.emit(changes, Some(&input.envelope));
        }
        let messages = match wire::decode(&input) {
            Ok(messages) => messages,
            Err(error) => {
                self.invalidate(&format!("{error:#}"), &mut changes)?;
                return self.emitter.emit(changes, Some(&input.envelope));
            }
        };
        for message in messages {
            if let Err(error) = self.accept(message, &mut changes) {
                self.invalidate(&format!("{error:#}"), &mut changes)?;
            }
        }
        self.launch_job()?;
        self.emitter.emit(changes, Some(&input.envelope))
    }
    async fn on_wakeup(&mut self) -> Result<Vec<OutputEnvelope>> {
        let now = self.now()?;
        let mut changes = Vec::new();
        self.observe_inputs(&mut changes)?;
        self.finish_job(&mut changes).await?;
        if self.kind == Kind::Simulator && self.initialized {
            for sample in self.simulation.tick(now, wire::utc_millis()?)? {
                self.add(
                    &mut changes,
                    "GpuSample",
                    &sample.gpu_id.to_string(),
                    &sample,
                )?;
            }
            self.emit_simulation(&mut changes)?;
        }
        if self.kind == Kind::Resilience
            && self.job.is_none()
            && self.scenarios.is_empty()
            && self.source_ready
            && self.completed != self.signature
        {
            if let Some(assessment) = self.assessment.clone() {
                let mut value = serde_json::to_value(assessment)?;
                value["observation_epoch"] = json!(self.epoch);
                value["status"] = json!("current");
                self.add(&mut changes, "ResilienceAssessment", "demo", &value)?;
                self.completed = self.signature.clone();
                self.status(&mut changes, "resilience-current", None)?;
            }
        }
        self.launch_job()?;
        self.emitter.emit(changes, None)
    }
}
