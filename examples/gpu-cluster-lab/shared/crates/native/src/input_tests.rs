use super::*;
use crate::inputs::{
    partition_records, BootstrapBoundary, DatabaseInputs, QueryBootstrapRow,
    QueryBootstrapWatermark, DATABASE_QUERY,
};
use drasi_core::evaluation::variable_value::VariableValue;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn boundary(epoch: Uuid, records: Option<Value>) -> BootstrapBoundary {
    BootstrapBoundary {
        epoch,
        queries: BTreeMap::from([(
            DATABASE_QUERY.into(),
            QueryBootstrapWatermark {
                sequence: 0,
                row_count: 1,
                rows: records.map(|records| {
                    vec![QueryBootstrapRow {
                        signature: 1,
                        records: records.as_array().unwrap().clone(),
                    }]
                }),
            },
        )]),
    }
}

fn aggregate(signature: u64, records: Value) -> Result<ChangeOperation> {
    let after = QueryChangeCodec::encode_row(
        DATABASE_QUERY,
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
    let mut records = Vec::new();
    for (table, field) in [
        ("regional_clusters", "clusters"),
        ("placement_policies", "policies"),
        ("data_profiles", "data_profiles"),
        ("gpu_inventory", "gpus"),
        ("workload_requirements", "workloads"),
    ] {
        for value in configuration[field].as_object().unwrap().values() {
            records.push(json!({"table":table,"value":value}));
        }
    }
    for value in fixture.settings.values() {
        records.push(json!({"table":"gpu_telemetry","value":value}));
    }
    let Message::Bootstrap { plan, .. } = bootstrap(fixture, epoch)? else {
        unreachable!()
    };
    records.push(json!({"table":"gpu_placements","value":plan}));
    Ok(BTreeMap::from([(DATABASE_QUERY, Value::Array(records))]))
}

fn records(epoch: Uuid) -> Result<Value> {
    Ok(fixture_rows(&fixtures::load("baseline")?, epoch)?
        .remove(DATABASE_QUERY)
        .unwrap())
}

#[test]
fn bootstrap_requires_complete_single_query_identity_and_epoch() -> Result<()> {
    let epoch = Uuid::new_v4();
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(boundary(epoch, None))?;
    assert!(
        inputs.take()?.is_none(),
        "undelivered sequence zero is not empty"
    );
    inputs.update(DATABASE_QUERY, 0, &[aggregate(1, records(epoch)?)?])?;
    assert!(inputs.take()?.is_some());
    assert!(inputs.take()?.is_none());
    inputs.unavailable();
    assert!(inputs.bootstrap(boundary(Uuid::new_v4(), None)).is_err());
    let mut invalid = boundary(epoch, None);
    invalid.queries.clear();
    assert!(invalid.validate().is_err());
    let mut invalid = boundary(epoch, None);
    invalid.queries.values_mut().next().unwrap().row_count = 2;
    assert!(invalid.validate().is_err());
    Ok(())
}

#[test]
fn bootstrap_preserves_complete_configuration_and_empty_tables() -> Result<()> {
    for empty_workloads in [false, true] {
        let mut fixture = fixtures::load("baseline")?;
        if empty_workloads {
            fixture.configuration.workloads.clear();
            fixture.assignments.clear();
        }
        let epoch = Uuid::new_v4();
        let records = fixture_rows(&fixture, epoch)?
            .remove(DATABASE_QUERY)
            .unwrap();
        let tables = partition_records(records.as_array().unwrap())?;
        let mut inputs = DatabaseInputs::default();
        inputs.bootstrap(boundary(epoch, Some(records)))?;
        let actual = inputs.take()?.context("bootstrap did not complete")?;
        assert_eq!(
            serde_json::to_value(actual.configuration)?,
            serde_json::to_value(fixture.configuration)?
        );
        assert_eq!(
            serde_json::to_value(actual.settings)?,
            serde_json::to_value(fixture.settings)?
        );
        assert_eq!(serde_json::to_value(actual.plan)?, tables["input-plan"][0]);
    }
    Ok(())
}

#[test]
fn rejects_missing_plan_duplicates_unknown_tables_and_row_bounds() -> Result<()> {
    let epoch = Uuid::new_v4();
    for mode in 0..5 {
        let mut records = records(epoch)?.as_array().unwrap().clone();
        match mode {
            0 => records.retain(|r| r["table"] != "gpu_placements"),
            1 => records.push(records[0].clone()),
            2 => records[0]["table"] = json!("unknown"),
            3 => records[0]["value"] = Value::Null,
            _ => records.extend(vec![records[0].clone(); 65]),
        }
        let mut inputs = DatabaseInputs::default();
        inputs.bootstrap(boundary(epoch, Some(json!(records))))?;
        assert!(inputs.take().is_err(), "invalid mode {mode} accepted");
    }
    Ok(())
}

#[test]
fn aggregate_identity_and_sequence_errors_are_atomic() -> Result<()> {
    let epoch = Uuid::new_v4();
    let first = aggregate(1, records(epoch)?)?;
    let mut inputs = DatabaseInputs::default();
    inputs.update(DATABASE_QUERY, 1, std::slice::from_ref(&first))?;
    inputs.update(DATABASE_QUERY, 1, std::slice::from_ref(&first))?;
    assert!(inputs
        .update(DATABASE_QUERY, 0, std::slice::from_ref(&first))
        .is_err());
    assert!(inputs
        .update(DATABASE_QUERY, 2, &[aggregate(2, json!([]))?])
        .is_err());
    assert!(inputs
        .update(DATABASE_QUERY, 1, &[aggregate(1, json!([]))?])
        .is_err());
    assert!(inputs
        .update("input-plan", 2, std::slice::from_ref(&first))
        .is_err());
    inputs.update(DATABASE_QUERY, 2, &[first])?;
    inputs.bootstrap(boundary(epoch, None))?;
    assert!(inputs.take()?.is_some());
    Ok(())
}

#[test]
fn snapshot_watermark_never_overwrites_a_newer_complete_configuration() -> Result<()> {
    let epoch = Uuid::new_v4();
    let original = records(epoch)?;
    let mut complete = boundary(epoch, Some(original.clone()));
    complete.queries.get_mut(DATABASE_QUERY).unwrap().sequence = 4;
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(complete.clone())?;
    inputs.take()?;
    inputs.update(DATABASE_QUERY, 3, &[aggregate(1, json!([]))?])?;
    assert!(inputs.take()?.is_none());
    inputs.update(DATABASE_QUERY, 4, &[aggregate(1, original.clone())?])?;
    let mut newer = original;
    for record in newer.as_array_mut().unwrap() {
        if record["table"] == "gpu_inventory" {
            record["value"]["scheduling_enabled"] = json!(false);
        }
    }
    let mut raced = DatabaseInputs::default();
    raced.update(DATABASE_QUERY, 5, &[aggregate(1, newer)?])?;
    raced.bootstrap(complete)?;
    assert!(raced
        .take()?
        .unwrap()
        .configuration
        .gpus
        .values()
        .all(|gpu| !gpu.scheduling_enabled));
    Ok(())
}

#[tokio::test]
async fn single_configuration_query_projects_every_table_and_live_settings() -> Result<()> {
    let epoch = Uuid::new_v4();
    let fixture = fixtures::load("baseline")?;
    let records = fixture_rows(&fixture, epoch)?
        .remove(DATABASE_QUERY)
        .unwrap();
    let text = crate::inputs::database_queries().remove(0).1;
    let mut query = query(DATABASE_QUERY, &text).await?;
    let mut source = Emitter::new(StreamId::try_new("database-input-test")?);
    let mut changes = Vec::new();
    for (index, record) in records.as_array().unwrap().iter().enumerate() {
        let mut value = record["value"].clone();
        value["__gpu_table"] = record["table"].clone();
        changes.extend(source.record(
            record["table"].as_str().unwrap(),
            &index.to_string(),
            &value,
        )?);
    }
    let outputs = source.emit(changes, None)?;
    project(&mut query, &outputs).await?;
    let snapshot = query.results().snapshot()?;
    assert_eq!(snapshot.rows.len(), 1);
    let row = QueryChangeCodec::decode_row(snapshot.rows.values().next().unwrap())?;
    let actual: Value = serde_json::to_value(&row.values["records"])?;
    let mut inputs = DatabaseInputs::default();
    inputs.bootstrap(boundary(epoch, Some(actual)))?;
    assert_eq!(
        serde_json::to_value(inputs.take()?.unwrap().configuration)?,
        serde_json::to_value(&fixture.configuration)?
    );
    let mut policy = processor(Kind::Policy, Arc::new(Workers::default()))?;
    policy.start().await?;
    let mut complete = boundary(epoch, None);
    complete.queries.insert(
        DATABASE_QUERY.into(),
        QueryBootstrapWatermark {
            sequence: snapshot.as_of_sequence,
            row_count: 1,
            rows: Some(vec![QueryBootstrapRow {
                signature: row.signature,
                records: serde_json::from_value(serde_json::to_value(
                    row.values["records"].clone(),
                )?)?,
            }]),
        },
    );
    policy.signals.lock().unwrap().bootstrap = Some(complete);
    assert_eq!(
        payloads(&policy.on_wakeup().await?, "FleetConfiguration")?.len(),
        1
    );
    let (index, record) = records
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(_, record)| record["table"] == "gpu_telemetry")
        .unwrap();
    let mut settings = record["value"].clone();
    settings["revision"] = json!(2);
    settings["background_compute_units"] = json!(35);
    settings["__gpu_table"] = json!("gpu_telemetry");
    let gpu = settings["gpu_id"].as_str().unwrap().to_owned();
    let change = source
        .record("gpu_telemetry", &index.to_string(), &settings)?
        .unwrap();
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
    assert!(published
        .iter()
        .any(|row| row["settings"][&gpu]["background_compute_units"] == 35));
    policy.stop().await?;
    query.stop().await?;
    Ok(())
}

#[cfg(feature = "runtime")]
#[tokio::test]
async fn committed_configuration_is_atomic_persistent_and_retracts_deleted_inputs() -> Result<()> {
    use crate::inputs::DatabaseSnapshot;
    use drasi_index_rocksdb::{
        computation::RocksDbComputationProvider, RocksDbMemoryBudget, RocksIndexOptions,
    };
    use std::{num::NonZeroU64, path::Path};

    fn limits() -> SourceTransactionLimits {
        SourceTransactionLimits {
            max_changes: NonZeroUsize::new(1024).unwrap(),
            max_bytes: NonZeroUsize::new(8 << 20).unwrap(),
            max_duration_ms: NonZeroU64::new(30_000).unwrap(),
        }
    }
    async fn persistent(path: &Path) -> Result<ContinuousQueryTransformer> {
        let mut query = ContinuousQueryTransformer::new_configured(
            ContinuousQueryDefinition {
                graph_id: "gpu-transaction-test".into(),
                id: ComponentId::try_new(DATABASE_QUERY)?,
                query: crate::inputs::database_queries().remove(0).1,
                language: ComputationQueryLanguage::Cypher,
                output_stream: StreamId::try_new("input-configuration/out")?,
                outbox_capacity: NonZeroUsize::new(8).unwrap(),
            },
            Arc::new(RocksDbComputationProvider::new(
                path,
                RocksIndexOptions::new(
                    false,
                    false,
                    RocksDbMemoryBudget::from_total_budget_bytes(32 << 20)?,
                ),
            )),
            QueryOptions::default(),
            QueryExecutionSettings {
                source_transactions: Some(limits()),
                ..Default::default()
            },
            None,
        )
        .await?;
        query.start().await?;
        Ok(query)
    }
    fn transaction(sequence: u64, changes: Vec<SourceChange>) -> Result<InputEnvelope> {
        let mut builder = SourceTransactionBuilder::new("postgres", limits())?;
        for change in changes {
            builder.push(change)?;
        }
        let position = sequence.to_be_bytes().to_vec();
        Ok(InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: builder
                .commit(position.clone().into(), position.into())?
                .into_envelope(
                    StreamId::try_new("postgres/out")?,
                    sequence,
                    chrono::Utc::now(),
                )?,
        })
    }
    fn snapshot(query: &ContinuousQueryTransformer, epoch: Uuid) -> Result<DatabaseSnapshot> {
        let snapshot = query.results().snapshot()?;
        assert_eq!(snapshot.rows.len(), 1);
        let row = QueryChangeCodec::decode_row(snapshot.rows.values().next().unwrap())?;
        let records: Vec<Value> =
            serde_json::from_value(serde_json::to_value(&row.values["records"])?)?;
        DatabaseSnapshot::from_records(epoch, &records)
    }

    let directory = tempfile::tempdir()?;
    let epoch = Uuid::new_v4();
    let mut records = records(epoch)?.as_array().unwrap().clone();
    let mut source = Emitter::new(StreamId::try_new("postgres")?);
    let mut changes = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let mut value = record["value"].clone();
        value["__gpu_table"] = record["table"].clone();
        changes.extend(source.record(
            record["table"].as_str().unwrap(),
            &index.to_string(),
            &value,
        )?);
    }
    let mut query = persistent(directory.path()).await?;
    let first = query.transform(transaction(1, changes)?).await?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].envelope.changes().operations().len(), 1);
    query.delivery_completed(&first).await?;
    let initial = snapshot(&query, epoch)?;
    let mut policy = processor(Kind::Policy, Arc::new(Workers::default()))?;
    policy.start().await?;
    let row =
        QueryChangeCodec::decode_row(query.results().snapshot()?.rows.values().next().unwrap())?;
    let mut complete = boundary(epoch, Some(serde_json::to_value(&row.values["records"])?));
    complete.queries.get_mut(DATABASE_QUERY).unwrap().sequence = 1;
    complete
        .queries
        .get_mut(DATABASE_QUERY)
        .unwrap()
        .rows
        .as_mut()
        .unwrap()[0]
        .signature = row.signature;
    policy.signals.lock().unwrap().bootstrap = Some(complete);
    let projected = policy.on_wakeup().await?;
    assert_eq!(
        payloads(&projected, "gpu_inventory")?.len(),
        initial.configuration.gpus.len()
    );
    assert_eq!(
        payloads(&projected, "workload_requirements")?.len(),
        initial.configuration.workloads.len()
    );
    let (ui_id, ui_text, settings) = crate::projections::definitions()
        .into_iter()
        .find(|(id, _, _)| *id == "ui-workloads")
        .unwrap();
    let mut ui = configured_query(ui_id, ui_text, settings).await?;
    assert_eq!(
        project(&mut ui, &projected).await?.len(),
        initial.configuration.workloads.len()
    );

    // An intermediate invalid workload must never escape a committed multi-table transaction.
    let (index, workload) = records
        .iter_mut()
        .enumerate()
        .find(|(_, record)| record["table"] == "workload_requirements")
        .unwrap();
    workload["value"]["__gpu_table"] = json!("workload_requirements");
    workload["value"]["replicas"] = json!(999);
    let mut changes = vec![source
        .record(
            "workload_requirements",
            &index.to_string(),
            &workload["value"],
        )?
        .unwrap()];
    workload["value"]["replicas"] = json!(1);
    workload["value"]["revision"] = json!("2");
    changes.extend(source.record(
        "workload_requirements",
        &index.to_string(),
        &workload["value"],
    )?);
    for (index, record) in records.iter_mut().enumerate() {
        if record["table"] == "gpu_telemetry" {
            record["value"]["background_compute_units"] = json!(35);
            record["value"]["__gpu_table"] = json!("gpu_telemetry");
            changes.extend(source.record("gpu_telemetry", &index.to_string(), &record["value"])?);
        }
    }
    let committed = transaction(2, changes)?;
    let outputs = query.transform(committed.clone()).await?;
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].envelope.changes().operations().len(), 1);
    let current = snapshot(&query, epoch)?;
    assert!(current
        .configuration
        .workloads
        .values()
        .all(|workload| workload.replicas <= 2));
    assert!(current
        .settings
        .values()
        .all(|settings| settings.background_compute_units == 35));
    let projected = policy
        .transform(InputEnvelope {
            port: PortId::try_new("in")?,
            envelope: outputs[0].envelope.clone(),
        })
        .await?;
    assert_eq!(payloads(&projected, "FleetConfiguration")?.len(), 1);
    query.delivery_completed(&outputs).await?;
    query.stop().await?;
    drop(query);
    let mut query = persistent(directory.path()).await?;
    assert_eq!(
        serde_json::to_value(snapshot(&query, epoch)?)?,
        serde_json::to_value(&current)?
    );
    assert!(query.transform(committed).await?.is_empty());

    let mut changes = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if record["table"] == "workload_requirements" {
            changes.extend(source.remove("workload_requirements", &index.to_string())?);
        }
    }
    let outputs = query.transform(transaction(3, changes)?).await?;
    assert!(snapshot(&query, epoch)?.configuration.workloads.is_empty());
    let mut projected = Vec::new();
    for output in &outputs {
        projected.extend(
            policy
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: output.envelope.clone(),
                })
                .await?,
        );
    }
    assert!(
        project(&mut ui, &projected).await?.is_empty(),
        "deleted workloads remain visible"
    );
    query.delivery_completed(&outputs).await?;
    policy.stop().await?;
    ui.stop().await?;
    query.stop().await?;
    Ok(())
}
