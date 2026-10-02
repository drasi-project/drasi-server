use super::*;
use crate::{
    bootstrap::{
        notification_bytes, notifications, Receiver, MAX_SNAPSHOT_BYTES, TRANSFER_TIMEOUT,
    },
    inputs::{
        BootstrapBoundary, DatabaseInputs, QueryBootstrapRow, QueryBootstrapWatermark,
        DATABASE_QUERIES,
    },
    lifecycle::{Signals, BOOTSTRAP_COMPLETE},
};
use serde_json::json;
use std::collections::BTreeMap;

fn boundary(padding: usize) -> Result<BootstrapBoundary> {
    let fixture = fixtures::load("regional-boundary")?;
    let epoch = Uuid::new_v4();
    let mut rows = super::inputs::fixture_rows(&fixture, epoch)?;
    rows.get_mut("input-plan").unwrap()[0]["decision_details"]["transport_test"] =
        json!("x".repeat(padding));
    Ok(BootstrapBoundary {
        epoch,
        queries: rows
            .into_iter()
            .enumerate()
            .map(|(index, (id, records))| {
                (
                    id.into(),
                    QueryBootstrapWatermark {
                        sequence: u64::MAX - index as u64,
                        row_count: 1,
                        rows: Some(vec![QueryBootstrapRow {
                            signature: u64::MAX - index as u64,
                            records: records.as_array().unwrap().clone(),
                        }]),
                    },
                )
            })
            .collect(),
    })
}

fn roundtrip(boundary: &BootstrapBoundary) -> Result<BootstrapBoundary> {
    let parts = notifications(boundary)?;
    let mut receiver = Receiver::default();
    let mut complete = None;
    for (index, (kind, payload)) in parts.iter().enumerate() {
        assert!(notification_bytes(kind, payload)? <= MAX_CONTROL_PAYLOAD_BYTES);
        assert!(kind.len() + serde_json::to_vec(payload)?.len() <= MAX_CONTROL_PAYLOAD_BYTES);
        let received = receiver.receive(kind, payload.clone(), 7, Instant::now())?;
        assert_eq!(
            received.is_some(),
            index + 1 == parts.len(),
            "partial snapshot admitted"
        );
        complete = received;
    }
    let complete = complete.context("no verified snapshot")?;
    assert_eq!(
        serde_json::to_value(&complete)?,
        serde_json::to_value(boundary)?
    );
    let (kind, payload) = parts.last().unwrap();
    assert!(receiver
        .receive(kind, payload.clone(), 7, Instant::now())
        .is_err());
    Ok(complete)
}

#[tokio::test]
async fn bootstrap_transport_policy_stays_uninitialized_until_commit_and_reports_timeout(
) -> Result<()> {
    let parts = notifications(&boundary(24_000)?)?;
    for timeout in [false, true] {
        let mut policy = processor(Kind::Policy, Arc::new(Workers::default()))?;
        policy.start().await?;
        for (index, (kind, payload)) in parts.iter().enumerate() {
            {
                let mut signals = policy.signals.lock().unwrap();
                if let Some(boundary) = signals.transfer.receive(
                    kind,
                    payload.clone(),
                    7,
                    if timeout {
                        Instant::now() - TRANSFER_TIMEOUT
                    } else {
                        Instant::now()
                    },
                )? {
                    signals.bootstrap = Some(boundary);
                }
            }
            let outputs = policy.on_wakeup().await?;
            assert_eq!(
                payloads(&outputs, "FleetConfiguration")?.len(),
                usize::from(!timeout && index + 1 == parts.len())
            );
            assert!(
                payloads(&outputs, "PolicyAssessment")?.is_empty(),
                "partial input launched a policy decision"
            );
            if timeout {
                let status = serde_json::to_string(&payloads(&outputs, "RuntimeStatus")?)?;
                assert!(status.contains("timed out"));
                break;
            }
        }
        policy.stop().await?;
    }
    Ok(())
}

#[test]
fn bootstrap_transport_large_row_preserves_entire_database_snapshot() -> Result<()> {
    let original = boundary(24_000)?;
    assert!(serde_json::to_vec(&original)?.len() > 19_840);
    let decoded = roundtrip(&original)?;
    let mut before = DatabaseInputs::default();
    before.bootstrap(original)?;
    let mut after = DatabaseInputs::default();
    assert!(after.take()?.is_none());
    after.bootstrap(decoded)?;
    assert_eq!(
        serde_json::to_value(before.take()?.unwrap())?,
        serde_json::to_value(after.take()?.unwrap())?
    );
    Ok(())
}

#[test]
fn bootstrap_transport_small_empty_and_wire_boundary() -> Result<()> {
    let mut empty = BootstrapBoundary {
        epoch: Uuid::new_v4(),
        queries: BTreeMap::new(),
    };
    for id in DATABASE_QUERIES {
        empty.queries.insert(
            id.into(),
            QueryBootstrapWatermark {
                sequence: 9,
                row_count: 0,
                rows: Some(vec![]),
            },
        );
    }
    assert_eq!(notifications(&empty)?[0].0, BOOTSTRAP_COMPLETE);
    roundtrip(&empty)?;
    for size in [15_000, 16_000, 16_384, 17_000] {
        let mut value = empty.clone();
        value.queries.get_mut("input-plan").unwrap().row_count = 1;
        value.queries.get_mut("input-plan").unwrap().rows = Some(vec![QueryBootstrapRow {
            signature: u64::MAX,
            records: vec![json!("x".repeat(size))],
        }]);
        let wire = notification_bytes(BOOTSTRAP_COMPLETE, &serde_json::to_value(&value)?)?;
        assert_eq!(
            notifications(&value)?.len() == 1,
            wire <= MAX_CONTROL_PAYLOAD_BYTES
        );
        roundtrip(&value)?;
    }
    Ok(())
}

#[test]
fn bootstrap_transport_escaping_unicode_integer_fidelity_and_size_limit() -> Result<()> {
    let mut value = boundary(0)?;
    let row = &mut value
        .queries
        .get_mut("input-plan")
        .unwrap()
        .rows
        .as_mut()
        .unwrap()[0];
    row.records.push(
        json!({"escaped": "\u{0000}\"\\\n\u{1f600}\u{6f22}".repeat(4000), "integer": u64::MAX}),
    );
    roundtrip(&value)?;
    assert!(notifications(&boundary(MAX_SNAPSHOT_BYTES)?)
        .unwrap_err()
        .to_string()
        .contains("1 MiB"));
    let size = serde_json::to_vec(&boundary(0)?)?.len();
    roundtrip(&boundary(MAX_SNAPSHOT_BYTES - size)?)?;
    Ok(())
}

#[test]
fn bootstrap_transport_rejects_duplicate_conflicting_stale_incomplete_and_invalid_parts(
) -> Result<()> {
    let parts = notifications(&boundary(24_000)?)?;
    for case in [
        "duplicate_begin",
        "duplicate_chunk",
        "out_of_order",
        "id",
        "epoch",
        "generation",
        "incomplete",
        "size",
        "count",
        "digest",
        "body_epoch",
        "malformed",
        "without_begin",
        "interrupt",
        "expired",
        "small_conflict",
    ] {
        let mut receiver = Receiver::default();
        let now = Instant::now();
        let (kind, begin) = &parts[0];
        let (chunk_kind, first) = &parts[1];
        let mut begin = begin.clone();
        if case == "size" {
            begin["bytes"] = json!(MAX_SNAPSHOT_BYTES + 1);
        }
        if case == "count" {
            begin["chunks"] = json!(usize::MAX);
        }
        if case == "digest" {
            begin["digest"] = json!("0".repeat(64));
        }
        let mut all = parts.clone();
        if case == "body_epoch" {
            let epoch = json!(Uuid::new_v4());
            begin["epoch"] = epoch.clone();
            for (_, payload) in &mut all {
                payload["epoch"] = epoch.clone();
            }
        }
        let result = if case == "without_begin" {
            receiver.receive(chunk_kind, first.clone(), 7, now)
        } else {
            let started = receiver.receive(kind, begin.clone(), 7, now);
            if case == "size" || case == "count" {
                started
            } else {
                assert!(started?.is_none());
                match case {
                    "duplicate_begin" => receiver.receive(kind, begin, 7, now),
                    "incomplete" => receiver.receive(
                        &parts.last().unwrap().0,
                        parts.last().unwrap().1.clone(),
                        7,
                        now,
                    ),
                    "small_conflict" => receiver.receive(
                        BOOTSTRAP_COMPLETE,
                        serde_json::to_value(boundary(0)?)?,
                        7,
                        now,
                    ),
                    "digest" | "body_epoch" => {
                        for (kind, payload) in all.iter().skip(1).take(all.len() - 2) {
                            assert!(receiver.receive(kind, payload.clone(), 7, now)?.is_none());
                        }
                        receiver.receive(
                            &all.last().unwrap().0,
                            all.last().unwrap().1.clone(),
                            7,
                            now,
                        )
                    }
                    _ => {
                        let mut chunk = first.clone();
                        match case {
                            "duplicate_chunk" => {
                                receiver.receive(chunk_kind, chunk.clone(), 7, now)?;
                            }
                            "out_of_order" => chunk["index"] = json!(2),
                            "id" => chunk["id"] = json!(Uuid::new_v4()),
                            "epoch" => chunk["epoch"] = json!(Uuid::new_v4()),
                            "malformed" => chunk["extra"] = json!(true),
                            "interrupt" => receiver.interrupt(),
                            _ => {}
                        }
                        receiver.receive(
                            chunk_kind,
                            chunk,
                            if case == "generation" { 8 } else { 7 },
                            if case == "expired" {
                                now + TRANSFER_TIMEOUT
                            } else {
                                now
                            },
                        )
                    }
                }
            }
        };
        assert!(result.is_err(), "accepted {case}");
        assert!(
            receiver.receive(kind, parts[0].1.clone(), 7, now).is_err(),
            "failure did not fence {case}"
        );
    }
    Ok(())
}

#[test]
fn bootstrap_transport_timeout_is_observable_and_signal_drain_preserves_transfer() -> Result<()> {
    let parts = notifications(&boundary(24_000)?)?;
    let mut signals = Signals::default();
    signals.transfer.receive(
        &parts[0].0,
        parts[0].1.clone(),
        7,
        Instant::now() - TRANSFER_TIMEOUT,
    )?;
    let (boundary, error) = signals.take();
    assert!(boundary.is_none());
    assert!(error.unwrap().contains("timed out"));
    let mut signals = Signals::default();
    for (kind, payload) in parts {
        if let Some(boundary) = signals
            .transfer
            .receive(&kind, payload, 7, Instant::now())?
        {
            signals.bootstrap = Some(boundary);
        }
        let (boundary, error) = signals.take();
        assert!(error.is_none());
        if boundary.is_some() {
            return Ok(());
        }
    }
    anyhow::bail!("signal drains discarded pending transfer")
}
