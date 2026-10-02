use super::*;
use crate::inputs::{
    BootstrapBoundary, DatabaseInputs, QueryBootstrapRow, QueryBootstrapWatermark, DATABASE_QUERIES,
};
use drasi_core::evaluation::variable_value::VariableValue;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn boundary(epoch: Uuid) -> BootstrapBoundary {
    BootstrapBoundary {
        epoch,
        queries: DATABASE_QUERIES
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    QueryBootstrapWatermark {
                        sequence: 0,
                        row_count: 1,
                        rows: None,
                    },
                )
            })
            .collect(),
    }
}

fn aggregate(query: &str, signature: u64, records: Value) -> Result<ChangeOperation> {
    let after = QueryChangeCodec::encode_row(
        query,
        signature,
        &BTreeMap::from([("records".into(), VariableValue::from(records))]),
        QueryRowKind::Aggregation {
            grouping_keys: vec![],
            default_before: false,
            default_after: false,
        },
        RecordImage::Full,
    )?;
    Ok(ChangeOperation::Added {
        ordinal: signature,
        after,
    })
}

pub(super) fn fixture_rows(
    fixture: &fixtures::Fixture,
    epoch: Uuid,
) -> Result<BTreeMap<&'static str, Value>> {
    let configuration = serde_json::to_value(&fixture.configuration)?;
    let mut rows = BTreeMap::new();
    for (query, field) in [
        ("input-clusters", "clusters"),
        ("input-policies", "policies"),
        ("input-data", "data_profiles"),
        ("input-gpus", "gpus"),
        ("input-workloads", "workloads"),
    ] {
        rows.insert(
            query,
            Value::Array(
                configuration[field]
                    .as_object()
                    .unwrap()
                    .values()
                    .cloned()
                    .collect(),
            ),
        );
    }
    rows.insert(
        "input-settings",
        serde_json::to_value(fixture.settings.values().collect::<Vec<_>>())?,
    );
    let Message::Bootstrap { plan, .. } = bootstrap(fixture, epoch)? else {
        unreachable!()
    };
    rows.insert("input-plan", json!([plan]));
    Ok(rows)
}

#[test]
fn bootstrap_never_confuses_undelivered_sequence_zero_with_empty() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let rows = fixture_rows(&fixture, epoch)?;
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(boundary(epoch))?;
    assert!(inputs.take()?.is_none());
    for query in DATABASE_QUERIES {
        inputs.update(query, 0, &[aggregate(query, 1, rows[query].clone())?])?;
        if query != "input-plan" {
            assert!(inputs.take()?.is_none());
        }
    }
    let assembled = inputs.take()?.context("bootstrap did not complete")?;
    assert_eq!(
        serde_json::to_value(assembled.configuration)?,
        serde_json::to_value(fixture.configuration)?
    );
    assert_eq!(
        serde_json::to_value(assembled.settings)?,
        serde_json::to_value(fixture.settings)?
    );
    assert_eq!(serde_json::to_value(assembled.plan)?, rows["input-plan"][0]);
    assert!(inputs.take()?.is_none());
    Ok(())
}

#[test]
fn bootstrap_requires_explicit_observations_and_preserves_epoch_fence() -> Result<()> {
    let epoch = Uuid::new_v4();
    let mut incomplete = boundary(epoch);
    incomplete.queries.remove("input-plan");
    assert!(incomplete.validate().is_err());
    let mut malformed = boundary(epoch);
    malformed.queries.get_mut("input-plan").unwrap().row_count = 2;
    assert!(malformed.validate().is_err());
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(boundary(epoch))?;
    inputs.unavailable();
    assert!(inputs.bootstrap(boundary(Uuid::new_v4())).is_err());
    assert!(inputs.take()?.is_none());
    Ok(())
}

#[test]
fn explicit_empty_query_bootstrap_does_not_require_a_fictional_event() -> Result<()> {
    let mut fixture = fixtures::load("baseline")?;
    fixture.configuration.workloads.clear();
    fixture.assignments.clear();
    let epoch = Uuid::new_v4();
    let rows = fixture_rows(&fixture, epoch)?;
    let mut inputs = DatabaseInputs::default();
    for query in DATABASE_QUERIES
        .into_iter()
        .filter(|id| *id != "input-workloads")
    {
        inputs.update(query, 0, &[aggregate(query, 1, rows[query].clone())?])?;
    }
    assert!(
        inputs.take()?.is_none(),
        "data alone is not bootstrap completion"
    );
    let mut complete = boundary(epoch);
    complete.queries.insert(
        "input-workloads".into(),
        QueryBootstrapWatermark {
            sequence: 9,
            row_count: 0,
            rows: None,
        },
    );
    inputs.bootstrap(complete)?;
    assert!(inputs
        .take()?
        .context("explicit empty bootstrap stalled")?
        .configuration
        .workloads
        .is_empty());
    Ok(())
}

#[test]
fn aggregate_identity_and_sequence_are_strict_and_errors_are_atomic() -> Result<()> {
    let mut inputs = DatabaseInputs::default();
    let first = aggregate("input-gpus", 1, json!([]))?;
    inputs.update("input-gpus", 1, std::slice::from_ref(&first))?;
    inputs.update("input-gpus", 1, std::slice::from_ref(&first))?;
    assert!(inputs
        .update("input-gpus", 0, std::slice::from_ref(&first))
        .is_err());
    assert!(inputs
        .update("input-gpus", 2, &[aggregate("input-gpus", 2, json!([]))?])
        .is_err());
    assert!(inputs
        .update(
            "input-gpus",
            1,
            &[aggregate("input-gpus", 1, json!([{"changed":true}]))?]
        )
        .is_err());
    inputs.update("input-gpus", 2, &[first])?;
    assert!(inputs
        .update("input-plan", 3, &[aggregate("input-gpus", 1, json!([]))?])
        .is_err());
    Ok(())
}

#[test]
fn bootstrap_rejects_missing_plan_and_duplicate_database_identities() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    for invalid_query in ["input-plan", "input-gpus"] {
        let mut rows = fixture_rows(&fixture, epoch)?;
        if invalid_query == "input-plan" {
            rows.insert(invalid_query, json!([]));
        } else {
            let duplicate = rows[invalid_query][0].clone();
            rows.get_mut(invalid_query)
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(duplicate);
        }
        let mut inputs = DatabaseInputs::default();
        for query in DATABASE_QUERIES {
            inputs.update(query, 0, &[aggregate(query, 1, rows[query].clone())?])?;
        }
        inputs.bootstrap(boundary(epoch))?;
        assert!(inputs.take().is_err());
    }
    Ok(())
}

#[tokio::test]
async fn database_settings_aggregate_publishes_live_updates() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let mut settings = fixture.settings.values().next().unwrap().clone();
    let text = crate::inputs::database_queries()
        .into_iter()
        .find(|(id, _)| *id == "input-settings")
        .context("settings query missing")?
        .1;
    let mut query = query("input-settings", &text).await?;
    let mut source = Emitter::new(StreamId::try_new("settings-update-test")?);
    for (revision, demand) in [(1, 10), (2, 35)] {
        settings.revision = revision;
        settings.background_compute_units = demand;
        let change = source
            .record("gpu_telemetry", "gpu-settings", &settings)?
            .context("changed settings must emit a source event")?;
        let outputs = source.emit(vec![change], None)?;
        let mut published = Vec::new();
        for input in outputs {
            let output = query
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: input.envelope,
                })
                .await?;
            for envelope in &output {
                for operation in envelope.envelope.changes().operations() {
                    if let ChangeOperation::Added { after, .. }
                    | ChangeOperation::Updated { after, .. } = operation
                    {
                        let row = QueryChangeCodec::decode_row(after)?;
                        published.push(serde_json::to_value(&row.values["records"])?);
                    }
                }
            }
            query.delivery_completed(&output).await?;
        }
        assert!(
            published.iter().any(|records| records[0]["background_compute_units"] == demand),
            "settings revision {revision} updated query state without publishing the changed aggregate: {published:?}"
        );
    }
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn keyed_bootstrap_policy_forwards_settings_only_updates() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let rows = fixture_rows(&fixture, epoch)?;
    let mut complete = boundary(epoch);
    let mut source = Emitter::new(StreamId::try_new("policy-settings-test")?);
    let mut settings_query = None;
    for (id, text) in crate::inputs::database_queries() {
        let label = match id {
            "input-clusters" => "regional_clusters",
            "input-policies" => "placement_policies",
            "input-data" => "data_profiles",
            "input-gpus" => "gpu_inventory",
            "input-settings" => "gpu_telemetry",
            "input-workloads" => "workload_requirements",
            "input-plan" => "gpu_placements",
            _ => unreachable!(),
        };
        let mut changes = Vec::new();
        for (index, row) in rows[id].as_array().unwrap().iter().enumerate() {
            changes.extend(source.record(label, &format!("{id}/{index}"), row)?);
        }
        let mut query = query(id, &text).await?;
        let mut sequence = 0;
        for input in source.emit(changes, None)? {
            let output = query
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: input.envelope,
                })
                .await?;
            for envelope in &output {
                sequence = QueryChangeCodec::query_sequence(&envelope.envelope)?;
            }
            query.delivery_completed(&output).await?;
        }
        let snapshot = query.results().snapshot()?;
        let records = snapshot
            .rows
            .values()
            .map(|record| {
                let row = QueryChangeCodec::decode_row(record)?;
                Ok(QueryBootstrapRow {
                    signature: row.signature,
                    records: serde_json::from_value(serde_json::to_value(&row.values["records"])?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        complete.queries.insert(
            id.into(),
            QueryBootstrapWatermark {
                sequence,
                row_count: records.len(),
                rows: Some(records),
            },
        );
        if id == "input-settings" {
            settings_query = Some(query);
        } else {
            query.stop().await?;
        }
    }
    let mut policy = processor(Kind::Policy, Arc::new(Workers::default()))?;
    policy.start().await?;
    policy.signals.lock().unwrap().bootstrap = Some(complete);
    assert_eq!(
        payloads(&policy.on_wakeup().await?, "FleetConfiguration")?.len(),
        1
    );
    let mut changed = rows["input-settings"][0].clone();
    changed["revision"] = json!(2);
    changed["background_compute_units"] = json!(35);
    let gpu = changed["gpu_id"].as_str().unwrap().to_owned();
    let change = source
        .record("gpu_telemetry", "input-settings/0", &changed)?
        .unwrap();
    let mut query = settings_query.unwrap();
    let mut published = Vec::new();
    for input in source.emit(vec![change], None)? {
        let outputs = query
            .transform(InputEnvelope {
                port: PortId::try_new("in")?,
                envelope: input.envelope,
            })
            .await?;
        for output in &outputs {
            published.extend(payloads(
                &policy
                    .transform(InputEnvelope {
                        port: PortId::try_new("in")?,
                        envelope: output.envelope.clone(),
                    })
                    .await?,
                "FleetConfiguration",
            )?);
        }
        query.delivery_completed(&outputs).await?;
    }
    assert!(published.iter().any(|row| row["settings"][&gpu]["background_compute_units"] == 35),
        "Policy must forward settings-only CDC updates even when its policy fingerprint is unchanged");
    policy.stop().await?;
    query.stop().await?;
    Ok(())
}

#[tokio::test]
async fn real_database_queries_reconstruct_the_authoritative_fixture() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let rows = fixture_rows(&fixture, epoch)?;
    let mut source = Emitter::new(StreamId::try_new("database-input-test")?);
    let mut changes = Vec::new();
    for (query_id, label) in [
        ("input-clusters", "regional_clusters"),
        ("input-policies", "placement_policies"),
        ("input-data", "data_profiles"),
        ("input-gpus", "gpu_inventory"),
        ("input-settings", "gpu_telemetry"),
        ("input-workloads", "workload_requirements"),
        ("input-plan", "gpu_placements"),
    ] {
        for (index, row) in rows[query_id].as_array().unwrap().iter().enumerate() {
            changes.extend(source.record(label, &format!("{query_id}/{index}"), row)?);
        }
    }
    let outputs = source.emit(changes, None)?;
    let mut inputs = DatabaseInputs::default();
    let mut complete = boundary(epoch);
    for (id, text) in crate::inputs::database_queries() {
        let mut query = query(id, &text).await?;
        let mut sequence = 0;
        for output in &outputs {
            let emitted = query
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: output.envelope.clone(),
                })
                .await?;
            for output in &emitted {
                sequence = QueryChangeCodec::query_sequence(&output.envelope)?;
                inputs.update(id, sequence, output.envelope.changes().operations())?;
            }
            query.delivery_completed(&emitted).await?;
        }
        complete.queries.insert(
            id.into(),
            QueryBootstrapWatermark {
                sequence,
                row_count: query.results().snapshot()?.rows.len(),
                rows: None,
            },
        );
        query.stop().await?;
    }
    assert!(inputs.take()?.is_none());
    inputs.bootstrap(complete)?;
    let actual = inputs
        .take()?
        .context("actual database CQs did not finish bootstrap")?;
    assert_eq!(
        serde_json::to_value(actual.configuration)?,
        serde_json::to_value(fixture.configuration)?
    );
    assert_eq!(
        serde_json::to_value(actual.settings)?,
        serde_json::to_value(fixture.settings)?
    );
    assert_eq!(serde_json::to_value(actual.plan)?, rows["input-plan"][0]);
    Ok(())
}

#[test]
fn keyed_bootstrap_snapshots_seed_state_without_replaying_source_events() -> Result<()> {
    let fixture = fixtures::load("baseline")?;
    let epoch = Uuid::new_v4();
    let rows = fixture_rows(&fixture, epoch)?;
    let mut complete = boundary(epoch);
    for (query, watermark) in &mut complete.queries {
        watermark.sequence = 4;
        watermark.rows = Some(vec![QueryBootstrapRow {
            signature: 1,
            records: rows[query.as_str()].as_array().unwrap().clone(),
        }]);
    }
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(complete.clone())?;
    assert_eq!(
        serde_json::to_value(inputs.take()?.unwrap().configuration)?,
        serde_json::to_value(&fixture.configuration)?
    );
    inputs.update("input-gpus", 3, &[aggregate("input-gpus", 1, json!([]))?])?;
    assert!(
        inputs.take()?.is_none(),
        "a covered delta cannot replace the authoritative snapshot"
    );
    inputs.update(
        "input-gpus",
        4,
        &[aggregate("input-gpus", 1, rows["input-gpus"].clone())?],
    )?;

    let mut newer = rows["input-gpus"].clone();
    newer[0]["scheduling_enabled"] = json!(false);
    let mut raced = DatabaseInputs::default();
    raced.update("input-gpus", 5, &[aggregate("input-gpus", 1, newer)?])?;
    raced.bootstrap(complete)?;
    let current = raced.take()?.unwrap();
    assert!(
        current
            .configuration
            .gpus
            .values()
            .any(|gpu| !gpu.scheduling_enabled),
        "bootstrap must not overwrite a newer query delta"
    );
    Ok(())
}
