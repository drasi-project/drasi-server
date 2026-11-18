use crate::{
    evidence::{Decision, DecisionOutcome, DecisionStage},
    inputs::{DatabaseInputs, DatabaseSnapshot, SimulationRow, DATABASE_QUERIES},
    lifecycle::Signals,
    planner::{self, Outcome, Planner, QueryRows},
    wire::{self, Emitter, Message},
    Kind, Workers,
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_computation_contracts::{
    ChangeOperation, ComponentDescriptor, InputEnvelope, OutputEnvelope, QueryChangeCodec,
    StreamId, WakeupSource,
};
use drasi_computation_plugin_sdk::lifecycle::{
    nothing_to_do, DisposeContext, InitializeContext, LifecycleResult, PauseContext, ResumeContext,
    StartContext, StopContext,
};
use drasi_computation_plugin_sdk::{wire::Scope, NativeComponent, NativeTransformer};
use drasi_core_types::models::SourceChange;
use gpu_contracts::{policy::Assessment, *};
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
    configured: bool,
    planner: Planner,
    policy_rows: QueryRows,
    policy_due: Option<Instant>,
    decisions: VecDeque<String>,
    published_plan: Option<String>,
    published_locations: BTreeMap<String, (Uuid, String)>,
}
impl Processor {
    pub fn new(
        kind: Kind,
        descriptor: ComponentDescriptor,
        configuration: serde_json::Value,
        workers: Arc<Workers>,
        scope: Option<&Scope>,
    ) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Config {
            stream: StreamId,
        }
        let config: Config = serde_json::from_value(configuration.clone())?;
        let emitter = Emitter::new(config.stream, descriptor.id().clone(), scope)?;
        Ok(Self {
            signals: Arc::new(Mutex::new(Signals::default())),
            database: DatabaseInputs::default(),
            kind,
            descriptor,
            configuration,
            emitter,
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
            configured: false,
            planner: Planner::default(),
            policy_rows: QueryRows::default(),
            policy_due: None,
            decisions: VecDeque::new(),
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
        let (bootstrap, unavailable) = {
            let mut signals = self
                .signals
                .lock()
                .map_err(|_| anyhow::anyhow!("lifecycle signal lock poisoned"))?;
            signals.take()
        };
        if let Some(reason) = unavailable {
            self.database.unavailable();
            self.invalidate(&reason, changes)?;
        }
        if let Some(boundary) = bootstrap {
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
                Kind::Configuration => !self.configured,
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
        if self.kind == Kind::Configuration {
            self.publish_database(&snapshot, changes)?;
            let mut value = serde_json::to_value(&snapshot)?;
            value["config_fingerprint"] = json!(snapshot.configuration.fingerprint()?);
            value["plan_version"] = json!(snapshot.plan.plan_version.to_string());
            self.add(changes, "FleetConfiguration", "demo", &value)?;
            self.status(changes, "configuration-current", None)?;
        }
        Ok(())
    }
    fn publish_database(
        &mut self,
        snapshot: &DatabaseSnapshot,
        changes: &mut Vec<SourceChange>,
    ) -> Result<()> {
        let value = serde_json::to_value(snapshot)?;
        for (label, rows) in [
            ("regional_clusters", &value["configuration"]["clusters"]),
            ("placement_policies", &value["configuration"]["policies"]),
            ("data_profiles", &value["configuration"]["data_profiles"]),
            ("gpu_inventory", &value["configuration"]["gpus"]),
            (
                "workload_requirements",
                &value["configuration"]["workloads"],
            ),
            ("gpu_telemetry", &value["settings"]),
        ] {
            let rows = rows
                .as_object()
                .context("database projection must be keyed")?;
            changes.extend(
                self.emitter
                    .retain(label, &rows.keys().cloned().collect())?,
            );
            for (key, row) in rows {
                self.add(changes, label, key, row)?;
            }
        }
        self.add(changes, "gpu_placements", "demo", &snapshot.plan)?;
        let assignments: BTreeMap<String, _> = snapshot
            .plan
            .assignments
            .iter()
            .map(|a| {
                (
                    format!("{}/{}", a.workload_id, a.replica_index),
                    json!({"workload_id":a.workload_id,"replica_index":a.replica_index,"gpu_id":a.gpu_id}),
                )
            })
            .collect();
        changes.extend(
            self.emitter
                .retain("PlanAssignment", &assignments.keys().cloned().collect())?,
        );
        for (key, row) in &assignments {
            self.add(changes, "PlanAssignment", key, row)?;
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
        let context = self.planner.context.clone();
        if self.kind == Kind::Placement {
            let value = json!({
                "fleet_id":"demo", "observation_epoch":self.epoch,
                "scheduling_signature":context.as_ref().map(|c| &c.scheduling_signature),
                "policy_signature":context.as_ref().map(|c| &c.policy_signature),
                "policy_bundle_hash":context.as_ref().map(|c| &c.policy_bundle_hash),
                "config_fingerprint":context.as_ref().map(|c| &c.config_fingerprint),
                "required_replicas":context.as_ref().map(|c| c.required_replicas),
                "current":self.source_ready && context.is_some(),
            });
            self.add(changes, "SchedulingContext", "demo", &value)?;
        }
        let component = gpu_contracts::component_label(self.descriptor.id().as_str());
        self.add(
            changes,
            "RuntimeStatus",
            &component,
            &json!({
                "schema_version":1,"component_id":component,"status":status,"error":error,
                "observation_epoch":self.epoch,
                "scheduling_signature":context.as_ref().map(|c| &c.scheduling_signature),
                "policy_signature":context.as_ref().map(|c| &c.policy_signature),
            }),
        )
    }
    fn invalidate(&mut self, reason: &str, changes: &mut Vec<SourceChange>) -> Result<()> {
        self.source_ready = false;
        self.recovering_source = false;
        self.configured = false;
        if self.kind == Kind::Placement {
            self.planner.invalidate();
        }
        if self.initialized {
            self.simulation.invalidate_source(self.now()?, reason);
        }
        for label in ["CandidatePlan", "FleetConfiguration"] {
            if let Some(change) = self.emitter.remove(label, "demo")? {
                changes.push(change);
            }
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
            (Kind::Configuration, Message::Bootstrap { commit, .. })
            | (Kind::Configuration, Message::Configuration { commit, .. }) => {
                nonempty(&commit)?;
                self.configured = true;
                self.source_ready = true;
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
}
#[async_trait]
impl NativeComponent for Processor {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(self.configuration.clone())
    }
    async fn initialize(&mut self, _ctx: &InitializeContext) -> LifecycleResult {
        nothing_to_do()
    }
    async fn start(&mut self, _ctx: &mut StartContext) -> LifecycleResult {
        async {
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
        .await
        .map_err(Into::into)
    }
    // The host pulls work, so paused components produce nothing.
    async fn pause(&mut self, _ctx: &PauseContext) -> LifecycleResult {
        nothing_to_do()
    }
    async fn resume(&mut self, _ctx: &ResumeContext) -> LifecycleResult {
        nothing_to_do()
    }
    async fn stop(&mut self, _ctx: &StopContext) -> LifecycleResult {
        async {
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
            self.workers
                .hub
                .publish(self.descriptor.id().as_str(), "stopped", None, self.epoch)
        }
        .await
        .map_err(Into::into)
    }
    async fn dispose(&mut self, _ctx: &DisposeContext) -> LifecycleResult {
        nothing_to_do()
    }
}
#[async_trait]
impl NativeTransformer for Processor {
    fn wakeup_source(&self) -> Option<Arc<dyn WakeupSource>> {
        Some(self.timer.clone())
    }
    async fn transform(&mut self, input: InputEnvelope) -> Result<Vec<OutputEnvelope>> {
        self.now()?;
        let mut changes = Vec::new();
        let query_id = QueryChangeCodec::metadata(&input.envelope)?.query_id;
        let query = gpu_contracts::component_name(
            query_id.as_str(),
            &[DATABASE_QUERIES.as_slice(), &PROCESSING_INPUTS].concat(),
        )
        .unwrap_or(query_id.as_str());
        if DATABASE_QUERIES.contains(&query) {
            ensure!(
                matches!(self.kind, Kind::Simulator | Kind::Configuration),
                "unexpected database input"
            );
            let projection = QueryChangeCodec::projection(&input.envelope)?;
            self.database.update(
                query,
                QueryChangeCodec::query_sequence(&input.envelope)?,
                input.envelope.changes().operations(),
                projection.operations(),
            )?;
            self.observe_inputs(&mut changes)?;
            return self.emitter.emit(changes, Some(&input.envelope));
        }
        let result = match query {
            "simulation-inputs" => self.simulation_input(&input, &mut changes),
            "planner-policy" => (|| -> Result<()> {
                ensure!(
                    matches!(self.kind, Kind::Simulator | Kind::Placement),
                    "policy decisions sent to another transformer"
                );
                if self.kind == Kind::Placement {
                    if self.planner.policy.apply(&input.envelope)? {
                        self.planner.touch(false);
                    }
                } else if self.policy_rows.apply(&input.envelope)? {
                    self.policy_due = Some(Instant::now() + POLICY_SETTLE);
                }
                Ok(())
            })(),
            "placement-gpus" | "planner-solution" | "planner-status" => (|| -> Result<()> {
                ensure!(
                    self.kind == Kind::Placement,
                    "planner input sent to another transformer"
                );
                let (rows, solver) = match query {
                    "placement-gpus" => (&mut self.planner.gpus, false),
                    "planner-solution" => (&mut self.planner.solution, true),
                    _ => (&mut self.planner.status, true),
                };
                if rows.apply(&input.envelope)? {
                    self.planner.touch(solver);
                }
                Ok(())
            })(),
            _ => match wire::decode(&input) {
                Ok(messages) => {
                    for message in messages {
                        if let Err(error) = self.accept(message, &mut changes) {
                            self.invalidate(&format!("{error:#}"), &mut changes)?;
                        }
                    }
                    Ok(())
                }
                Err(error) => Err(error),
            },
        };
        if let Err(error) = result {
            self.invalidate(&format!("{error:#}"), &mut changes)?;
        }
        self.emitter.emit(changes, Some(&input.envelope))
    }
    async fn on_wakeup(&mut self) -> Result<Vec<OutputEnvelope>> {
        let now = self.now()?;
        let mut changes = Vec::new();
        self.observe_inputs(&mut changes)?;
        if self.kind == Kind::Simulator {
            if self.policy_due.is_some_and(|due| Instant::now() >= due) {
                self.policy_due = None;
                if let Err(error) = self.observe_policy(&mut changes) {
                    self.invalidate(&format!("{error:#}"), &mut changes)?;
                }
            }
            if self.initialized {
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
        }
        if self.kind == Kind::Placement && self.source_ready {
            if let Err(error) = self.plan(&mut changes) {
                self.invalidate(&format!("{error:#}"), &mut changes)?;
            }
        }
        self.emitter.emit(changes, None)
    }
}

const PROCESSING_INPUTS: [&str; 5] = [
    "simulation-inputs",
    "placement-gpus",
    "planner-policy",
    "planner-solution",
    "planner-status",
];
const POLICY_SETTLE: Duration = Duration::from_millis(100);

impl Processor {
    fn simulation_input(
        &mut self,
        input: &InputEnvelope,
        changes: &mut Vec<SourceChange>,
    ) -> Result<()> {
        ensure!(
            matches!(self.kind, Kind::Simulator | Kind::Placement),
            "simulation input sent to another transformer"
        );
        for operation in input.envelope.changes().operations() {
            let image = match operation {
                ChangeOperation::Added { after, .. } | ChangeOperation::Updated { after, .. } => {
                    after
                }
                ChangeOperation::Deleted { .. } => {
                    self.invalidate("Current input query context was removed", changes)?;
                    continue;
                }
            };
            let row: SimulationRow = serde_json::from_value(serde_json::to_value(
                QueryChangeCodec::decode_row(image)?.values,
            )?)?;
            if self.kind == Kind::Placement {
                self.accept_plan_inputs(row.snapshot, changes)?;
            } else {
                self.accept_database(row.snapshot, changes)?;
                self.policy_due = Some(Instant::now() + POLICY_SETTLE);
            }
        }
        Ok(())
    }

    /// Re-derives the simulator's policy view from the current decisions.
    fn observe_policy(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        let Some(inputs) = &self.simulation_inputs else {
            return Ok(());
        };
        let epoch = self.epoch.context("missing epoch")?;
        match planner::assessment(&inputs.configuration, &self.policy_rows) {
            Ok(assessment) => {
                if inputs.policy.as_ref().map(|p| &p.policy_signature)
                    != Some(&assessment.policy_signature)
                {
                    let config_fingerprint = inputs.configuration.fingerprint()?;
                    self.accept(
                        Message::Policy {
                            epoch,
                            config_fingerprint,
                            assessment,
                        },
                        changes,
                    )?;
                }
            }
            Err(_) => {
                if let Some(inputs) = &mut self.simulation_inputs {
                    inputs.policy = None;
                }
                if self.initialized {
                    self.simulation.invalidate_policy(self.now()?)?;
                    self.emit_simulation(changes)?;
                }
                self.status(changes, "awaiting-policy", None)?;
            }
        }
        Ok(())
    }

    fn accept_plan_inputs(
        &mut self,
        snapshot: DatabaseSnapshot,
        changes: &mut Vec<SourceChange>,
    ) -> Result<()> {
        if let Some(epoch) = self.epoch {
            ensure!(
                epoch == snapshot.epoch,
                "new observation epoch requires fresh component construction"
            );
        } else {
            self.epoch = Some(snapshot.epoch);
        }
        snapshot.configuration.validate()?;
        let decision = Decision::committed(&snapshot.plan, snapshot.epoch)?;
        self.decision(changes, &decision)?;
        self.planner.snapshot = Some(snapshot);
        self.planner.touch(false);
        if !self.source_ready {
            self.source_ready = true;
            self.status(changes, "pending", None)?;
        }
        Ok(())
    }

    fn plan(&mut self, changes: &mut Vec<SourceChange>) -> Result<()> {
        let Some(outcome) = self.planner.evaluate()? else {
            return Ok(());
        };
        let epoch = self.epoch.context("missing epoch")?;
        if !matches!(outcome, Outcome::Candidate(_) | Outcome::Waiting(..)) {
            if let Some(change) = self.emitter.remove("CandidatePlan", "demo")? {
                changes.push(change);
            }
        }
        match outcome {
            Outcome::Waiting(status, error) => self.status(changes, status, error),
            Outcome::Current => self.status(changes, "plan-current", None),
            Outcome::Candidate(candidate) => {
                let mut candidate = *candidate;
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
                self.status(changes, "candidate-produced", None)
            }
            Outcome::Infeasible(reason) | Outcome::Unknown(reason) => {
                let infeasible = reason == planner::INFEASIBLE;
                let context = self
                    .planner
                    .context
                    .clone()
                    .context("scheduling context missing")?;
                let configuration = &self
                    .planner
                    .snapshot
                    .as_ref()
                    .context("configuration missing")?
                    .configuration;
                let decision = Decision::diagnostic(
                    epoch,
                    configuration,
                    &context.policy_signature,
                    &context.scheduling_signature,
                    if infeasible {
                        DecisionOutcome::Infeasible
                    } else {
                        DecisionOutcome::Unknown
                    },
                    reason.clone(),
                    if infeasible {
                        "no-complete-plan"
                    } else {
                        "solver-unknown"
                    },
                )?;
                self.decision(changes, &decision)?;
                self.workers.hub.event(
                    epoch,
                    self.descriptor.id().as_str(),
                    if infeasible {
                        "infeasible"
                    } else {
                        "solver-unknown"
                    },
                    reason.clone(),
                    Some(decision.decision_id),
                    None,
                )?;
                self.status(
                    changes,
                    if infeasible { "infeasible" } else { "unknown" },
                    Some(reason),
                )
            }
        }
    }
}
