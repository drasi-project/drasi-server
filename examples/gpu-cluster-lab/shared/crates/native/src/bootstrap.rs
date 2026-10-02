//! Bounded transport for the embedded host's canonical query bootstrap snapshots.
use crate::{inputs::BootstrapBoundary, lifecycle::BOOTSTRAP_COMPLETE};
use anyhow::{ensure, Context, Result};
use drasi_computation_plugin_sdk::{ControlDirection, ControlMessage, ControlNotification};
use drasi_lib::computation::v1::{ComponentId, MAX_CONTROL_JSON_DEPTH, MAX_CONTROL_PAYLOAD_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Write,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const TRANSFER: &str = "gpu.lab/database-bootstrap-transfer/v1";
pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const CHUNK_BYTES: usize = 2048;
const MAX_CHUNKS: usize = MAX_SNAPSHOT_BYTES / CHUNK_BYTES + 1;
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(15);

fn validate_depth(value: &Value) -> Result<()> {
    let mut pending = vec![(value, 0)];
    while let Some((value, depth)) = pending.pop() {
        ensure!(
            depth <= MAX_CONTROL_JSON_DEPTH,
            "GPU bootstrap exceeds Core JSON depth limit"
        );
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(values) => {
                pending.extend(values.values().map(|value| (value, depth + 1)))
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
enum Part {
    Begin {
        id: Uuid,
        epoch: Uuid,
        bytes: usize,
        chunks: usize,
        digest: String,
    },
    Chunk {
        id: Uuid,
        epoch: Uuid,
        index: usize,
        data: String,
    },
    Commit {
        id: Uuid,
        epoch: Uuid,
    },
}

struct LimitedBytes(Vec<u8>);
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_SNAPSHOT_BYTES - self.0.len() {
            return Err(std::io::Error::other(
                "GPU bootstrap exceeds the 1 MiB snapshot limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn notification_bytes(kind: &str, payload: &Value) -> Result<usize> {
    // Include SDK message framing and the widest generation, beyond Core's kind+JSON budget.
    Ok(serde_json::to_vec(&ControlMessage {
        from: ComponentId::try_new("input-plan")?,
        generation: u64::MAX,
        direction: ControlDirection::Downstream,
        notification: ControlNotification::Custom {
            kind: kind.into(),
            payload: payload.clone(),
        },
    })?
    .len())
}

pub fn notifications(boundary: &BootstrapBoundary) -> Result<Vec<(String, Value)>> {
    boundary.validate()?;
    let mut bytes = LimitedBytes(Vec::new());
    serde_json::to_writer(&mut bytes, boundary)?;
    let text = String::from_utf8(bytes.0)?;
    let value: Value = serde_json::from_str(&text)?;
    validate_depth(&value)?;
    if notification_bytes(BOOTSTRAP_COMPLETE, &value)? <= MAX_CONTROL_PAYLOAD_BYTES {
        return Ok(vec![(BOOTSTRAP_COMPLETE.into(), value)]);
    }
    let mut chunks = Vec::new();
    let mut remaining = text.as_str();
    while !remaining.is_empty() {
        let mut end = remaining.len().min(CHUNK_BYTES);
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(remaining[..end].to_owned());
        remaining = &remaining[end..];
    }
    ensure!(
        chunks.len() <= MAX_CHUNKS,
        "GPU bootstrap chunk count exceeded"
    );
    let id = Uuid::new_v4();
    let epoch = boundary.epoch;
    let mut parts = vec![Part::Begin {
        id,
        epoch,
        bytes: text.len(),
        chunks: chunks.len(),
        digest: gpu_contracts::hash("gpu-bootstrap-transfer-v1", &text)?,
    }];
    parts.extend(
        chunks
            .into_iter()
            .enumerate()
            .map(|(index, data)| Part::Chunk {
                id,
                epoch,
                index,
                data,
            }),
    );
    parts.push(Part::Commit { id, epoch });
    parts
        .into_iter()
        .map(|part| {
            let payload = serde_json::to_value(part)?;
            ensure!(
                notification_bytes(TRANSFER, &payload)? <= MAX_CONTROL_PAYLOAD_BYTES,
                "GPU bootstrap notification exceeds wire limit"
            );
            Ok((TRANSFER.into(), payload))
        })
        .collect()
}

struct Active {
    id: Uuid,
    epoch: Uuid,
    generation: u64,
    bytes: usize,
    chunks: usize,
    next: usize,
    digest: String,
    text: String,
    deadline: Instant,
}

#[derive(Default)]
pub(crate) struct Receiver {
    active: Option<Active>,
    terminal: bool,
}
impl Receiver {
    pub fn interrupt(&mut self) {
        if self.active.is_some() {
            self.abort();
        }
    }
    pub fn abort(&mut self) {
        self.active = None;
        self.terminal = true;
    }
    pub fn expired(&mut self, now: Instant) -> bool {
        if self
            .active
            .as_ref()
            .is_some_and(|active| now >= active.deadline)
        {
            self.abort();
            return true;
        }
        false
    }
    pub fn receive(
        &mut self,
        kind: &str,
        payload: Value,
        generation: u64,
        now: Instant,
    ) -> Result<Option<BootstrapBoundary>> {
        let result = self.accept(kind, payload, generation, now);
        if result.is_err() {
            self.abort();
        }
        result
    }
    fn accept(
        &mut self,
        kind: &str,
        payload: Value,
        generation: u64,
        now: Instant,
    ) -> Result<Option<BootstrapBoundary>> {
        ensure!(!self.expired(now), "GPU bootstrap transfer timed out");
        ensure!(
            !self.terminal,
            "GPU bootstrap transfer already completed or failed; reconstruct the component"
        );
        ensure!(
            notification_bytes(kind, &payload)? <= MAX_CONTROL_PAYLOAD_BYTES,
            "GPU bootstrap notification exceeds wire limit"
        );
        if kind == BOOTSTRAP_COMPLETE {
            ensure!(
                self.active.is_none(),
                "small bootstrap conflicts with an active transfer"
            );
            let boundary: BootstrapBoundary = serde_json::from_value(payload)?;
            boundary.validate()?;
            self.terminal = true;
            return Ok(Some(boundary));
        }
        ensure!(kind == TRANSFER, "unsupported GPU bootstrap notification");
        match serde_json::from_value::<Part>(payload)? {
            Part::Begin {
                id,
                epoch,
                bytes,
                chunks,
                digest,
            } => {
                ensure!(
                    self.active.is_none(),
                    "duplicate or conflicting GPU bootstrap begin"
                );
                ensure!(
                    bytes > 0 && bytes <= MAX_SNAPSHOT_BYTES,
                    "GPU bootstrap snapshot size outside 1 MiB bound"
                );
                ensure!(
                    chunks > 0 && chunks <= MAX_CHUNKS && chunks <= bytes,
                    "invalid GPU bootstrap chunk count"
                );
                ensure!(
                    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "invalid GPU bootstrap digest"
                );
                self.active = Some(Active {
                    id,
                    epoch,
                    generation,
                    bytes,
                    chunks,
                    digest,
                    next: 0,
                    text: String::with_capacity(bytes),
                    deadline: now + TRANSFER_TIMEOUT,
                });
                Ok(None)
            }
            Part::Chunk {
                id,
                epoch,
                index,
                data,
            } => {
                let active = self
                    .active
                    .as_mut()
                    .context("GPU bootstrap chunk without begin")?;
                ensure!(
                    id == active.id && epoch == active.epoch && generation == active.generation,
                    "stale or conflicting GPU bootstrap transfer"
                );
                ensure!(
                    index == active.next && index < active.chunks,
                    "duplicate or out-of-order GPU bootstrap chunk"
                );
                ensure!(
                    !data.is_empty()
                        && data.len() <= CHUNK_BYTES
                        && data.len() <= active.bytes - active.text.len(),
                    "GPU bootstrap chunk exceeds declared bounds"
                );
                active.text.push_str(&data);
                active.next += 1;
                Ok(None)
            }
            Part::Commit { id, epoch } => {
                let active = self
                    .active
                    .as_ref()
                    .context("GPU bootstrap commit without begin")?;
                ensure!(
                    id == active.id && epoch == active.epoch && generation == active.generation,
                    "stale or conflicting GPU bootstrap commit"
                );
                ensure!(
                    active.next == active.chunks && active.text.len() == active.bytes,
                    "incomplete GPU bootstrap transfer"
                );
                ensure!(
                    gpu_contracts::hash("gpu-bootstrap-transfer-v1", &active.text)?
                        == active.digest,
                    "GPU bootstrap digest mismatch"
                );
                let value: Value = serde_json::from_str(&active.text)?;
                validate_depth(&value)?;
                let boundary: BootstrapBoundary = serde_json::from_value(value)?;
                boundary.validate()?;
                ensure!(boundary.epoch == epoch, "GPU bootstrap body epoch mismatch");
                self.abort();
                Ok(Some(boundary))
            }
        }
    }
}
