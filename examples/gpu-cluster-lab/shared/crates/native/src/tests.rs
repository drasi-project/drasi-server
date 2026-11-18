use super::*;
use crate::{
    processor::Processor,
    wire::{Emitter, Message},
};
use anyhow::{Context, Result};
use drasi_computation_plugin_sdk::{NativeComponent, NativeSink, NativeSource, NativeTransformer};
use drasi_core::{computation::InMemoryComputationProvider, models::SourceChange};
use drasi_lib::computation::v1::*;
use gpu_contracts::*;
use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[path = "projection_tests.rs"]
mod projections;

#[path = "input_tests.rs"]
mod inputs;

#[path = "bootstrap_tests.rs"]
mod bootstrap_transport;

#[path = "producer_tests.rs"]
mod producer_progress;

#[path = "lab_tests.rs"]
mod lab;

fn processor(kind: Kind, workers: Arc<Workers>) -> Result<Processor> {
    let factory = NativeFactory {
        kind,
        workers: workers.clone(),
    };
    Processor::new(
        kind,
        factory
            .metadata()
            .descriptor(crate::test_contexts::component_id(format!("{kind:?}"))?)?,
        serde_json::json!({"stream":format!("{kind:?}/{}",Uuid::new_v4())}),
        workers,
        None,
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
            id: crate::test_contexts::component_id(id)?,
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
    query.start(&crate::test_contexts::lib::start()).await?;
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
                SourceChange::Insert { element }
                | SourceChange::Replace { element }
                | SourceChange::Update { element, .. } => element,
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
    let policy = policy::reference_assessment(&f.configuration)?;
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

#[tokio::test]
async fn actual_cq_native_policy_simulator_and_five_second_non_event() -> Result<()> {
    let f = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let workers = Arc::new(Workers::default());
    let mut simulator = processor(Kind::Simulator, workers)?;
    simulator
        .start(&mut crate::test_contexts::sdk::start())
        .await?;
    let mut inputs = query("inputs", "MATCH (m:Input) RETURN m.payload AS payload").await?;
    let mut source = Emitter::new(
        StreamId::try_new("test-source")?,
        crate::test_contexts::component_id("simulator-input-fixture")?,
        None,
    )?;
    let bootstrap = bootstrap(&f, epoch)?;
    let message = Message::Policy {
        epoch,
        config_fingerprint: f.configuration.fingerprint()?,
        assessment: policy::reference_assessment(&f.configuration)?,
    };
    for (key, message) in [("bootstrap", &bootstrap), ("policy", &message)] {
        for row in send(&mut inputs, &mut source, key, message).await? {
            simulator
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: row.envelope,
                })
                .await?;
        }
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
        assessment: policy::reference_assessment(&f.configuration)?,
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
    simulator.stop(&crate::test_contexts::sdk::stop()).await?;
    inputs.stop(&crate::test_contexts::lib::stop()).await?;
    heartbeat.stop(&crate::test_contexts::lib::stop()).await?;
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
        assessment: policy::reference_assessment(&f.configuration)?,
    };
    let mut simulator = processor(Kind::Simulator, Arc::new(Workers::default()))?;
    simulator
        .start(&mut crate::test_contexts::sdk::start())
        .await?;
    let mut inputs = query(
        "recovery-input",
        "MATCH (m:Input) RETURN m.payload AS payload",
    )
    .await?;
    let mut source = Emitter::new(
        StreamId::try_new("recovery-source")?,
        crate::test_contexts::component_id("recovery-fixture")?,
        None,
    )?;
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
    simulator.stop(&crate::test_contexts::sdk::stop()).await?;
    simulator.stop(&crate::test_contexts::sdk::stop()).await?;
    assert!(
        simulator
            .start(&mut crate::test_contexts::sdk::start())
            .await
            .is_err(),
        "restart must construct fresh volatile state"
    );
    inputs.stop(&crate::test_contexts::lib::stop()).await?;
    Ok(())
}

#[tokio::test]
async fn configured_plugins_plan_the_fragmented_fleet_and_assess_losses() -> Result<()> {
    let mut f = fixtures::load("fragmentation")?;
    let w = Workload::from_profile("assistant", "assistant-v1", 1, "demo-open", "demo")?;
    f.configuration.workloads.insert(w.workload_id, w);
    let mut lab = lab::Lab::new(&[]).await?;
    lab.probe(
        "scenarios",
        "MATCH (s:PlacementScenario) RETURN s.scenario AS scenario, s.value AS value, s.status AS status, s.feasible AS feasible",
        &["solver"],
    )
    .await?;
    lab.probe(
        "capacity",
        "MATCH (s:CapacityStatus) RETURN s.status AS status",
        &["capacity-check"],
    )
    .await?;
    lab.bootstrap(&f, Uuid::new_v4()).await?;
    lab.until("a candidate plan", |lab| {
        Ok(!lab.payloads("placement", "CandidatePlan")?.is_empty())
    })
    .await?;
    let candidate: Candidate = serde_json::from_value(
        lab.payloads("placement", "CandidatePlan")?
            .pop()
            .context("candidate")?,
    )?;
    assert_eq!(candidate.assignments.len(), 7);
    assert_eq!(candidate.decision_details["moved_replicas"], 1);
    let hosts: std::collections::BTreeSet<_> = f
        .configuration
        .gpus
        .values()
        .map(|gpu| gpu.host_id.clone())
        .collect();
    lab.until("worker and region scenarios", |lab| {
        let rows = lab.rows("scenarios")?;
        Ok(rows
            .iter()
            .filter(|row| row["scenario"] == "worker")
            .count()
            == hosts.len()
            && rows.iter().any(|row| row["scenario"] == "region"))
    })
    .await?;
    assert!(lab
        .rows("scenarios")?
        .iter()
        .all(|row| row["feasible"].is_boolean()));
    lab.until("capacity status", |lab| {
        Ok(lab
            .rows("capacity")?
            .first()
            .is_some_and(|row| row["status"] == "optimal"))
    })
    .await?;
    lab.stop().await
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
            .descriptor(crate::test_contexts::component_id("plan-writer")?)?,
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
            .descriptor(crate::test_contexts::component_id("status-producer")?)?,
        serde_json::json!({"stream":"test-runtime-status"}),
        hub,
        None,
    )?;
    assert_eq!(reaction.completion(), SinkCompletion::Accepted);
    status
        .start(&mut crate::test_contexts::sdk::start())
        .await?;
    reaction
        .start(&mut crate::test_contexts::sdk::start())
        .await?;
    let mut inputs = query("write-plan", "MATCH (m:Input) RETURN m.payload AS payload").await?;
    let mut emitter = Emitter::new(
        StreamId::try_new("test-plan-source")?,
        crate::test_contexts::component_id("plan-fixture")?,
        None,
    )?;
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
    reaction.stop(&crate::test_contexts::sdk::stop()).await?;
    status.stop(&crate::test_contexts::sdk::stop()).await?;
    inputs.stop(&crate::test_contexts::lib::stop()).await?;
    server.abort();
    let _ = server.await;
    Ok(())
}

#[tokio::test]
async fn policy_infeasibility_withdraws_the_candidate_and_reports_capacity() -> Result<()> {
    let f = fixtures::load("regional-boundary")?;
    let mut lab = lab::Lab::new(&[]).await?;
    lab.probe(
        "placement-status",
        "MATCH (s:PlacementStatus) RETURN s.status AS status, s.provenance.config_fingerprint AS config_fingerprint",
        &["solver"],
    )
    .await?;
    lab.probe(
        "capacity",
        "MATCH (s:CapacityStatus) RETURN s.status AS status",
        &["capacity-check"],
    )
    .await?;
    lab.bootstrap(&f, Uuid::new_v4()).await?;
    lab.until("an optimal placement", |lab| {
        Ok(lab
            .rows("placement-status")?
            .first()
            .is_some_and(|row| row["status"] == "optimal"))
    })
    .await?;
    let mark = lab.log.len();
    lab.update(
        "placement_policies",
        |policy| policy["policy_id"] == "customer-eu-processing",
        |policy| policy["allowed_regions"] = serde_json::json!([]),
    )
    .await?;
    let mut blocked = f.configuration.clone();
    let policy = blocked
        .policies
        .get_mut("customer-eu-processing")
        .context("fixture policy missing")?;
    policy.allowed_regions.clear();
    policy.revision += 1;
    let fingerprint = serde_json::json!(blocked.fingerprint()?);
    lab.until("an infeasible placement", |lab| {
        Ok(lab.rows("placement-status")?.first().is_some_and(|row| {
            row["status"] == "infeasible" && row["config_fingerprint"] == fingerprint
        }) && lab
            .payloads_since(mark, "placement", "RuntimeStatus")?
            .iter()
            .any(|status| status["status"] == "infeasible"))
    })
    .await?;
    assert!(lab
        .payloads_since(mark, "placement", "CandidatePlan")?
        .is_empty());
    assert_eq!(lab.rows("capacity")?[0]["status"], "optimal");
    lab.stop().await
}
