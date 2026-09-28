use super::*;
use crate::{
    processor::Processor,
    wire::{Emitter, Message, SolveInput},
};
use anyhow::{Context, Result};
use drasi_core::{computation::InMemoryComputationProvider, models::SourceChange};
use drasi_lib::computation::v1::*;
use drasi_lib::{queries::FetchError, ComponentStatus, DrasiError};
use gpu_contracts::*;
use gpu_policy::Evaluator;
use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[path = "projection_tests.rs"]
mod projections;

#[path = "input_tests.rs"]
mod inputs;

fn processor(kind: Kind, workers: Arc<Workers>) -> Result<Processor> {
    let factory = NativeFactory {
        kind,
        workers: workers.clone(),
    };
    Processor::new(
        kind,
        factory
            .metadata()
            .descriptor(ComponentId::try_new(format!("{kind:?}"))?)?,
        serde_json::json!({"stream":format!("{kind:?}/{}",Uuid::new_v4())}),
        workers,
    )
}
async fn query(id: &str, text: &str) -> Result<ContinuousQueryTransformer> {
    configured_query(id, text, QueryExecutionSettings::default()).await
}
async fn configured_query(
    id: &str,
    text: &str,
    settings: QueryExecutionSettings,
) -> Result<ContinuousQueryTransformer> {
    let mut query = ContinuousQueryTransformer::new_configured(
        ContinuousQueryDefinition {
            graph_id: "gpu-cluster-test".into(),
            id: ComponentId::try_new(id)?,
            query: text.into(),
            language: ComputationQueryLanguage::Cypher,
            output_stream: StreamId::try_new(format!("{id}/out"))?,
            outbox_capacity: NonZeroUsize::new(64).unwrap(),
        },
        Arc::new(InMemoryComputationProvider),
        QueryOptions::default(),
        settings,
        None,
    )
    .await?;
    query.start().await?;
    Ok(query)
}
async fn project(
    query: &mut ContinuousQueryTransformer,
    outputs: &[OutputEnvelope],
) -> Result<Vec<serde_json::Value>> {
    for output in outputs {
        let projected = query
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: output.envelope.clone(),
            })
            .await
            .with_context(|| {
                format!(
                    "query {} processing stream {} sequence {}",
                    query.descriptor().id(),
                    output.envelope.system().stream(),
                    output.envelope.system().sequence(),
                )
            })?;
        query.delivery_completed(&projected).await?;
    }
    query
        .results()
        .snapshot()?
        .rows
        .values()
        .map(|record| {
            Ok(serde_json::to_value(
                QueryChangeCodec::decode_row(record)?.values,
            )?)
        })
        .collect()
}
async fn send(
    query: &mut ContinuousQueryTransformer,
    emitter: &mut Emitter,
    key: &str,
    message: &Message,
) -> Result<Vec<OutputEnvelope>> {
    let change = emitter
        .record("Input", key, message)?
        .context("test message unchanged")?;
    let output = emitter
        .emit(vec![change], None)?
        .pop()
        .context("test source empty")?;
    let rows = query
        .transform(InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: output.envelope,
        })
        .await?;
    query.delivery_completed(&rows).await?;
    Ok(rows)
}
fn payloads(outputs: &[OutputEnvelope], label: &str) -> Result<Vec<serde_json::Value>> {
    let mut values = Vec::new();
    for output in outputs {
        for change in GraphChangeCodec::decode_changes(&output.envelope)? {
            let element = match change {
                SourceChange::Insert { element } | SourceChange::Update { element } => element,
                _ => continue,
            };
            let drasi_core::models::Element::Node {
                metadata,
                properties,
            } = element
            else {
                continue;
            };
            if metadata.labels.iter().any(|v| v.as_ref() == label) {
                let payload = properties
                    .get("payload")
                    .context("output payload missing")?;
                let drasi_core::models::ElementValue::String(payload) = payload else {
                    anyhow::bail!("nonstring payload");
                };
                values.push(serde_json::from_str(payload)?);
            }
        }
    }
    Ok(values)
}
fn bootstrap(f: &fixtures::Fixture, epoch: Uuid) -> Result<Message> {
    let policy = Evaluator::new()?.evaluate(&f.configuration)?;
    Ok(Message::Bootstrap {
        epoch,
        commit: "bootstrap-complete".into(),
        configuration: f.configuration.clone(),
        settings: f.settings.clone(),
        plan: Plan {
            fleet_id: "demo".into(),
            plan_version: 1,
            decision_id: Uuid::new_v4(),
            config_fingerprint: f.configuration.fingerprint()?,
            policy_signature: policy.policy_signature,
            policy_bundle_hash: policy.policy_bundle_hash,
            assignments: f.assignments.clone(),
            decision_details: serde_json::json!({"observation_epoch":epoch,"reason_codes":["fixture-setup"]}),
        },
    })
}
async fn poll_until(p: &mut Processor, label: &str) -> Result<Vec<OutputEnvelope>> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut collected = Vec::new();
        loop {
            let outputs = p.on_wakeup().await?;
            let found = !payloads(&outputs, label)?.is_empty();
            collected.extend(outputs);
            if found {
                return Ok(collected);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[tokio::test]
async fn actual_cq_native_policy_simulator_and_five_second_non_event() -> Result<()> {
    let f = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let workers = Arc::new(Workers::default());
    let mut policy = processor(Kind::Policy, workers.clone())?;
    let mut simulator = processor(Kind::Simulator, workers)?;
    policy.start().await?;
    simulator.start().await?;
    let mut inputs = query("inputs", "MATCH (m:Input) RETURN m.payload AS payload").await?;
    let mut source = Emitter::new(StreamId::try_new("test-source")?);
    let bootstrap = bootstrap(&f, epoch)?;
    for row in send(&mut inputs, &mut source, "bootstrap", &bootstrap).await? {
        let input = InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: row.envelope,
        };
        policy.transform(input.clone()).await?;
        simulator.transform(input).await?;
    }
    let assessment = poll_until(&mut policy, "PolicyAssessment").await?;
    let message: Message =
        serde_json::from_value(payloads(&assessment, "PolicyAssessment")?.remove(0))?;
    for row in send(&mut inputs, &mut source, "policy", &message).await? {
        simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
    }
    let samples = simulator.on_wakeup().await?;
    assert_eq!(payloads(&samples, "GpuSample")?.len(), 6);
    let mut heartbeat = query(
        "missing",
        include_str!("../../../queries/missing-gpu-reports.cypher"),
    )
    .await?;
    let sent = Instant::now();
    for output in &samples {
        let outputs = heartbeat
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: output.envelope.clone(),
            })
            .await?;
        heartbeat.delivery_completed(&outputs).await?;
        assert!(outputs
            .iter()
            .all(|o| o.envelope.changes().operations().is_empty()));
    }
    let mut settings = f.settings.clone();
    for setting in settings.values_mut() {
        setting.reporting_enabled = false;
        setting.revision += 1;
    }
    let message = Message::Configuration {
        epoch,
        commit: "reporting-disabled-commit".into(),
        configuration: f.configuration.clone(),
        settings,
    };
    for row in send(&mut inputs, &mut source, "configuration", &message).await? {
        let outputs = simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
        assert!(
            payloads(&outputs, "GpuSample")?.is_empty(),
            "settings/fencing must not refresh heartbeat"
        );
    }
    assert!(payloads(&simulator.on_wakeup().await?, "GpuSample")?.is_empty());
    let wakeup = heartbeat
        .wakeup_source()
        .context("time-aware query has no wakeup")?;
    let count = tokio::time::timeout(Duration::from_millis(5500), async {
        let mut count = 0;
        while count < 6 {
            wakeup.wait().await?;
            let expired = heartbeat.on_wakeup().await?;
            heartbeat.delivery_completed(&expired).await?;
            count += expired
                .iter()
                .map(|o| o.envelope.changes().operations().len())
                .sum::<usize>();
        }
        Ok::<_, anyhow::Error>(count)
    })
    .await??;
    assert_eq!(
        count, 6,
        "all stopped reporters must expire without another sample"
    );
    assert!(sent.elapsed() >= Duration::from_millis(4900));
    assert!(
        sent.elapsed() < Duration::from_millis(5500),
        "expiry exceeded the 500 ms allowance"
    );
    let invalidated = Instant::now();
    let mut acknowledgements = Vec::new();
    for row in send(
        &mut inputs,
        &mut source,
        "source-failure",
        &Message::Unavailable {
            epoch,
            reason: "source disconnected".into(),
        },
    )
    .await?
    {
        let output = simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
        assert!(payloads(&output, "GpuSample")?.is_empty());
        acknowledgements.extend(payloads(&output, "PolicyEnforcement")?);
    }
    assert_eq!(acknowledgements.len(), 8);
    assert!(acknowledgements.iter().all(|e| e["state"] == "suspended"));
    assert!(
        invalidated.elapsed() < Duration::from_secs(1),
        "observed invalidation was not acknowledged promptly"
    );
    let fresh_policy = Message::Policy {
        epoch,
        config_fingerprint: f.configuration.fingerprint()?,
        assessment: Evaluator::new()?.evaluate(&f.configuration)?,
    };
    for row in send(&mut inputs, &mut source, "delayed-policy", &fresh_policy).await? {
        let output = simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
        assert!(payloads(&output, "PolicyEnforcement")?
            .iter()
            .all(|e| e["state"] != "running"));
    }
    for row in send(&mut inputs, &mut source, "fresh-bootstrap", &bootstrap).await? {
        simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
    }
    let mut resumed = Vec::new();
    for row in send(&mut inputs, &mut source, "recovery-policy", &fresh_policy).await? {
        let output = simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
        assert!(payloads(&output, "GpuSample")?.is_empty());
        resumed.extend(payloads(&output, "PolicyEnforcement")?);
    }
    assert_eq!(resumed.len(), 8);
    assert!(resumed.iter().all(|e| e["state"] == "running"));
    simulator.stop().await?;
    policy.stop().await?;
    inputs.stop().await?;
    heartbeat.stop().await?;
    Ok(())
}

#[tokio::test]
async fn simulator_recovery_requires_a_bootstrap_after_the_latest_invalidation() -> Result<()> {
    let mut f = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let bootstrap = bootstrap(&f, epoch)?;
    let unavailable = Message::Unavailable {
        epoch,
        reason: "source interrupted".into(),
    };
    let policy = Message::Policy {
        epoch,
        config_fingerprint: f.configuration.fingerprint()?,
        assessment: Evaluator::new()?.evaluate(&f.configuration)?,
    };
    let mut simulator = processor(Kind::Simulator, Arc::new(Workers::default()))?;
    simulator.start().await?;
    let mut inputs = query(
        "recovery-input",
        "MATCH (m:Input) RETURN m.payload AS payload",
    )
    .await?;
    let mut source = Emitter::new(StreamId::try_new("recovery-source")?);
    let steps = [
        (&bootstrap, 0),
        (&unavailable, 0),
        (&policy, 0),
        (&bootstrap, 0),
        (&policy, 8),
        (&unavailable, 0),
        (&bootstrap, 0),
        (&unavailable, 0),
        (&policy, 0),
        (&bootstrap, 0),
        (&policy, 8),
    ];
    for (index, (message, expected_running)) in steps.into_iter().enumerate() {
        let mut running = 0;
        for row in send(&mut inputs, &mut source, &index.to_string(), message).await? {
            let output = simulator
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: row.envelope,
                })
                .await?;
            running += payloads(&output, "PolicyEnforcement")?
                .iter()
                .filter(|v| v["state"] == "running")
                .count();
            assert!(payloads(&output, "GpuSample")?.is_empty());
        }
        assert_eq!(
            running, expected_running,
            "unexpected execution at step {index}"
        );
        if index < 4 {
            assert!(
                payloads(&simulator.on_wakeup().await?, "GpuSample")?.is_empty(),
                "stale bootstrap initialized the simulator"
            );
        }
    }
    assert_eq!(
        payloads(&simulator.on_wakeup().await?, "GpuSample")?.len(),
        6
    );
    let removed = *f
        .configuration
        .gpus
        .keys()
        .next()
        .context("fixture GPUs missing")?;
    f.configuration.gpus.remove(&removed);
    f.settings.remove(&removed);
    let configuration = Message::Configuration {
        epoch,
        commit: "gpu-removed".into(),
        configuration: f.configuration,
        settings: f.settings,
    };
    let mut deleted_samples = 0;
    for row in send(&mut inputs, &mut source, "removal", &configuration).await? {
        let output = simulator
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
        for output in output {
            deleted_samples += GraphChangeCodec::decode_changes(&output.envelope)?.iter().filter(|change| {
                matches!(change, SourceChange::Delete { metadata } if metadata.labels.iter().any(|v| v.as_ref() == "GpuSample"))
            }).count();
        }
    }
    assert_eq!(
        deleted_samples, 1,
        "removed inventory must delete the old sample"
    );
    simulator.stop().await?;
    simulator.stop().await?;
    assert!(
        simulator.start().await.is_err(),
        "restart must construct fresh volatile state"
    );
    inputs.stop().await?;
    Ok(())
}

#[tokio::test]
async fn native_root_query_is_exposed_through_ordinary_read_apis() -> Result<()> {
    let (source, _handle) = drasi_source_application::ApplicationSource::new(
        "probe",
        drasi_source_application::ApplicationSourceConfig {
            properties: Default::default(),
            durability: None,
        },
    )?;
    let drasi = drasi_lib::DrasiLib::builder()
        .with_id("gpu-demo-probe")
        .with_source(source)
        .build()
        .await?;
    let root_id = drasi.computation_control()?.desired_snapshot().id.clone();
    let config = drasi_lib::Query::cypher("ui-gpus")
        .query("MATCH (n:Probe) RETURN n.id AS id")
        .from_source("probe")
        .enable_bootstrap(false)
        .auto_start(false)
        .build();
    let batch: ComponentBatch = drasi
        .computation_pipeline()?
        .source(
            drasi.borrow_computation_source("probe").await?,
            SourceSubscriptionOptions::default(),
        )?
        .query(config.clone())
        .build()?
        .auto_start(false);
    let report: ReconciliationReport = drasi.add_components(batch).await?;
    assert!(
        report.committed,
        "component declarations were not committed"
    );
    assert_eq!(report.summary, OperationSummary::Completed);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    tokio::time::timeout(
        Duration::from_secs(10),
        drasi.computation_component("ui-gpus")?.wait_created(),
    )
    .await
    .context("ui-gpus creation did not finish")??;
    let snapshot = drasi.inspect_computation_graph()?.snapshot();
    assert_eq!(snapshot.desired.id, root_id, "instance root changed");
    let query = snapshot
        .desired
        .nodes
        .iter()
        .find(|node| node.descriptor.id().as_str() == "ui-gpus")
        .context("ui-gpus was not declared in the instance root")?;
    assert_eq!(query.role, ComponentRole::Query);
    let inventory = drasi.inspect_computation_inventory().await?;
    assert_eq!(
        inventory
            .scopes
            .values()
            .filter(|scope| scope.owner.is_none())
            .count(),
        1,
        "the instance must have exactly one root"
    );
    assert_eq!(
        drasi.list_queries().await?,
        vec![("ui-gpus".to_string(), ComponentStatus::Added)]
    );
    assert_eq!(
        serde_json::to_value(drasi.get_query_config("ui-gpus").await?)?,
        serde_json::to_value(&config)?
    );
    assert_eq!(
        drasi.get_query_status("ui-gpus").await?,
        ComponentStatus::Added
    );
    let info = drasi.get_query_info("ui-gpus").await?;
    assert_eq!(info.id, config.id);
    assert_eq!(info.query, config.query);
    assert_eq!(info.status, ComponentStatus::Added);
    assert_eq!(
        serde_json::to_value(info.source_subscriptions)?,
        serde_json::to_value(&config.sources)?
    );
    assert!(info.error_message.is_none());
    let error = drasi.get_query_results("ui-gpus").await.unwrap_err();
    assert!(
        matches!(&error, DrasiError::InvalidState { message } if message == "Query 'ui-gpus' is not running"),
        "an unstarted query must fail as not running, not as missing: {error:?}"
    );
    let query = drasi
        .query_manager()
        .get_query_instance("ui-gpus")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        query.fetch_snapshot().await.unwrap_err(),
        FetchError::NotRunning {
            status: ComponentStatus::Added
        }
    );
    drasi.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn native_placement_and_resilience_keep_hypothetical_allocations_read_only() -> Result<()> {
    let mut f = fixtures::load("fragmentation")?;
    let Message::Bootstrap { plan, .. } = bootstrap(&f, Uuid::new_v4())? else {
        unreachable!();
    };
    let w = Workload::from_profile("assistant", "assistant-v1", 1, "demo-open", "demo")?;
    f.configuration.workloads.insert(w.workload_id, w);
    let input = SolveInput {
        policy: Evaluator::new()?.evaluate(&f.configuration)?,
        capacities: fixtures::baseline_capacities(&f.configuration),
        configuration: f.configuration,
        plan,
    };
    let workers = Arc::new(Workers::default());
    let mut placement = processor(Kind::Placement, workers.clone())?;
    let mut resilience = processor(Kind::Resilience, workers)?;
    placement.start().await?;
    resilience.start().await?;
    let mut inputs = query("scheduling", "MATCH (m:Input) RETURN m.payload AS payload").await?;
    let mut source = Emitter::new(StreamId::try_new("test-scheduling")?);
    for row in send(
        &mut inputs,
        &mut source,
        "schedule",
        &Message::Schedule {
            epoch: Uuid::new_v4(),
            input,
        },
    )
    .await?
    {
        let input = InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: row.envelope,
        };
        placement.transform(input.clone()).await?;
        resilience.transform(input).await?;
    }

    let outputs = poll_until(&mut placement, "CandidatePlan").await?;
    let candidate: Candidate =
        serde_json::from_value(payloads(&outputs, "CandidatePlan")?.remove(0))?;
    assert_eq!(candidate.assignments.len(), 7);
    assert_eq!(candidate.decision_details["moved_replicas"], 1);
    let assessment = poll_until(&mut resilience, "ResilienceAssessment").await?;
    assert!(payloads(&assessment, "CandidatePlan")?.is_empty());
    assert!(payloads(&assessment, "ResilienceAssessment")?[0]
        .get("assignments")
        .is_none());
    placement.stop().await?;
    resilience.stop().await?;
    inputs.stop().await?;
    Ok(())
}

#[tokio::test]
async fn managed_plan_reaction_reports_real_receipts_via_status_producer() -> Result<()> {
    use axum::{routing::post, Json, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}/plans", listener.local_addr()?);
    let server = tokio::spawn(async move {
        axum::serve(listener,Router::new().route("/plans",post(|Json(candidate):Json<Candidate>|async move {
                Json(serde_json::json!({"decision_id":candidate.decision_id,"plan_version":"2","current_plan_version":"2","status":"committed"}))
            }))).await.expect("test HTTP receiver failed");
    });
    let hub = Arc::new(crate::status::Hub::default());
    let factory = AuxiliaryFactory {
        reaction: true,
        hub: hub.clone(),
    };
    let mut reaction = crate::reaction::Reaction::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("plan-writer")?)?,
        serde_json::json!({"endpoint":endpoint,"token":"test-only"}),
        hub.clone(),
    )?;
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: hub.clone(),
    };
    let mut status = crate::status::Producer::new(
        factory
            .metadata()
            .descriptor(ComponentId::try_new("status-producer")?)?,
        serde_json::json!({"stream":"test-runtime-status"}),
        hub,
    )?;
    assert_eq!(reaction.completion(), SinkCompletion::Accepted);
    status.start().await?;
    reaction.start().await?;
    let mut inputs = query("write-plan", "MATCH (m:Input) RETURN m.payload AS payload").await?;
    let mut emitter = Emitter::new(StreamId::try_new("test-plan-source")?);
    let candidate = Candidate {
        decision_id: Uuid::new_v4(),
        expected_plan_version: 1,
        config_fingerprint: "test-config".into(),
        policy_signature: "test-policy".into(),
        policy_bundle_hash: "test-bundle".into(),
        scheduling_signature: "test-scheduling".into(),
        assignments: vec![],
        decision_details: serde_json::json!({"observation_epoch":Uuid::new_v4()}),
    };
    let change = emitter
        .record("Input", "demo", &candidate)?
        .context("test candidate missing")?;
    for output in emitter.emit(vec![change], None)? {
        let rows = inputs
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: output.envelope,
            })
            .await?;
        for row in &rows {
            reaction
                .handle(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: row.envelope.clone(),
                })
                .await?;
        }

        inputs.delivery_completed(&rows).await?;
    }
    let receipt = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let output = status
                .next()
                .await?
                .context("status source unexpectedly exhausted")?;
            let receipts = payloads(&[output], "PlanWriteReceipt")?;
            if let Some(receipt) = receipts.into_iter().next() {
                return Ok::<_, anyhow::Error>(receipt);
            }
        }
    })
    .await??;
    assert_eq!(receipt["decision_id"], candidate.decision_id.to_string());
    assert_eq!(receipt["plan_version"], "2");
    reaction.stop().await?;
    status.stop().await?;
    inputs.stop().await?;
    server.abort();
    let _ = server.await;
    Ok(())
}

#[tokio::test]
async fn policy_infeasibility_gets_only_a_read_only_capacity_diagnostic() -> Result<()> {
    let mut f = fixtures::load("regional-boundary")?;
    let epoch = Uuid::new_v4();
    let Message::Bootstrap { plan, .. } = bootstrap(&f, epoch)? else {
        unreachable!();
    };
    let policy = f
        .configuration
        .policies
        .get_mut("customer-eu-processing")
        .context("fixture policy missing")?;
    policy.allowed_regions.clear();
    policy.revision += 1;
    let input = SolveInput {
        policy: Evaluator::new()?.evaluate(&f.configuration)?,
        capacities: fixtures::baseline_capacities(&f.configuration),
        configuration: f.configuration,
        plan,
    };
    let mut placement = processor(Kind::Placement, Arc::new(Workers::default()))?;
    placement.start().await?;
    let mut inputs = query(
        "blocked-scheduling",
        "MATCH (m:Input) RETURN m.payload AS payload",
    )
    .await?;
    let mut source = Emitter::new(StreamId::try_new("blocked-source")?);
    for row in send(
        &mut inputs,
        &mut source,
        "schedule",
        &Message::Schedule { epoch, input },
    )
    .await?
    {
        placement
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: row.envelope,
            })
            .await?;
    }
    let diagnostic = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let output = placement.on_wakeup().await?;
            assert!(payloads(&output, "CandidatePlan")?.is_empty());
            if let Some(diagnostic) = payloads(&output, "CapacityDiagnostic")?.into_iter().next() {
                return Ok::<_, anyhow::Error>(diagnostic);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert_eq!(diagnostic["diagnostic"]["feasible"], true);
    assert!(diagnostic["diagnostic"].get("assignments").is_none());
    placement.stop().await?;
    inputs.stop().await?;
    Ok(())
}
