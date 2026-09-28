use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use drasi_lib::computation::v1::*;
use gpu_native::projections;
use serde_json::{json, Value};
use std::{collections::BTreeMap, num::NonZeroUsize, path::PathBuf};
use tokio::{fs, io::AsyncWriteExt};

pub const SINK: &str = "diagnostic-envelopes";
pub const DIRECTORY: &str = "/tmp/gpu-lab-trace";
pub const QUERIES: [&str; 3] = [
    "diagnostic-contexts",
    "diagnostic-workload-predicates",
    "diagnostic-placement-predicates",
];
const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub fn enabled() -> Result<bool> {
    match std::env::var("GPU_LAB_DIAGNOSTICS") {
        Err(std::env::VarError::NotPresent) => Ok(false),
        Ok(value) if value == "0" => Ok(false),
        Ok(value) if value == "1" => Ok(true),
        _ => anyhow::bail!("GPU_LAB_DIAGNOSTICS must be 0 or 1"),
    }
}

pub fn definitions() -> Result<Vec<(&'static str, String, QueryExecutionSettings)>> {
    let flags = "epoch_current, inputs_current, plan_applied, workload_matches, policy_current, report_current, assignment_matches";
    let workloads = projections::WORKLOADS
        .split_once("WITH w, a, r, e, epoch_current")
        .map(|(prefix, _)| prefix)
        .context("workload predicate prefix missing")?
        .replace("WITH w, a, r, e,\n", "WITH w, a, p, c, r, e, s,\n");
    let placements = projections::PLACEMENTS
        .split_once("WITH p, a, c, r, e, epoch_current")
        .map(|(prefix, _)| prefix)
        .context("placement predicate prefix missing")?
        .replace("WITH p, a, c, r, e,\n", "WITH p, a, c, r, e, s,\n");
    let operands = "toString(p.plan_version) AS saved_version, size(p.assignments) AS saved_count,
        a.application.applied_plan_version AS application_version, e.applied_plan_version AS execution_version,
        s.applied_plan_version AS sample_version, s.report_time_ms AS sample_time, e.acknowledged_at_ms AS acknowledged_time,
        a.observation_epoch AS applied_epoch, r.observation_epoch AS readiness_epoch, c.observation_epoch AS context_epoch,
        c.current AS scheduling_current,
        a.config_fingerprint AS applied_config, c.config_fingerprint AS context_config,
        r.scheduling_signature AS readiness_scheduling, c.scheduling_signature AS context_scheduling,
        r.policy_signature AS readiness_policy, c.policy_signature AS context_policy";
    Ok(vec![
        (QUERIES[0], "MATCH (a:AppliedPlan) OPTIONAL MATCH (c:SchedulingContext) OPTIONAL MATCH (r:DemoReadiness)
          RETURN a.source_ready AS source_ready, r.inputs_ready AS inputs_ready, c.current AS context_current,
          a.observation_epoch AS applied_epoch, c.observation_epoch AS context_epoch, r.observation_epoch AS readiness_epoch,
          a.config_fingerprint AS applied_config, c.config_fingerprint AS context_config,
          c.required_replicas AS required_replicas, a.application AS application,
          c.scheduling_signature AS context_scheduling, r.scheduling_signature AS readiness_scheduling,
          c.policy_signature AS context_policy, r.policy_signature AS readiness_policy".into(), QueryExecutionSettings::default()),
        (QUERIES[1], format!("{workloads} RETURN w.workload_id AS workload_id, e.id AS execution_id,
          a IS NOT NULL AS applied_present, r IS NOT NULL AS readiness_present,
          a.source_ready AS source_ready, r.inputs_ready AS inputs_ready, {flags}, {operands}"), projections::execution_settings()),
        (QUERIES[2], format!("{placements} RETURN e.id AS execution_id,
          a IS NOT NULL AS applied_present, c IS NOT NULL AS context_present, r IS NOT NULL AS readiness_present,
          a.source_ready AS source_ready, c.current AS context_current, r.inputs_ready AS inputs_ready, {flags}, {operands}"), projections::execution_settings()),
    ])
}

pub struct Recorder {
    descriptor: ComponentDescriptor,
    codec: EnvelopeCodec,
    directory: PathBuf,
    path: PathBuf,
    file: Option<fs::File>,
    bytes: u64,
    sequences: BTreeMap<String, u64>,
}
impl Recorder {
    pub fn new() -> Result<Self> {
        Self::in_directory(PathBuf::from(DIRECTORY))
    }
    fn in_directory(directory: PathBuf) -> Result<Self> {
        let mut codec = EnvelopeCodec::new(NonZeroUsize::new(8 * 1024 * 1024).unwrap());
        codec.register_schema(GraphChangeCodec::schema())?;
        codec.register_schema(QueryChangeCodec::schema())?;
        let descriptor = ComponentDescriptor::try_new(
            super::id(SINK)?,
            [
                ("graph", GraphChangeCodec::schema()),
                ("query", QueryChangeCodec::schema()),
            ]
            .into_iter()
            .map(|(name, schema)| {
                Ok(PortDescriptor::new(
                    PortId::try_new(name)?,
                    PortDirection::Input,
                    schema.descriptor().clone(),
                    PipeRequirements::default(),
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        )?;
        Ok(Self {
            descriptor,
            codec,
            path: directory.join(format!("{}.ndjson", uuid::Uuid::new_v4())),
            directory,
            file: None,
            bytes: 0,
            sequences: BTreeMap::new(),
        })
    }
}
#[async_trait]
impl ComputationComponent for Recorder {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<Value> {
        Ok(json!({"directory":DIRECTORY,"max_total_bytes":MAX_BYTES}))
    }
    async fn start(&mut self) -> Result<()> {
        ensure!(self.file.is_none(), "diagnostic recorder already started");
        fs::create_dir_all(&self.directory).await?;
        let mut files = fs::read_dir(&self.directory).await?;
        let mut count = 0;
        self.bytes = 0;
        while let Some(entry) = files.next_entry().await? {
            let metadata = entry.metadata().await?;
            ensure!(metadata.is_file(), "unexpected diagnostic directory entry");
            self.bytes = self
                .bytes
                .checked_add(metadata.len())
                .context("trace size overflow")?;
            count += 1;
        }
        ensure!(
            count < 32 && self.bytes < MAX_BYTES,
            "diagnostic storage limit reached"
        );
        self.path = self
            .directory
            .join(format!("{}.ndjson", uuid::Uuid::new_v4()));
        self.sequences.clear();
        self.file = Some(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.path)
                .await?,
        );
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush().await?;
        }
        Ok(())
    }
}
#[async_trait]
impl EnvelopeSink for Recorder {
    fn completion(&self) -> SinkCompletion {
        SinkCompletion::Handled
    }
    async fn handle(&mut self, input: InputEnvelope) -> Result<()> {
        let stream = input.envelope.system().stream().as_str().to_owned();
        let sequence = input.envelope.system().sequence();
        ensure!(
            self.sequences
                .get(&stream)
                .is_none_or(|previous| sequence > *previous),
            "diagnostic stream did not advance"
        );
        let frame: Value = serde_json::from_slice(&self.codec.encode(&input.envelope)?)?;
        let mut line = serde_json::to_vec(&json!({
            "observed_at_ms": chrono::Utc::now().timestamp_millis(),
            "port":input.port.as_str(), "frame":frame,
        }))?;
        line.push(b'\n');
        let bytes = self
            .bytes
            .checked_add(line.len() as u64)
            .context("trace size overflow")?;
        ensure!(bytes <= MAX_BYTES, "diagnostic storage limit reached");
        self.file
            .as_mut()
            .context("diagnostic recorder is stopped")?
            .write_all(&line)
            .await?;
        self.bytes = bytes;
        self.sequences.insert(stream, sequence);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use drasi_core::models::{ElementMetadata, ElementReference, SourceChange};

    #[tokio::test]
    async fn recorder_preserves_envelopes_and_rejects_repeated_sequences() -> Result<()> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native")
            .join(format!("diagnostic-test-{}", uuid::Uuid::new_v4()));
        let mut recorder = Recorder::in_directory(directory.clone())?;
        let envelope = GraphChangeCodec::encode_changes(
            &[SourceChange::Delete {
                metadata: ElementMetadata {
                    reference: ElementReference::new("source", "node"),
                    labels: vec!["Test".into()].into(),
                    effective_from: 1,
                },
            }],
            StreamId::try_new("test/out")?,
            3,
            None,
        )?;
        let input = InputEnvelope {
            port: PortId::try_new("graph")?,
            envelope: envelope.clone(),
        };
        assert!(recorder.handle(input.clone()).await.is_err());
        recorder.start().await?;
        recorder.handle(input.clone()).await?;
        assert!(recorder.handle(input.clone()).await.is_err());
        recorder.stop().await?;
        let first = recorder.path.clone();
        let recorded: Value = serde_json::from_slice(&fs::read(&first).await?)?;
        let decoded = recorder
            .codec
            .decode(&serde_json::to_vec(&recorded["frame"])?)?;
        assert_eq!(decoded.id(), envelope.id());
        assert_eq!(decoded.system(), envelope.system());
        assert_eq!(
            GraphChangeCodec::decode_changes(&decoded)?,
            GraphChangeCodec::decode_changes(&envelope)?
        );
        recorder.start().await?;
        recorder.handle(input).await?;
        recorder.stop().await?;
        fs::remove_file(first).await?;
        fs::remove_file(&recorder.path).await?;
        fs::remove_dir(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn recorder_rejects_storage_exhaustion_before_writing() -> Result<()> {
        let mut recorder = Recorder::new()?;
        recorder.bytes = MAX_BYTES;
        let envelope =
            GraphChangeCodec::encode_changes(&[], StreamId::try_new("test/out")?, 1, None)?;
        let error = recorder
            .handle(InputEnvelope {
                port: PortId::try_new("graph")?,
                envelope,
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("storage limit"), "{error}");
        Ok(())
    }
}
