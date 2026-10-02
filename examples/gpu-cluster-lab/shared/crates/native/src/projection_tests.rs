use super::*;
use crate::projections as views;
use drasi_lib::channels::ResultDiff;
use serde_json::{json, Value};

#[tokio::test]
async fn every_ui_query_constructs_with_its_declared_joins() -> Result<()> {
    let definitions = views::definitions();
    let actual: std::collections::BTreeSet<_> = definitions.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(actual, gpu_contracts::UI_QUERIES.into_iter().collect());
    for (id, text, settings) in definitions {
        eprintln!("Constructing {id}");
        configured_query(id, text, settings)
            .await
            .with_context(|| format!("constructing {id}"))?
            .stop()
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn shared_data_policy_rules_are_projected_from_configuration_with_currentness() -> Result<()>
{
    let mut configuration = fixtures::load("regional-boundary")?.configuration;
    let baseline = fixtures::load("baseline")?.configuration;
    configuration.policies.extend(baseline.policies);
    configuration.data_profiles.extend(baseline.data_profiles);
    configuration.validate()?;
    let epoch = Uuid::new_v4();
    let assessment = Evaluator::new()?.evaluate(&configuration)?;
    let mut source = Emitter::new(StreamId::try_new("shared-policy-projection")?);
    let mut query = query("shared-policy-status", views::STATUS).await?;
    let mut ready = json!({
        "fleet_id":"demo", "observation_epoch":epoch,
        "scheduling_signature":"schedule", "policy_signature":assessment.policy_signature,
        "scenario_ready":true, "inputs_ready":true, "scenario":"regional-boundary",
        "state":"ready", "detail":"Observed current inputs", "components":[],
    });
    let mut changes = Vec::new();
    changes.extend(source.record("DemoReadiness", "demo", &ready)?);
    let rows = project(&mut query, &source.emit(changes, None)?).await?;
    assert!(rows[0]["policy_rules"].is_null());
    assert_eq!(rows[0]["policy_rules_current"], false);

    let mut changes = Vec::new();
    changes.extend(source.record(
        "FleetConfiguration",
        "demo",
        &json!({
            "epoch":epoch, "configuration":configuration,
            "config_fingerprint":configuration.fingerprint()?,
        }),
    )?);
    changes.extend(source.record(
        "PolicyAssessment",
        "demo",
        &json!({
            "epoch":epoch, "config_fingerprint":configuration.fingerprint()?,
            "assessment":assessment,
        }),
    )?);
    let rows = project(&mut query, &source.emit(changes, None)?).await?;
    assert_eq!(
        rows[0]["policy_rules"],
        serde_json::to_value(&configuration.policies)?
    );
    assert_eq!(
        rows[0]["data_profiles"],
        serde_json::to_value(&configuration.data_profiles)?
    );
    assert_eq!(rows[0]["policy_rules_current"], true);
    assert_eq!(
        rows[0]["policy_rules"]["demo-permissive"]["allowed_purposes"],
        json!(["demo"])
    );
    assert_eq!(
        rows[0]["data_profiles"]["customer-eu-documents"]["classification"],
        "restricted"
    );

    let rule = configuration
        .policies
        .get_mut("customer-eu-processing")
        .unwrap();
    rule.allowed_regions = ["northeurope".into()].into_iter().collect();
    rule.revision += 1;
    let assessment = Evaluator::new()?.evaluate(&configuration)?;
    let mut changes = Vec::new();
    changes.extend(source.record(
        "FleetConfiguration",
        "demo",
        &json!({
            "epoch":epoch, "configuration":configuration,
            "config_fingerprint":configuration.fingerprint()?,
        }),
    )?);
    let rows = project(&mut query, &source.emit(changes, None)?).await?;
    assert_eq!(
        rows[0]["policy_rules"]["customer-eu-processing"]["allowed_regions"],
        json!(["northeurope"])
    );
    assert_eq!(
        rows[0]["policy_rules_current"], false,
        "new settings are not an evaluated decision"
    );
    ready["policy_signature"] = json!(assessment.policy_signature);
    let mut changes = Vec::new();
    changes.extend(source.record("DemoReadiness", "demo", &ready)?);
    changes.extend(source.record(
        "PolicyAssessment",
        "demo",
        &json!({
            "epoch":epoch, "config_fingerprint":configuration.fingerprint()?,
            "assessment":assessment,
        }),
    )?);
    let rows = project(&mut query, &source.emit(changes, None)?).await?;
    assert_eq!(rows[0]["policy_rules_current"], true);
    ready["observation_epoch"] = json!(Uuid::new_v4());
    let changes = source
        .record("DemoReadiness", "demo", &ready)?
        .into_iter()
        .collect();
    let rows = project(&mut query, &source.emit(changes, None)?).await?;
    assert!(
        rows[0]["policy_rules"].is_null(),
        "a previous runtime's criteria must not become current"
    );
    assert!(rows[0]["data_profiles"].is_null());
    assert_eq!(rows[0]["policy_rules_current"], false);
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn timeline_is_bounded_and_preserves_explicit_observation_epochs() -> Result<()> {
    let hub = Arc::new(crate::status::Hub::default());
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: hub.clone(),
    };
    let mut producer = crate::status::Producer::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("bounded-timeline")?)?,
        json!({"stream":"bounded-timeline-source"}),
        hub.clone(),
    )?;
    producer.start().await?;
    let epoch = Uuid::new_v4();
    for index in 0..130 {
        hub.event(
            epoch,
            "simulator",
            "suspended",
            format!("Acknowledged pause {index}"),
            None,
            None,
        )?;
    }
    let mut timeline = query("bounded-ui-timeline", views::TIMELINE).await?;
    let output = producer.next().await?.context("timeline output missing")?;
    let rows = project(&mut timeline, &[output]).await?;
    assert_eq!(rows.len(), 128);
    assert!(rows
        .iter()
        .all(|row| row["observation_epoch"] == epoch.to_string()));
    let next_epoch = Uuid::new_v4();
    hub.event(
        next_epoch,
        "simulator",
        "applied",
        "Applied after explicit new bootstrap".into(),
        None,
        Some("2".into()),
    )?;
    let output = producer
        .next()
        .await?
        .context("new timeline output missing")?;
    let rows = project(&mut timeline, &[output]).await?;
    assert_eq!(
        rows.len(),
        128,
        "expired events must be deleted from the CQ"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["observation_epoch"] == next_epoch.to_string())
            .count(),
        1
    );
    assert!(rows.iter().all(|row| row["event_id"]
        == format!(
            "{}/{}",
            row["observation_epoch"].as_str().unwrap(),
            row["event_sequence"].as_str().unwrap()
        )));
    producer.stop().await?;
    timeline.stop().await?;
    Ok(())
}

#[tokio::test]
async fn fresh_runtime_producer_does_not_reuse_prior_epoch_evidence() -> Result<()> {
    let hub = Arc::new(crate::status::Hub::default());
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: hub.clone(),
    };
    let descriptor = factory
        .metadata()
        .descriptor(ComponentId::try_new("runtime-status")?)?;
    let mut first = crate::status::Producer::new(
        descriptor.clone(),
        json!({"stream":"first-status"}),
        hub.clone(),
    )?;
    let mut second =
        crate::status::Producer::new(descriptor, json!({"stream":"second-status"}), hub.clone())?;
    first.start().await?;
    assert!(
        second.start().await.is_err(),
        "concurrent runtime owners must be rejected"
    );
    let epoch = Uuid::new_v4();
    hub.event(
        epoch,
        "simulator",
        "applied",
        "old runtime event".into(),
        None,
        Some("1".into()),
    )?;
    hub.observe_runtime(RuntimeObservation {
        sequence: 1,
        observation_epoch: epoch,
        scenario: "baseline".into(),
        scheduling_signature: "schedule".into(),
        policy_signature: "policy".into(),
        source_bootstrap_complete: true,
        query_bootstrap_complete: true,
        query_results_current: true,
        reset_in_progress: false,
        detail: "Prior runtime test observation".into(),
        required_components: ["runtime-status".into()].into_iter().collect(),
        components: vec![RuntimeComponent {
            component_id: "runtime-status".into(),
            status: "running".into(),
            error: None,
        }],
    })?;
    let mut old_timeline = query("old-runtime-timeline", views::TIMELINE).await?;
    let old = first.next().await?.context("old runtime output missing")?;
    assert_eq!(
        project(&mut old_timeline, std::slice::from_ref(&old))
            .await?
            .len(),
        1
    );
    old_timeline.stop().await?;
    let mut old_status = query("old-runtime-status", views::STATUS).await?;
    assert_eq!(
        project(&mut old_status, &[old]).await?[0]["scenario_ready"],
        true
    );
    old_status.stop().await?;
    first.stop().await?;
    assert!(
        first.start().await.is_err(),
        "restart requires reconstruction"
    );
    second.start().await?;
    first.stop().await?;
    let output = second.next().await?.context("new runtime status missing")?;
    let mut timeline = query("new-runtime-timeline", views::TIMELINE).await?;
    assert!(project(&mut timeline, std::slice::from_ref(&output))
        .await?
        .is_empty());
    timeline.stop().await?;
    let mut query = query("fresh-status", views::STATUS).await?;
    let rows = project(&mut query, &[output]).await?;
    assert_eq!(rows[0]["scenario_ready"], false);
    assert!(rows[0]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["component_id"] == "runtime-status"
            && component["status"] == "running"));
    query.stop().await?;
    second.stop().await?;
    Ok(())
}

#[tokio::test]
async fn runtime_readiness_requires_complete_lifecycle_observations() -> Result<()> {
    let workers = Arc::new(Workers::default());
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: workers.hub.clone(),
    };
    let mut producer = crate::status::Producer::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("runtime-status")?)?,
        json!({"stream":"readiness-test"}),
        workers.hub.clone(),
    )?;
    producer.start().await?;
    let mut query = query("readiness-test", views::STATUS).await?;
    let output = producer.next().await?.context("missing status output")?;
    let rows = project(&mut query, &[output]).await?;
    assert_eq!(rows[0]["scenario_ready"], false);
    let mut observation = RuntimeObservation {
        sequence: 1,
        observation_epoch: Uuid::new_v4(),
        scenario: "baseline".into(),
        scheduling_signature: "current-schedule".into(),
        policy_signature: "current-policy".into(),
        source_bootstrap_complete: false,
        query_bootstrap_complete: true,
        query_results_current: false,
        reset_in_progress: false,
        detail: "Lifecycle observation under test".into(),
        required_components: std::collections::BTreeSet::from(["required-query".into()]),
        components: vec![RuntimeComponent {
            component_id: "required-query".into(),
            status: "running".into(),
            error: None,
        }],
    };
    workers.observe_runtime(observation.clone())?;
    let output = producer
        .next()
        .await?
        .context("missing source-pending output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        false
    );
    observation.sequence += 1;
    observation.source_bootstrap_complete = true;
    observation.query_bootstrap_complete = false;
    workers.observe_runtime(observation.clone())?;
    let output = producer
        .next()
        .await?
        .context("missing query-pending output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        false
    );
    observation.sequence += 1;
    observation.query_bootstrap_complete = true;
    workers.observe_runtime(observation.clone())?;
    let output = producer
        .next()
        .await?
        .context("missing projection-pending output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        false
    );
    observation.sequence += 1;
    observation.query_results_current = true;
    workers.observe_runtime(observation.clone())?;
    let output = producer.next().await?.context("missing ready output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        true
    );
    workers.hub.publish(
        "required-query",
        "stopped",
        None,
        Some(observation.observation_epoch),
    )?;
    let output = producer.next().await?.context("missing stopped output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        false
    );
    let stale = observation.clone();
    observation.sequence += 1;
    observation.observation_epoch = Uuid::new_v4();
    observation.reset_in_progress = true;
    workers.observe_runtime(observation.clone())?;
    assert!(
        workers.observe_runtime(stale).is_err(),
        "late readiness cannot overwrite a new run"
    );
    let output = producer.next().await?.context("missing reset output")?;
    let rows = project(&mut query, &[output]).await?;
    assert_eq!(rows[0]["scenario_ready"], false);
    assert_eq!(
        rows[0]["observation_epoch"],
        observation.observation_epoch.to_string()
    );
    observation.sequence += 1;
    observation.reset_in_progress = false;
    observation.components.clear();
    workers.observe_runtime(observation.clone())?;
    let output = producer
        .next()
        .await?
        .context("missing component-pending output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        false
    );
    observation.sequence += 1;
    observation.components.push(RuntimeComponent {
        component_id: "required-query".into(),
        status: "running".into(),
        error: None,
    });
    workers.observe_runtime(observation)?;
    let output = producer
        .next()
        .await?
        .context("missing new-run ready output")?;
    assert_eq!(
        project(&mut query, &[output]).await?[0]["scenario_ready"],
        true,
        "a stopped component from an old epoch cannot override current host evidence"
    );
    producer.stop().await?;
    query.stop().await?;
    Ok(())
}

#[test]
fn readiness_requires_current_query_outcomes_not_only_running_components() -> Result<()> {
    let observation = RuntimeObservation {
        sequence: 1,
        observation_epoch: Uuid::new_v4(),
        scenario: "baseline".into(),
        scheduling_signature: "schedule".into(),
        policy_signature: "policy".into(),
        source_bootstrap_complete: true,
        query_bootstrap_complete: true,
        query_results_current: false,
        reset_in_progress: false,
        detail: "projection test".into(),
        required_components: Default::default(),
        components: vec![],
    };
    let context = json!({"observation_epoch":observation.observation_epoch,
        "scheduling_signature":"schedule","policy_signature":"policy"});
    let mut plan = context.clone();
    plan["status"] = json!("confirmed");
    plan["desired"] = json!([{"id":"service/0"},{"id":"service/1"}]);
    for key in [
        "desired_plan_version",
        "applied_plan_version",
        "confirmed_plan_version",
    ] {
        plan[key] = json!("1");
    }
    let mut policy = context.clone();
    policy["current"] = json!(true);
    policy["authorization"] = json!("allow");
    let mut resilience = context.clone();
    resilience["status"] = json!("current");
    let snapshots = std::collections::BTreeMap::from([
        ("ui-placements", vec![plan]),
        ("ui-policy", vec![policy]),
        ("ui-resilience", vec![resilience]),
        ("ui-decisions", vec![]),
        (
            "ui-workloads",
            vec![json!({
                "replicas":2,"ready_replicas":2.0,"fencing_pending_replicas":0.0,"suspended_replicas":0.0,
            })],
        ),
    ]);
    assert!(observation.projections_current(2, &snapshots)?);
    for (query, field, value) in [
        ("ui-placements", "status", json!("awaiting-measurements")),
        ("ui-placements", "confirmed_plan_version", json!("2")),
        ("ui-workloads", "ready_replicas", json!(1)),
        ("ui-workloads", "fencing_pending_replicas", json!(1)),
        ("ui-workloads", "suspended_replicas", json!(1)),
        ("ui-policy", "authorization", json!("unknown")),
        ("ui-policy", "policy_signature", json!("old-policy")),
        ("ui-resilience", "status", json!("checking")),
        ("ui-resilience", "observation_epoch", json!(Uuid::new_v4())),
    ] {
        let mut pending = snapshots.clone();
        pending.get_mut(query).unwrap()[0][field] = value;
        assert!(
            !observation.projections_current(2, &pending)?,
            "{query}.{field} must block readiness"
        );
    }
    assert!(!observation.projections_current(3, &snapshots)?);
    let mut infeasible = snapshots.clone();
    infeasible.get_mut("ui-placements").unwrap()[0]["status"] = json!("blocked");
    infeasible.get_mut("ui-workloads").unwrap()[0]["ready_replicas"] = json!(1);
    let mut decision = context;
    decision["stage"] = json!("diagnostic");
    decision["outcome"] = json!("infeasible");
    infeasible.get_mut("ui-decisions").unwrap().push(decision);
    assert!(
        observation.projections_current(2, &infeasible)?,
        "current infeasibility is valid demo state, not confirmation"
    );
    infeasible.get_mut("ui-decisions").unwrap()[0]["scheduling_signature"] = json!("old");
    assert!(!observation.projections_current(2, &infeasible)?);
    let mut empty = snapshots.clone();
    empty.get_mut("ui-placements").unwrap()[0]["desired"] = json!([]);
    empty.get_mut("ui-workloads").unwrap().clear();
    empty.get_mut("ui-policy").unwrap().clear();
    assert!(
        observation.projections_current(0, &empty)?,
        "explicit empty workloads remain supported"
    );
    Ok(())
}

async fn deliver(
    processor: &mut Processor,
    input_query: &mut ContinuousQueryTransformer,
    source: &mut Emitter,
    key: &str,
    message: &Message,
) -> Result<Vec<OutputEnvelope>> {
    let mut output = Vec::new();
    for row in send(input_query, source, key, message).await? {
        output.extend(
            processor
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: row.envelope,
                })
                .await?,
        );
    }
    Ok(output)
}

#[tokio::test]
async fn simulator_activity_names_actual_moves_without_inventing_moves_on_bootstrap_or_rejection(
) -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let workers = Arc::new(Workers::default());
    let mut simulator = processor(Kind::Simulator, workers.clone())?;
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: workers.hub.clone(),
    };
    let mut runtime = crate::status::Producer::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("move-status")?)?,
        json!({"stream":"move-status"}),
        workers.hub.clone(),
    )?;
    runtime.start().await?;
    simulator.start().await?;
    let mut input = query("move-input", "MATCH (i:Input) RETURN i.payload AS payload").await?;
    let mut timeline = query("move-timeline", views::TIMELINE).await?;
    let mut source = Emitter::new(StreamId::try_new("move-source")?);
    let boot = bootstrap(&fixture, epoch)?;
    let Message::Bootstrap { mut plan, .. } = boot.clone() else {
        unreachable!()
    };
    deliver(&mut simulator, &mut input, &mut source, "bootstrap", &boot).await?;
    deliver(
        &mut simulator,
        &mut input,
        &mut source,
        "policy",
        &Message::Policy {
            epoch,
            config_fingerprint: fixture.configuration.fingerprint()?,
            assessment: Evaluator::new()?.evaluate(&fixture.configuration)?,
        },
    )
    .await?;
    let output = runtime.next().await?.context("initial status missing")?;
    let rows = project(&mut timeline, &[output]).await?;
    assert!(!rows.iter().any(|row| row["kind"] == "replica-moved"));
    let workload = fixture
        .configuration
        .workloads
        .values()
        .find(|w| w.name == "chat")
        .unwrap();
    let gpu = |host: &str, slot| {
        fixture
            .configuration
            .gpus
            .values()
            .find(|g| g.host_id == host && g.gpu_index == slot)
            .unwrap()
            .gpu_id
    };
    let assignment = plan
        .assignments
        .iter_mut()
        .find(|a| a.workload_id == workload.workload_id && a.replica_index == 0)
        .unwrap();
    assignment.gpu_id = gpu("inference-b", 1);
    plan.plan_version += 1;
    plan.decision_id = Uuid::new_v4();
    validate_assignments(
        &fixture.configuration,
        &fixtures::baseline_capacities(&fixture.configuration),
        &plan.assignments,
    )?;
    let changed = deliver(
        &mut simulator,
        &mut input,
        &mut source,
        "allocation",
        &Message::Allocation {
            epoch,
            plan: plan.clone(),
        },
    )
    .await?;
    assert_eq!(
        payloads(&changed, "AppliedPlan")?[0]["application"]["applied_plan_version"],
        "2"
    );
    let output = runtime.next().await?.context("move status missing")?;
    let rows = project(&mut timeline, &[output]).await?;
    let moves = rows
        .iter()
        .filter(|row| row["kind"] == "replica-moved")
        .collect::<Vec<_>>();
    assert_eq!(moves.len(), 1);
    assert_eq!(
        moves[0]["message"],
        "chat replica 1 moved from inference-a / GPU 1 to inference-b / GPU 1."
    );
    assert_eq!(moves[0]["decision_id"], plan.decision_id.to_string());
    assert_eq!(moves[0]["plan_version"], "2");
    deliver(
        &mut simulator,
        &mut input,
        &mut source,
        "same-allocation",
        &Message::Allocation {
            epoch,
            plan: plan.clone(),
        },
    )
    .await?;
    plan.plan_version += 1;
    plan.assignments
        .iter_mut()
        .find(|a| a.workload_id == workload.workload_id && a.replica_index == 0)
        .unwrap()
        .gpu_id = gpu("inference-c", 0);
    let rejected = deliver(
        &mut simulator,
        &mut input,
        &mut source,
        "rejected-allocation",
        &Message::Allocation { epoch, plan },
    )
    .await?;
    let applied = payloads(&rejected, "AppliedPlan")?;
    assert!(applied[0]["application"]["error"].is_string());
    assert_eq!(applied[0]["application"]["applied_plan_version"], "2");
    let output = runtime.next().await?.context("rejection status missing")?;
    let rows = project(&mut timeline, &[output]).await?;
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "replica-moved")
            .count(),
        1
    );
    simulator.stop().await?;
    input.stop().await?;
    timeline.stop().await?;
    runtime.stop().await?;
    Ok(())
}

fn row<'a>(rows: &'a [Value], key: &str, id: &str) -> Result<&'a Value> {
    let matches: Vec<_> = rows.iter().filter(|row| row[key] == id).collect();
    anyhow::ensure!(
        matches.len() == 1,
        "expected one {key}={id} row, got {}",
        matches.len()
    );
    Ok(matches[0])
}

fn database_configuration(
    source: &mut Emitter,
    fixture: &fixtures::Fixture,
) -> Result<Vec<OutputEnvelope>> {
    let mut changes = Vec::new();
    for cluster in fixture.configuration.clusters.values() {
        changes.extend(source.record("regional_clusters", &cluster.cluster_id, cluster)?);
    }
    for gpu in fixture.configuration.gpus.values() {
        changes.extend(source.record("gpu_inventory", &gpu.gpu_id.to_string(), gpu)?);
    }
    for settings in fixture.settings.values() {
        changes.extend(source.record("gpu_telemetry", &settings.gpu_id.to_string(), settings)?);
    }
    for workload in fixture.configuration.workloads.values() {
        changes.extend(source.record(
            "workload_requirements",
            &workload.workload_id.to_string(),
            workload,
        )?);
    }
    source.emit(changes, None)
}

async fn project_workload_updates(
    query: &mut ContinuousQueryTransformer,
    outputs: &[OutputEnvelope],
    live: &mut BTreeMap<String, Value>,
) -> Result<BTreeMap<String, u64>> {
    let key = |value: &Value| {
        value["workload_id"]
            .as_str()
            .map(str::to_owned)
            .context("missing workload result key")
    };
    for output in outputs {
        let projected = query
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: output.envelope.clone(),
            })
            .await?;
        for output in &projected {
            for change in QueryChangeCodec::to_legacy_result(&output.envelope)?.results {
                match change {
                    ResultDiff::Add { data, .. } => {
                        live.insert(key(&data)?, data);
                    }
                    ResultDiff::Update { before, after, .. } => {
                        if key(&before)? != key(&after)? {
                            live.remove(&key(&before)?);
                        }
                        live.insert(key(&after)?, after);
                    }
                    ResultDiff::Delete { data, .. } => {
                        live.remove(&key(&data)?);
                    }
                    ResultDiff::Aggregation { before, after, .. } => {
                        if let Some(before) = before {
                            if key(&before)? != key(&after)? {
                                live.remove(&key(&before)?);
                            }
                        }
                        live.insert(key(&after)?, after);
                    }
                    ResultDiff::Noop => {}
                }
            }
        }
        query.delivery_completed(&projected).await?;
    }
    let mut snapshot = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for record in query.results().snapshot()?.rows.values() {
        let row = QueryChangeCodec::decode_row(record)?;
        let value = QueryChangeCodec::row_values_to_json(&row.values);
        let id = key(&value)?;
        assert!(
            snapshot.insert(id.clone(), value).is_none(),
            "duplicate workload row"
        );
        identities.insert(id, row.signature);
    }
    assert_eq!(
        live, &snapshot,
        "SSE updates keyed by workload_id must agree with the query snapshot"
    );
    Ok(identities)
}

#[tokio::test]
async fn workload_identity_survives_readiness_and_execution_transitions() -> Result<()> {
    let mut query = configured_query(
        "ui-workloads",
        views::WORKLOADS,
        views::execution_settings(),
    )
    .await?;
    let mut source = Emitter::new(StreamId::try_new("workload-identity")?);
    let fixture = fixtures::load("baseline")?;
    let mut live = BTreeMap::new();
    project_workload_updates(
        &mut query,
        &database_configuration(&mut source, &fixture)?,
        &mut live,
    )
    .await?;
    assert_eq!(live.len(), 4);
    assert!(live.values().all(|row| row["running_replicas"].is_null()));

    let mut changes = Vec::new();
    changes.extend(source.record("AppliedPlan", "demo", &json!({"source_ready":true}))?);
    changes.extend(source.record("DemoReadiness", "demo", &json!({"inputs_ready":true}))?);
    for assignment in &fixture.assignments {
        let mut value = serde_json::to_value(assignment)?;
        value["state"] = json!("running");
        changes.extend(source.record(
            "PolicyEnforcement",
            &format!("{}/{}", assignment.workload_id, assignment.replica_index),
            &value,
        )?);
    }
    let identities =
        project_workload_updates(&mut query, &source.emit(changes, None)?, &mut live).await?;
    assert!(
        live.values()
            .all(|row| row["running_replicas"].as_f64() == Some(2.0)),
        "expected two running replicas per workload: {live:?}"
    );

    for ready in [false, true, false, true] {
        let changes = source
            .record("DemoReadiness", "demo", &json!({"inputs_ready":ready}))?
            .into_iter()
            .collect();
        let current =
            project_workload_updates(&mut query, &source.emit(changes, None)?, &mut live).await?;
        assert_eq!(
            current, identities,
            "readiness must not change workload row identities"
        );
        assert_eq!(live.len(), fixture.configuration.workloads.len());
        for row in live.values() {
            assert_eq!(row["running_replicas"].as_f64(), Some(2.0));
            if ready {
                assert_eq!(row["ready_replicas"].as_f64(), Some(0.0));
            } else {
                assert!(row["ready_replicas"].is_null());
            }
        }
    }

    for state in ["suspended", "fenced", "running"] {
        let mut changes = Vec::new();
        for assignment in &fixture.assignments {
            let mut value = serde_json::to_value(assignment)?;
            value["state"] = json!(state);
            changes.extend(source.record(
                "PolicyEnforcement",
                &format!("{}/{}", assignment.workload_id, assignment.replica_index),
                &value,
            )?);
        }
        let current =
            project_workload_updates(&mut query, &source.emit(changes, None)?, &mut live).await?;
        assert_eq!(current, identities);
        for row in live.values() {
            assert_eq!(
                row["running_replicas"].as_f64(),
                Some(if state == "running" { 2.0 } else { 0.0 })
            );
            assert_eq!(
                row["suspended_replicas"].as_f64(),
                Some(if state == "suspended" { 2.0 } else { 0.0 })
            );
            assert_eq!(
                row["fenced_replicas"].as_f64(),
                Some(if state == "fenced" { 2.0 } else { 0.0 })
            );
        }
    }
    let removed = source.remove("AppliedPlan", "demo")?.unwrap();
    let current =
        project_workload_updates(&mut query, &source.emit(vec![removed], None)?, &mut live).await?;
    assert_eq!(current, identities);
    assert!(live.values().all(|row| row["running_replicas"].is_null()));
    let restored = source
        .record("AppliedPlan", "demo", &json!({"source_ready":true}))?
        .unwrap();
    let current =
        project_workload_updates(&mut query, &source.emit(vec![restored], None)?, &mut live)
            .await?;
    assert_eq!(current, identities);

    let mut renamed = fixture
        .configuration
        .workloads
        .values()
        .next()
        .unwrap()
        .clone();
    renamed.name = "renamed-service".into();
    renamed.revision += 1;
    let changes = source
        .record(
            "workload_requirements",
            &renamed.workload_id.to_string(),
            &renamed,
        )?
        .into_iter()
        .collect();
    let current =
        project_workload_updates(&mut query, &source.emit(changes, None)?, &mut live).await?;
    assert_eq!(current, identities);
    assert_eq!(
        live[&renamed.workload_id.to_string()]["name"],
        "renamed-service"
    );
    let removed = source
        .remove("workload_requirements", &renamed.workload_id.to_string())?
        .unwrap();
    let current =
        project_workload_updates(&mut query, &source.emit(vec![removed], None)?, &mut live).await?;
    assert_eq!(live.len(), 3);
    assert!(!live.contains_key(&renamed.workload_id.to_string()));
    assert_eq!(current.len(), 3);
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn rejected_startup_plan_preserves_observed_input_readiness() -> Result<()> {
    for denied in [false, true] {
        let mut fixture = fixtures::load("baseline")?;
        if denied {
            let policy = fixture
                .configuration
                .policies
                .get_mut("demo-permissive")
                .unwrap();
            policy.allowed_regions.clear();
            policy.revision += 1;
        } else {
            for settings in fixture.settings.values_mut() {
                settings.powered_on = false;
                settings.revision += 1;
            }
        }
        let epoch = Uuid::new_v4();
        let workers = Arc::new(Workers::default());
        let mut simulator = processor(Kind::Simulator, workers.clone())?;
        let factory = AuxiliaryFactory {
            reaction: false,
            hub: workers.hub.clone(),
        };
        let mut runtime = crate::status::Producer::new(
            factory
                .metadata()
                .descriptor(ComponentId::try_new("runtime-status")?)?,
            json!({"stream":"rejected-plan-status"}),
            workers.hub.clone(),
        )?;
        runtime.start().await?;
        simulator.start().await?;
        let mut input_query = query(
            "startup-inputs",
            "MATCH (i:Input) RETURN i.payload AS payload",
        )
        .await?;
        let mut status_query = query("startup-status", views::STATUS).await?;
        let mut source = Emitter::new(StreamId::try_new("rejected-plan-source")?);
        deliver(
            &mut simulator,
            &mut input_query,
            &mut source,
            "bootstrap",
            &bootstrap(&fixture, epoch)?,
        )
        .await?;
        let assessment = Evaluator::new()?.evaluate(&fixture.configuration)?;
        deliver(
            &mut simulator,
            &mut input_query,
            &mut source,
            "policy",
            &Message::Policy {
                epoch,
                config_fingerprint: fixture.configuration.fingerprint()?,
                assessment: assessment.clone(),
            },
        )
        .await?;
        let output = simulator.on_wakeup().await?;
        let applied = payloads(&output, "AppliedPlan")?;
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0]["source_ready"], true);
        assert!(applied[0]["application"]["applied_plan_version"].is_null());
        assert!(applied[0]["application"]["error"].is_string());
        assert_eq!(applied[0]["execution"], json!([]));
        let mut observation = RuntimeObservation {
            sequence: 1,
            observation_epoch: epoch,
            scenario: "baseline".into(),
            scheduling_signature: "observed-scheduling-input".into(),
            policy_signature: assessment.policy_signature,
            source_bootstrap_complete: true,
            query_bootstrap_complete: true,
            query_results_current: false,
            reset_in_progress: false,
            detail: "Bootstrapped inputs with a rejected saved plan".into(),
            required_components: [simulator.descriptor().id().to_string()]
                .into_iter()
                .collect(),
            components: vec![RuntimeComponent {
                component_id: simulator.descriptor().id().to_string(),
                status: "running".into(),
                error: None,
            }],
        };
        workers.observe_runtime(observation.clone())?;
        let update = runtime.next().await?.context("runtime status missing")?;
        let rows = project(&mut status_query, &[update]).await?;
        assert_eq!(
            rows[0]["inputs_ready"], true,
            "valid inputs must allow corrective edits"
        );
        assert_eq!(
            rows[0]["scenario_ready"], false,
            "rejected application is not convergence"
        );
        assert!(rows[0]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| {
                component["component_id"] == "Simulator"
                    && component["status"] == "application-rejected"
                    && component["error"].is_string()
            }));
        observation.sequence += 1;
        observation.source_bootstrap_complete = false;
        workers.observe_runtime(observation)?;
        let update = runtime
            .next()
            .await?
            .context("source loss status missing")?;
        assert_eq!(
            project(&mut status_query, &[update]).await?[0]["inputs_ready"],
            false,
            "source failure must still close the input gate"
        );
        simulator.stop().await?;
        input_query.stop().await?;
        status_query.stop().await?;
        runtime.stop().await?;
    }
    Ok(())
}

#[tokio::test]
async fn simultaneous_heartbeat_expiry_preserves_one_capacity_per_gpu() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let configuration = &fixture.configuration;
    let fingerprint = configuration.fingerprint()?;
    let Message::Bootstrap { plan, .. } = bootstrap(&fixture, epoch)? else {
        unreachable!()
    };
    let mut settings = views::cluster_settings();
    settings.joins.push(drasi_lib::config::QueryJoinConfig {
        id: "POLICY_CONFIG".into(),
        keys: ["FleetConfiguration", "PolicyAssessment"]
            .into_iter()
            .map(|label| drasi_lib::config::QueryJoinKeyConfig {
                label: label.into(),
                property: "config_fingerprint".into(),
            })
            .collect(),
    });
    let mut query = configured_query(
        "simultaneous-heartbeat-expiry",
        crate::inputs::SCHEDULING_QUERY,
        settings,
    )
    .await?;
    let mut database = Emitter::new(StreamId::try_new("expiry-database")?);
    project(
        &mut query,
        &database_configuration(&mut database, &fixture)?,
    )
    .await?;
    let mut context = Emitter::new(StreamId::try_new("expiry-policy")?);
    let mut snapshot = json!({
        "epoch":epoch, "configuration":configuration, "plan":plan,
        "settings":fixture.settings, "config_fingerprint":fingerprint,
    });
    let changes = vec![
        context
            .record("FleetConfiguration", "demo", &snapshot)?
            .unwrap(),
        context
            .record(
                "PolicyAssessment",
                "demo",
                &json!({
                    "epoch":epoch, "config_fingerprint":fingerprint,
                    "assessment":Evaluator::new()?.evaluate(configuration)?,
                }),
            )?
            .unwrap(),
    ];
    project(&mut query, &context.emit(changes, None)?).await?;
    let mut reports = Emitter::new(StreamId::try_new("expiry-simulator")?);
    let now = wire::utc_millis()?;
    let mut changes = Vec::new();
    for gpu in configuration.gpus.values() {
        changes.extend(reports.record(
            "GpuSample",
            &gpu.gpu_id.to_string(),
            &json!({
                "gpu_id":gpu.gpu_id, "observation_epoch":epoch,
                "report_time_ms":if gpu.host_id == "inference-a" { now - 4700 } else { now },
                "background_memory_requested_mib":0, "background_compute_units":10,
            }),
        )?);
    }
    let rows = project(&mut query, &reports.emit(changes, None)?).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["capacities"].as_array().unwrap().len(), 6);
    for gpu in configuration
        .gpus
        .values()
        .filter(|g| g.host_id == "inference-a")
    {
        snapshot["settings"][gpu.gpu_id.to_string()]["powered_on"] = json!(false);
        let change = context
            .record("FleetConfiguration", "demo", &snapshot)?
            .unwrap();
        project(&mut query, &context.emit(vec![change], None)?).await?;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let now = wire::utc_millis()?;
    let mut changes = Vec::new();
    for gpu in configuration
        .gpus
        .values()
        .filter(|g| g.host_id != "inference-a")
    {
        changes.extend(reports.record(
            "GpuSample",
            &gpu.gpu_id.to_string(),
            &json!({
                "gpu_id":gpu.gpu_id, "observation_epoch":epoch, "report_time_ms":now,
                "background_memory_requested_mib":0, "background_compute_units":10,
            }),
        )?);
    }
    project(&mut query, &reports.emit(changes, None)?).await?;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            query
                .wakeup_source()
                .context("scheduling timer missing")?
                .wait()
                .await?;
            let changes = query.on_wakeup().await?;
            query.delivery_completed(&changes).await?;
            let rows = project(&mut query, &[]).await?;
            assert_eq!(rows.len(), 1);
            let capacities = rows[0]["capacities"].as_array().unwrap();
            assert_eq!(
                capacities.len(),
                6,
                "expired GPU must not be appended twice: {capacities:?}"
            );
            let identities: std::collections::BTreeSet<_> = capacities
                .iter()
                .map(|c| c["gpu_id"].as_str().unwrap())
                .collect();
            assert_eq!(identities.len(), 6);
            if capacities.iter().filter(|c| c["fresh"] == false).count() == 2 {
                break Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await??;
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn inventory_views_preserve_reports_until_the_real_deadline() -> Result<()> {
    let started_at = wire::utc_millis()?;
    let mut fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let workers = Arc::new(Workers::default());
    let mut simulator = processor(Kind::Simulator, workers)?;
    simulator.start().await?;
    let mut inputs = query(
        "inventory-input",
        "MATCH (i:Input) RETURN i.payload AS payload",
    )
    .await?;
    let mut source = Emitter::new(StreamId::try_new("inventory-source")?);
    let mut gpus =
        configured_query("inventory-ui-gpus", views::GPUS, views::gpu_settings()).await?;
    let mut clusters = configured_query(
        "inventory-ui-clusters",
        views::CLUSTERS,
        views::cluster_settings(),
    )
    .await?;
    let database = database_configuration(&mut source, &fixture)?;
    project(&mut gpus, &database).await?;
    project(&mut clusters, &database).await?;
    deliver(
        &mut simulator,
        &mut inputs,
        &mut source,
        "bootstrap",
        &bootstrap(&fixture, epoch)?,
    )
    .await?;
    let application = deliver(
        &mut simulator,
        &mut inputs,
        &mut source,
        "policy",
        &Message::Policy {
            epoch,
            config_fingerprint: fixture.configuration.fingerprint()?,
            assessment: Evaluator::new()?.evaluate(&fixture.configuration)?,
        },
    )
    .await?;
    let applied = payloads(&application, "AppliedPlan")?;
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["source_ready"], true);
    assert_eq!(
        applied[0]["config_fingerprint"],
        fixture.configuration.fingerprint()?
    );
    let execution = payloads(&application, "PolicyEnforcement")?;
    assert_eq!(execution.len(), 8);
    assert!(execution
        .iter()
        .all(|row| row["applied_plan_version"] == "1"
            && row["observation_epoch"] == epoch.to_string()
            && row["acknowledged_at_ms"]
                .as_u64()
                .is_some_and(|time| time >= started_at)));
    assert!(payloads(&application, "GpuSample")?.is_empty());
    let reports = simulator.on_wakeup().await?;
    let fresh = project(&mut gpus, &reports).await?;
    assert_eq!(fresh.len(), 6);
    assert!(fresh.iter().all(|row| row["health"] == "healthy"));
    for replica in &execution {
        let report = row(&fresh, "gpu_id", replica["gpu_id"].as_str().unwrap())?;
        assert!(report["report_time_ms"].as_u64() >= replica["acknowledged_at_ms"].as_u64());
    }
    let regions = project(&mut clusters, &reports).await?;
    assert_eq!(regions.len(), 1, "expected one cluster row: {regions:#?}");
    assert_eq!(
        regions[0]["healthy_gpus"].as_f64(),
        Some(6.0),
        "{regions:#?}"
    );

    let powered_off = fixture.assignments[0].gpu_id;
    for settings in fixture.settings.values_mut() {
        settings.reporting_enabled = false;
        settings.powered_on = settings.gpu_id != powered_off;
        settings.revision += 1;
    }
    let actions = deliver(
        &mut simulator,
        &mut inputs,
        &mut source,
        "pause-and-power-off",
        &Message::Configuration {
            epoch,
            commit: "complete-settings-update".into(),
            configuration: fixture.configuration.clone(),
            settings: fixture.settings.clone(),
        },
    )
    .await?;
    assert!(payloads(&actions, "GpuSample")?.is_empty());
    assert!(payloads(&simulator.on_wakeup().await?, "GpuSample")?.is_empty());
    let database = database_configuration(&mut source, &fixture)?;
    let paused = project(&mut gpus, &database).await?;
    assert_eq!(paused.len(), 6);
    assert!(paused
        .iter()
        .all(|row| row["health"] == "healthy" && row["reporting_enabled"] == false));
    assert_eq!(
        row(&paused, "gpu_id", &powered_off.to_string())?["powered_on"],
        false
    );
    for previous in &fresh {
        let current = row(&paused, "gpu_id", previous["gpu_id"].as_str().unwrap())?;
        for field in [
            "report_time_ms",
            "report_sequence",
            "modeled_memory_used_mib",
            "total_compute_units",
        ] {
            assert_eq!(
                current[field], previous[field],
                "settings must not refresh {field}"
            );
        }
    }

    let expired = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            gpus.wakeup_source()
                .context("GPU query timer missing")?
                .wait()
                .await?;
            let updates = gpus.on_wakeup().await?;
            gpus.delivery_completed(&updates).await?;
            let rows = project(&mut gpus, &[]).await?;
            if rows.iter().all(|row| row["health"] == "unreachable") {
                break Ok::<_, anyhow::Error>(rows);
            }
        }
    })
    .await??;
    assert_eq!(expired.len(), 6);
    assert!(expired
        .iter()
        .all(|row| row["sample_age_ms"].as_u64().is_some_and(|age| age >= 5000)));
    tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            clusters
                .wakeup_source()
                .context("cluster query timer missing")?
                .wait()
                .await?;
            let updates = clusters.on_wakeup().await?;
            clusters.delivery_completed(&updates).await?;
            let rows = project(&mut clusters, &[]).await?;
            assert_eq!(rows.len(), 1);
            if rows[0]["healthy_gpus"].as_f64() == Some(0.0) {
                break Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await??;

    let empty = Cluster {
        cluster_id: "empty-region".into(),
        name: "Empty region".into(),
        region: "northeurope".into(),
        revision: 1,
    };
    let change = source
        .record("regional_clusters", &empty.cluster_id, &empty)?
        .unwrap();
    let rows = project(&mut clusters, &source.emit(vec![change], None)?).await?;
    let empty = row(&rows, "cluster_id", "empty-region")?;
    assert_eq!(empty["registered_workers"], 0);
    assert_eq!(empty["healthy_gpus"].as_f64(), Some(0.0));
    let unavailable = deliver(
        &mut simulator,
        &mut inputs,
        &mut source,
        "unavailable",
        &Message::Unavailable {
            epoch,
            reason: "Observed source interruption".into(),
        },
    )
    .await?;
    assert_eq!(
        payloads(&unavailable, "AppliedPlan")?[0]["source_ready"],
        false
    );
    assert!(payloads(&unavailable, "GpuSample")?.is_empty());
    simulator.stop().await?;
    inputs.stop().await?;
    gpus.stop().await?;
    clusters.stop().await?;
    Ok(())
}

#[tokio::test]
async fn optional_native_join_replaces_the_unmatched_snapshot_row() -> Result<()> {
    let mut query = configured_query(
        "optional-join-probe",
        "MATCH (d:DecisionExplanation) OPTIONAL MATCH (d)-[:DECISION_WRITE]->(w:PlanWriteOutcome) RETURN d.decision_id AS decision_id, w.outcome AS outcome",
        views::decision_settings(),
    ).await?;
    let mut source = Emitter::new(StreamId::try_new("optional-join-probe-source")?);
    let left = source
        .record(
            "DecisionExplanation",
            "decision",
            &json!({"decision_id":"decision"}),
        )?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![left], None)?).await?;
    assert_eq!(rows, vec![json!({"decision_id":"decision","outcome":null})]);
    let right = source
        .record(
            "PlanWriteOutcome",
            "decision",
            &json!({"decision_id":"decision","outcome":"rejected"}),
        )?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![right], None)?).await?;
    assert_eq!(
        rows,
        vec![json!({"decision_id":"decision","outcome":"rejected"})],
        "the unmatched optional row must be retracted when its matching right row arrives"
    );
    let updated = source
        .record(
            "PlanWriteOutcome",
            "decision",
            &json!({"decision_id":"decision","outcome":"receipt"}),
        )?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![updated], None)?).await?;
    assert_eq!(
        rows,
        vec![json!({"decision_id":"decision","outcome":"receipt"})]
    );
    let removed = source.remove("PlanWriteOutcome", "decision")?.unwrap();
    let rows = project(&mut query, &source.emit(vec![removed], None)?).await?;
    assert_eq!(rows, vec![json!({"decision_id":"decision","outcome":null})]);
    let removed = source.remove("DecisionExplanation", "decision")?.unwrap();
    assert!(project(&mut query, &source.emit(vec![removed], None)?)
        .await?
        .is_empty());
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn optional_fleet_context_preserves_inventory_before_bootstrap() -> Result<()> {
    let mut query = query("optional-context",
        "MATCH (w:workload_requirements) OPTIONAL MATCH (a:AppliedPlan) RETURN w.workload_id AS workload_id, a.fleet_id AS fleet_id").await?;
    let mut source = Emitter::new(StreamId::try_new("optional-context")?);
    let change = source
        .record(
            "workload_requirements",
            "workload",
            &json!({"workload_id":"workload"}),
        )?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![change], None)?).await?;
    assert_eq!(
        rows,
        vec![json!({"workload_id":"workload","fleet_id":null})]
    );
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn optional_assignment_list_preserves_null_before_and_after_context() -> Result<()> {
    let mut query = query(
        "optional-assignment-list",
        "MATCH (w:workload_requirements) OPTIONAL MATCH (p:gpu_placements) RETURN w.workload_id AS workload_id, [d IN p.assignments WHERE d = 'replica'] AS matching",
    ).await?;
    let mut source = Emitter::new(StreamId::try_new("optional-assignment-list-source")?);
    let workload = source
        .record(
            "workload_requirements",
            "workload",
            &json!({"workload_id":"workload"}),
        )?
        .unwrap();
    assert_eq!(
        project(&mut query, &source.emit(vec![workload], None)?).await?,
        vec![json!({"workload_id":"workload","matching":null})]
    );
    let plan = source
        .record(
            "gpu_placements",
            "demo",
            &json!({"assignments":["replica","other"]}),
        )?
        .unwrap();
    assert_eq!(
        project(&mut query, &source.emit(vec![plan], None)?).await?,
        vec![json!({"workload_id":"workload","matching":["replica"]})]
    );
    let plan = source
        .record("gpu_placements", "demo", &json!({"assignments":[]}))?
        .unwrap();
    assert_eq!(
        project(&mut query, &source.emit(vec![plan], None)?).await?,
        vec![json!({"workload_id":"workload","matching":[]})]
    );
    let plan = source.remove("gpu_placements", "demo")?.unwrap();
    assert_eq!(
        project(&mut query, &source.emit(vec![plan], None)?).await?,
        vec![json!({"workload_id":"workload","matching":null})]
    );
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn assignment_list_matches_equal_property_maps() -> Result<()> {
    let mut query = query(
        "assignment-equality",
        "MATCH (w:workload_requirements) OPTIONAL MATCH (p:gpu_placements) RETURN w.workload_id AS workload_id, [d IN p.assignments WHERE d = w.assignment] AS matching",
    ).await?;
    let mut source = Emitter::new(StreamId::try_new("assignment-equality-source")?);
    let assignment = json!({"gpu_id":"gpu-1","workload_revision":"1","memory_mib":4096});
    let workload = source
        .record(
            "workload_requirements",
            "workload",
            &json!({"workload_id":"replica","assignment":assignment}),
        )?
        .unwrap();
    project(&mut query, &source.emit(vec![workload], None)?).await?;
    let plan = source
        .record(
            "gpu_placements",
            "demo",
            &json!({"assignments":[assignment]}),
        )?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![plan], None)?).await?;
    assert_eq!(rows[0]["matching"], json!([assignment]));
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn aggregate_count_equals_integer_list_size() -> Result<()> {
    let mut query = query("numeric-count-equality",
        "MATCH (w:workload_requirements) WITH sum(w.replicas) AS count RETURN count AS count, size([1,2]) AS expected, count = size([1,2]) AS equal, count <> size([1,2]) AS unequal"
    ).await?;
    let mut source = Emitter::new(StreamId::try_new("numeric-count-source")?);
    let workload = source
        .record("workload_requirements", "workload", &json!({"replicas":2}))?
        .unwrap();
    let rows = project(&mut query, &source.emit(vec![workload], None)?).await?;
    assert_eq!(rows[0]["equal"], true, "{}", rows[0]);
    assert_eq!(rows[0]["unequal"], false);
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn native_evidence_projects_into_real_ui_queries() -> Result<()> {
    let before = wire::utc_millis()?;
    let workers = Arc::new(Workers::default());
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: workers.hub.clone(),
    };
    let mut runtime = crate::status::Producer::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("runtime-status")?)?,
        json!({"stream":"projection-runtime"}),
        workers.hub.clone(),
    )?;
    runtime.start().await?;
    let mut policy = processor(Kind::Policy, workers.clone())?;
    let mut simulator = processor(Kind::Simulator, workers.clone())?;
    let mut placement = processor(Kind::Placement, workers.clone())?;
    let mut resilience = processor(Kind::Resilience, workers.clone())?;
    policy.start().await?;
    simulator.start().await?;
    placement.start().await?;
    resilience.start().await?;
    let mut input_query = query(
        "projection-input",
        "MATCH (i:Input) RETURN i.payload AS payload",
    )
    .await?;
    let mut source = Emitter::new(StreamId::try_new("projection-source")?);
    let mut policy_query = query("ui-policy", views::POLICY).await?;
    let mut decision_query =
        configured_query("ui-decisions", views::DECISIONS, views::decision_settings()).await?;
    let mut timeline_query = query("ui-timeline", views::TIMELINE).await?;
    let mut resilience_query = configured_query(
        "ui-resilience",
        views::RESILIENCE,
        views::resilience_settings(),
    )
    .await?;
    let mut gpu_query = configured_query("ui-gpus", views::GPUS, views::gpu_settings()).await?;
    let mut cluster_query =
        configured_query("ui-clusters", views::CLUSTERS, views::cluster_settings()).await?;
    let mut workload_query = configured_query(
        "ui-workloads",
        views::WORKLOADS,
        views::execution_settings(),
    )
    .await?;
    let mut placement_query = configured_query(
        "ui-placements",
        views::PLACEMENTS,
        views::execution_settings(),
    )
    .await?;
    let mut status_query = query("ui-status", views::STATUS).await?;
    let mut batches = Vec::new();
    let epoch = Uuid::new_v4();
    let fixture = fixtures::load("baseline")?;
    let database = database_configuration(&mut source, &fixture)?;
    let rows = project(&mut workload_query, &database).await?;
    assert_eq!(rows.len(), 4);
    assert!(rows
        .iter()
        .all(|row| row["running_replicas"].is_null() && row["ready_replicas"].is_null()));
    batches.push(json!({"query":"ui-workloads","rows":rows}));
    project(&mut placement_query, &database).await?;
    let initial_runtime = runtime
        .next()
        .await?
        .context("initial runtime output missing")?;
    let rows = project(&mut status_query, std::slice::from_ref(&initial_runtime)).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["scenario_ready"], false);
    assert_eq!(
        rows[0]["observation_epoch"], "",
        "unknown epoch must not be invented"
    );
    batches.push(json!({"query":"ui-status","rows":rows}));
    project(&mut workload_query, std::slice::from_ref(&initial_runtime)).await?;
    project(&mut placement_query, std::slice::from_ref(&initial_runtime)).await?;
    let rows = project(&mut gpu_query, &database).await?;
    assert_eq!(rows.len(), fixture.configuration.gpus.len());
    assert!(rows
        .iter()
        .all(|row| row["health"] == "unknown" && row["report_time_ms"].is_null()));
    batches.push(json!({"query":"ui-gpus","rows":rows}));
    let rows = project(&mut cluster_query, &database).await?;
    assert_eq!(rows.len(), fixture.configuration.clusters.len());
    assert_eq!(rows[0]["registered_workers"], 3);
    assert!(
        rows[0]["healthy_gpus"].is_null(),
        "absent reports are not a measured zero"
    );
    batches.push(json!({"query":"ui-clusters","rows":rows}));
    let bootstrap = bootstrap(&fixture, epoch)?;
    let Message::Bootstrap {
        plan: baseline_plan,
        ..
    } = &bootstrap
    else {
        unreachable!()
    };
    let saved = source
        .record("gpu_placements", "demo", baseline_plan)?
        .context("saved plan missing")?;
    let saved = source.emit(vec![saved], None)?;
    project(&mut workload_query, &saved).await?;
    let rows = project(&mut placement_query, &saved).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["desired_plan_version"], "1");
    assert!(rows[0]["applied_plan_version"].is_null());
    batches.push(json!({"query":"ui-placements","rows":rows}));
    let pending = deliver(
        &mut policy,
        &mut input_query,
        &mut source,
        "policy-bootstrap",
        &bootstrap,
    )
    .await?;
    let rows = project(&mut policy_query, &pending).await?;
    assert_eq!(rows.len(), 4);
    assert!(rows
        .iter()
        .all(|row| row["authorization"] == "unknown" && row["current"] == false));
    batches.push(json!({"query":"ui-policy","rows":rows}));
    deliver(
        &mut simulator,
        &mut input_query,
        &mut source,
        "simulator-bootstrap",
        &bootstrap,
    )
    .await?;
    let evaluated = poll_until(&mut policy, "PolicyAssessment").await?;
    let configuration = source.record(
        "FleetConfiguration",
        "demo",
        &json!({
            "epoch":epoch, "configuration":fixture.configuration,
            "config_fingerprint":fixture.configuration.fingerprint()?,
        }),
    )?;
    project(
        &mut status_query,
        &source.emit(configuration.into_iter().collect(), None)?,
    )
    .await?;
    project(&mut status_query, &pending).await?;
    project(&mut status_query, &evaluated).await?;
    project(&mut workload_query, &evaluated).await?;
    project(&mut placement_query, &evaluated).await?;
    let rows = project(&mut policy_query, &evaluated).await?;
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row["authorization"] == "allow"
        && row["policy_revision"] == "1"
        && row["allowed_regions"] == json!(["*"])
        && row["observation_epoch"] == epoch.to_string()));
    batches.push(json!({"query":"ui-policy","rows":rows}));
    let message: Message =
        serde_json::from_value(payloads(&evaluated, "PolicyAssessment")?.remove(0))?;
    let applied = deliver(
        &mut simulator,
        &mut input_query,
        &mut source,
        "apply-policy",
        &message,
    )
    .await?;
    let actual = payloads(&applied, "PolicyEnforcement")?;
    assert_eq!(actual.len(), 8);
    assert!(actual
        .iter()
        .all(|row| row["observation_epoch"] == epoch.to_string()
            && row["acknowledged_at_ms"]
                .as_u64()
                .is_some_and(|time| time >= before)
            && row["workload_id"].is_string()
            && row["replica_index"].is_number()));
    assert!(payloads(&applied, "GpuSample")?.is_empty());
    let rows = project(&mut workload_query, &applied).await?;
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(
        |row| row["running_replicas"].as_f64() == Some(2.0) && row["ready_replicas"].is_null()
    ));
    batches.push(json!({"query":"ui-workloads","rows":rows}));
    project(&mut placement_query, &applied).await?;
    let baseline_input = SolveInput {
        configuration: fixture.configuration.clone(),
        policy: Evaluator::new()?.evaluate(&fixture.configuration)?,
        capacities: fixtures::baseline_capacities(&fixture.configuration),
        plan: baseline_plan.clone(),
    };
    let baseline_context = deliver(
        &mut placement,
        &mut input_query,
        &mut source,
        "baseline-context",
        &Message::Schedule {
            epoch,
            input: baseline_input.clone(),
        },
    )
    .await?;
    project(&mut workload_query, &baseline_context).await?;
    project(&mut placement_query, &baseline_context).await?;
    let component_ids: Vec<_> = gpu_contracts::UI_QUERIES
        .into_iter()
        .map(str::to_owned)
        .chain(
            [
                policy.descriptor(),
                simulator.descriptor(),
                placement.descriptor(),
                resilience.descriptor(),
                runtime.descriptor(),
            ]
            .into_iter()
            .map(|descriptor| descriptor.id().to_string()),
        )
        .collect();
    let mut observation = RuntimeObservation {
        sequence: 1, observation_epoch: epoch, scenario: "baseline".into(),
        scheduling_signature: baseline_input.signature()?, policy_signature: baseline_input.policy.policy_signature.clone(),
        source_bootstrap_complete: true, query_bootstrap_complete: true, query_results_current: false, reset_in_progress: false,
        detail: "Explicit fixture bootstrap delivered to started test queries; not PostgreSQL acceptance.".into(),
        required_components: component_ids.iter().cloned().collect(),
        components: component_ids.into_iter().map(|id| RuntimeComponent {
            component_id:id, status:"running".into(), error:None
        }).collect(),
    };
    workers.observe_runtime(observation.clone())?;
    let observed = runtime
        .next()
        .await?
        .context("runtime observation missing")?;
    let rows = project(&mut status_query, std::slice::from_ref(&observed)).await?;
    assert_eq!(rows[0]["scenario_ready"], false);
    batches.push(json!({"query":"ui-status","rows":rows}));
    let rows = project(&mut workload_query, std::slice::from_ref(&observed)).await?;
    assert!(
        rows.iter()
            .all(|row| row["ready_replicas"].as_f64() == Some(0.0)),
        "application is not a fresh report"
    );
    let waiting = project(&mut placement_query, std::slice::from_ref(&observed)).await?;
    assert_eq!(waiting[0]["status"], "awaiting-measurements");
    assert_eq!(
        waiting[0]["reason"],
        "Waiting for fleet confirmation: 0 confirmed, 8 active, 8 saved, 8 required replicas."
    );
    let report_started = wire::utc_millis()?;
    let reports = simulator.on_wakeup().await?;
    let report_finished = wire::utc_millis()?;
    let samples = payloads(&reports, "GpuSample")?;
    assert!(
        samples.iter().all(|sample| sample["report_time_ms"]
            .as_u64()
            .is_some_and(|time| (report_started..=report_finished).contains(&time))),
        "reports must carry their actual UTC generation time"
    );
    let rows = project(&mut gpu_query, &reports).await?;
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|row| row["health"] == "healthy"
        && row["sample_plan_version"] == "1"
        && row["observation_epoch"] == epoch.to_string()));
    batches.push(json!({"query":"ui-gpus","rows":rows}));
    let rows = project(&mut cluster_query, &reports).await?;
    assert_eq!(rows[0]["healthy_gpus"].as_f64(), Some(6.0));
    batches.push(json!({"query":"ui-clusters","rows":rows}));
    let rows = project(&mut workload_query, &reports).await?;
    if !rows
        .iter()
        .all(|row| row["ready_replicas"].as_f64() == Some(2.0))
    {
        let prefix = views::WORKLOADS
            .split("WITH w, a, r, e, epoch_current")
            .next()
            .context("confirmation query prefix missing")?;
        let mut diagnostic = configured_query("confirmation-diagnostic", &format!(
            "{prefix} RETURN w.workload_id AS workload_id, e.id AS execution_id, epoch_current AS epoch_current, inputs_current AS inputs_current, plan_applied AS plan_applied, workload_matches AS workload_matches, policy_current AS policy_current, report_current AS report_current, assignment_matches AS assignment_matches"
        ), views::execution_settings()).await?;
        for output in [
            &database,
            &vec![initial_runtime],
            &saved,
            &pending,
            &evaluated,
            &applied,
            &baseline_context,
            &vec![observed],
            &reports,
        ] {
            project(&mut diagnostic, output).await?;
        }
        let diagnostics = project(&mut diagnostic, &[]).await?;
        diagnostic.stop().await?;
        anyhow::bail!(
            "Fresh confirmation counts are incorrect: {}; predicate diagnostics: {}",
            serde_json::to_string(&rows)?,
            serde_json::to_string(&diagnostics)?
        );
    }
    assert!(rows
        .iter()
        .all(|row| row["ready_replicas"].as_f64() == Some(2.0)));
    batches.push(json!({"query":"ui-workloads","rows":rows}));
    let rows = project(&mut placement_query, &reports).await?;
    assert_eq!(rows.len(), 1);
    if rows[0]["status"] != "confirmed" {
        let prefix = views::PLACEMENTS
            .split("WITH p, a, c, confirmed_count, active_count,\n")
            .next()
            .context("placement count prefix missing")?;
        let mut diagnostic = configured_query("placement-count-diagnostic", &format!(
            "{prefix} RETURN confirmed_count AS confirmed_count, active_count AS active_count, size(p.assignments) AS expected_count, c.required_replicas AS required_count, confirmed_count = size(p.assignments) AS confirmed_equal, active_count = size(p.assignments) AS active_equal, c.required_replicas = size(p.assignments) AS required_equal"
        ), views::execution_settings()).await?;
        for output in [
            database.as_slice(),
            std::slice::from_ref(&initial_runtime),
            &saved,
            &pending,
            &evaluated,
            &applied,
            &baseline_context,
            std::slice::from_ref(&observed),
            &reports,
        ] {
            project(&mut diagnostic, output).await?;
        }
        let diagnostics = project(&mut diagnostic, &[]).await?;
        diagnostic.stop().await?;
        anyhow::bail!(
            "Placement confirmation is missing; count diagnostics: {}",
            serde_json::to_string(&diagnostics)?
        );
    }
    assert_eq!(rows[0]["status"], "confirmed");
    assert_eq!(rows[0]["confirmed_plan_version"], "1");
    batches.push(json!({"query":"ui-placements","rows":rows}));
    assert_eq!(samples.len(), 6);
    for execution in &actual {
        let sample = samples
            .iter()
            .find(|sample| sample["gpu_id"] == execution["gpu_id"])
            .context("sample missing")?;
        assert_eq!(sample["observation_epoch"], execution["observation_epoch"]);
        assert!(sample["report_time_ms"].as_u64() >= execution["acknowledged_at_ms"].as_u64());
    }

    poll_until(&mut placement, "RuntimeStatus").await?;
    deliver(
        &mut resilience,
        &mut input_query,
        &mut source,
        "baseline-resilience",
        &Message::Schedule {
            epoch,
            input: baseline_input.clone(),
        },
    )
    .await?;
    let assessed = poll_until(&mut resilience, "ResilienceAssessment").await?;
    let rows = project(&mut resilience_query, &assessed).await?;
    batches.push(json!({"query":"ui-resilience","rows":rows}));
    let snapshots = std::collections::BTreeMap::from([
        ("ui-placements", project(&mut placement_query, &[]).await?),
        ("ui-policy", project(&mut policy_query, &[]).await?),
        ("ui-workloads", project(&mut workload_query, &[]).await?),
        ("ui-decisions", project(&mut decision_query, &[]).await?),
        ("ui-resilience", project(&mut resilience_query, &[]).await?),
    ]);
    observation.sequence += 1;
    observation.query_results_current = observation.projections_current(8, &snapshots)?;
    assert!(
        observation.query_results_current,
        "actual current query evidence must satisfy readiness"
    );
    workers.observe_runtime(observation.clone())?;
    let ready = runtime
        .next()
        .await?
        .context("current query readiness output missing")?;
    let rows = project(&mut status_query, &[ready]).await?;
    assert_eq!(rows[0]["scenario_ready"], true);
    batches.push(json!({"query":"ui-status","rows":rows}));

    let unchanged = deliver(
        &mut simulator,
        &mut input_query,
        &mut source,
        "unchanged-config",
        &Message::Configuration {
            epoch,
            commit: "unchanged-complete-config".into(),
            configuration: fixture.configuration.clone(),
            settings: fixture.settings.clone(),
        },
    )
    .await?;
    assert!(
        payloads(&unchanged, "PolicyEnforcement")?.is_empty(),
        "unchanged input cannot refresh acknowledgements"
    );
    assert!(
        payloads(&unchanged, "GpuSample")?.is_empty(),
        "settings are not GPU reports"
    );

    let mut fragmented = fixtures::load("fragmentation")?;
    let Message::Bootstrap { plan, .. } = super::bootstrap(&fragmented, epoch)? else {
        unreachable!()
    };
    let workload = Workload::from_profile("new-assistant", "assistant-v1", 1, "demo-open", "demo")?;
    fragmented
        .configuration
        .workloads
        .insert(workload.workload_id, workload);
    let mut input = SolveInput {
        policy: Evaluator::new()?.evaluate(&fragmented.configuration)?,
        capacities: fixtures::baseline_capacities(&fragmented.configuration),
        configuration: fragmented.configuration.clone(),
        plan,
    };
    let scheduled = deliver(
        &mut placement,
        &mut input_query,
        &mut source,
        "schedule",
        &Message::Schedule {
            epoch,
            input: input.clone(),
        },
    )
    .await?;
    project(&mut decision_query, &scheduled).await?;
    let proposed = poll_until(&mut placement, "CandidatePlan").await?;
    let candidate: Candidate =
        serde_json::from_value(payloads(&proposed, "CandidatePlan")?.remove(0))?;
    let rows = project(&mut decision_query, &proposed).await?;
    let decision = row(&rows, "decision_id", &candidate.decision_id.to_string())?;
    assert_eq!(decision["stage"], "candidate");
    assert_eq!(decision["moved_replicas"], 1);
    assert_eq!(decision["new_replicas"], 1);
    assert_eq!(
        decision["moves"]
            .as_array()
            .context("moves not projected as array")?
            .len(),
        2
    );
    assert_eq!(decision["pre_plan_free_memory_mib"], 336 * 1024);
    assert_eq!(decision["pre_plan_largest_gap_mib"], 56 * 1024);
    batches.push(json!({"query":"ui-decisions","rows":rows}));

    workers
        .hub
        .write_outcome(&candidate, "receipt", "HTTP receipt received")?;
    let update = runtime.next().await?.context("runtime output missing")?;
    let rows = project(&mut decision_query, std::slice::from_ref(&update)).await?;
    assert_eq!(
        row(&rows, "decision_id", &candidate.decision_id.to_string())?["stage"],
        "candidate",
        "a receipt alone must not promote a decision to committed"
    );
    let events = project(&mut timeline_query, &[update]).await?;
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|event| event["observation_epoch"] == epoch.to_string()
            && event["message"].is_string()
            && event["kind"].is_string()));
    batches.push(json!({"query":"ui-timeline","rows":events}));

    let mut stale = candidate.clone();
    stale.decision_details["observation_epoch"] = json!(Uuid::new_v4());
    workers
        .hub
        .write_outcome(&stale, "rejected", "Previous run rejected")?;
    let update = runtime.next().await?.context("runtime output missing")?;
    let rows = project(&mut decision_query, &[update]).await?;
    assert_eq!(
        row(&rows, "decision_id", &candidate.decision_id.to_string())?["stage"],
        "candidate"
    );
    workers
        .hub
        .write_outcome(&candidate, "rejected", "Candidate rejected with 409")?;
    let update = runtime.next().await?.context("runtime output missing")?;
    let rows = project(&mut decision_query, &[update]).await?;
    assert_eq!(
        row(&rows, "decision_id", &candidate.decision_id.to_string())?["stage"],
        "rejected"
    );
    batches.push(json!({"query":"ui-decisions","rows":rows}));

    input.plan = Plan {
        fleet_id: "demo".into(),
        plan_version: 2,
        decision_id: candidate.decision_id,
        config_fingerprint: candidate.config_fingerprint.clone(),
        policy_signature: candidate.policy_signature.clone(),
        policy_bundle_hash: candidate.policy_bundle_hash.clone(),
        assignments: candidate.assignments.clone(),
        decision_details: candidate.decision_details.clone(),
    };
    let committed = deliver(
        &mut placement,
        &mut input_query,
        &mut source,
        "committed-schedule",
        &Message::Schedule {
            epoch,
            input: input.clone(),
        },
    )
    .await?;
    let rows = project(&mut decision_query, &committed).await?;
    let saved = row(&rows, "decision_id", &candidate.decision_id.to_string())?;
    assert_eq!(saved["stage"], "committed");
    assert_eq!(saved["plan_version"], "2");
    assert_eq!(saved["moves"], decision["moves"]);
    batches.push(json!({"query":"ui-decisions","rows":rows}));

    input
        .configuration
        .policies
        .get_mut("demo-permissive")
        .context("fixture policy missing")?
        .allowed_regions
        .clear();
    input.policy = Evaluator::new()?.evaluate(&input.configuration)?;
    let blocked = Message::Schedule {
        epoch,
        input: input.clone(),
    };
    let output = deliver(
        &mut placement,
        &mut input_query,
        &mut source,
        "blocked-schedule",
        &blocked,
    )
    .await?;
    project(&mut decision_query, &output).await?;
    let invalidated = deliver(
        &mut resilience,
        &mut input_query,
        &mut source,
        "assess-blocked-schedule",
        &blocked,
    )
    .await?;
    assert!(project(&mut resilience_query, &invalidated)
        .await?
        .is_empty());
    let diagnostic = poll_until(&mut placement, "CapacityDiagnostic").await?;
    assert!(payloads(&diagnostic, "CandidatePlan")?.is_empty());
    let rows = project(&mut decision_query, &diagnostic).await?;
    assert!(rows.iter().any(|row| row["outcome"] == "infeasible"
        && row["stage"] == "diagnostic"
        && row["plan_version"].is_null()
        && row["moved_replicas"].is_null()));
    batches.push(json!({"query":"ui-decisions","rows":rows}));
    project(&mut resilience_query, &diagnostic).await?;
    let assessment = poll_until(&mut resilience, "ResilienceAssessment").await?;
    let rows = project(&mut resilience_query, &assessment).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["capacity_only_feasible"], true);
    assert_eq!(rows[0]["observation_epoch"], epoch.to_string());
    assert_eq!(
        rows[0]["workers"]
            .as_array()
            .context("worker scenarios not an array")?
            .len(),
        3
    );
    assert!(rows[0]["workers"]
        .as_array()
        .unwrap()
        .iter()
        .all(|scenario| scenario["feasible"] == false));
    batches.push(json!({"query":"ui-resilience","rows":rows}));

    let mut invalid = fixture.configuration.clone();
    invalid.policies.clear();
    let pending = deliver(
        &mut policy,
        &mut input_query,
        &mut source,
        "invalid-context",
        &Message::Configuration {
            epoch,
            commit: "invalid-policy-context-commit".into(),
            configuration: invalid,
            settings: fixture.settings,
        },
    )
    .await?;
    project(&mut policy_query, &pending).await?;
    let evaluated = poll_until(&mut policy, "PolicyAssessment").await?;
    let rows = project(&mut policy_query, &evaluated).await?;
    assert_eq!(
        rows.len(),
        4,
        "invalid policy must not silently remove pairs"
    );
    assert!(rows.iter().all(|row| row["authorization"] == "unknown"
        && row["policy_revision"].is_null()
        && row["error"].is_string()));
    batches.push(json!({"query":"ui-policy","rows":rows}));

    if let Some(path) = std::env::var_os("GPU_LAB_NATIVE_ROWS") {
        std::fs::write(path, serde_json::to_vec_pretty(&batches)?)?;
    }
    policy.stop().await?;
    simulator.stop().await?;
    placement.stop().await?;
    resilience.stop().await?;
    runtime.stop().await?;
    input_query.stop().await?;
    policy_query.stop().await?;
    decision_query.stop().await?;
    timeline_query.stop().await?;
    resilience_query.stop().await?;
    gpu_query.stop().await?;
    cluster_query.stop().await?;
    workload_query.stop().await?;
    placement_query.stop().await?;
    status_query.stop().await?;
    Ok(())
}

#[test]
fn graph_projection_rejects_lossy_integers_without_caching_failed_records() -> Result<()> {
    let mut emitter = Emitter::new(StreamId::try_new("typed-record-test")?);
    let invalid = json!({"nested":[{"counter":u64::MAX}]});
    assert!(emitter.record("Typed", "id", &invalid).is_err());
    assert!(emitter.record("Typed", "id", &invalid).is_err());
    assert!(matches!(
        emitter.record(
            "Typed",
            "id",
            &json!({"nested":[{"counter":"18446744073709551615"}]})
        )?,
        Some(SourceChange::Insert { .. })
    ));
    Ok(())
}
