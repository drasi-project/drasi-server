// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! A loaded stateful transformer keeps its state in a `nativeProcessing`
//! resource over an index resource, declared in readable configuration.

use super::*;
use drasi_server::computation::{validate_definition, ComputationConfigInput};

const STATEFUL_MIDDLEWARE: &str = "drasi.standard/stateful-middleware";

/// counter → stateful middleware, whose state is in `processing` → capture.
fn readable(output: &Path, dependencies: Value, volatile: bool) -> Value {
    let stream = |name: &str| format!("{}/out", id(name));
    json!({"graph": {
        "resources": {
            "indexes": {"kind": "memoryIndexes"},
            "processing": {"kind": "nativeProcessing", "indexes": "indexes"}
        },
        "resourceDependencies": dependencies,
        "components": [
            {"id": id("counter"), "name": "counter", "kind": COUNTER, "autoStart": false,
             "config": {"stream": stream("counter"), "count": 4, "start": 2, "step": 3}},
            {"id": id("middleware"), "name": "middleware", "kind": STATEFUL_MIDDLEWARE,
             "autoStart": false, "resources": {"processing": "processing"},
             "config": {"stream": stream("middleware"), "volatile": volatile,
                "middleware": [{"name": "rename", "kind": "relabel",
                    "config": {"labelMappings": {"Counter": "Projected"}}}],
                "pipeline": ["rename"]}},
            {"id": id("capture"), "name": "capture", "kind": CAPTURE, "autoStart": false,
             "config": {"path": output}}
        ],
        "connections": [
            {"from": id("counter"), "to": id("middleware")},
            {"from": id("middleware"), "to": id("capture")}
        ]
    }})
}

fn resolve(value: Value, plugins: &PluginRegistry) -> Result<ComputationConfig> {
    let registry = drasi_server::computation::native_factory_registry(plugins)?;
    let config = serde_json::from_value::<ComputationConfigInput>(value)?
        .resolve(&registry)?
        .into_owned();
    validate_definition(&config)?;
    Ok(config)
}

async fn captured(path: &Path, count: usize) -> Result<Vec<SourceChange>> {
    tokio::time::timeout(DEADLINE, async {
        loop {
            match tokio::fs::read(path).await {
                Ok(bytes) if bytes.iter().filter(|byte| **byte == b'\n').count() == count => break,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let mut codec = EnvelopeCodec::new(NonZeroUsize::new(64 * 1024 * 1024).unwrap());
    codec.register_schema(GraphChangeCodec::schema())?;
    let mut changes = Vec::new();
    for line in tokio::fs::read(path)
        .await?
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        changes.extend(GraphChangeCodec::decode_changes(&codec.decode(line)?)?);
    }
    Ok(changes)
}

#[tokio::test]
async fn native_processing_keeps_a_loaded_stateful_transformers_state() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("capture.jsonl");
    let plugins = factories();
    // The resource names exactly the index resource that holds the state.
    for dependencies in [json!({}), json!({"processing": []})] {
        assert!(resolve(readable(&output, dependencies.clone(), true), &plugins).is_err());
    }
    let config = resolve(
        readable(&output, json!({"processing": ["indexes"]}), true),
        &plugins,
    )?;
    let core = core("native-processing").await?;
    register_components(&config, &core, &plugins).await?;
    let control = core.computation_control()?;
    let started = control
        .start_requested(
            control.desired_snapshot().revision,
            GraphSelection::Exact(
                ["counter", "middleware", "capture"]
                    .into_iter()
                    .map(id)
                    .collect(),
            ),
        )
        .await?;
    assert_eq!(started.summary, OperationSummary::Completed, "{started:?}");

    let mut values = Vec::new();
    for change in captured(&output, 4).await? {
        let SourceChange::Insert {
            element:
                Element::Node {
                    metadata,
                    properties,
                },
        } = change
        else {
            anyhow::bail!("expected inserted nodes, got {change:?}");
        };
        assert_eq!(metadata.labels.as_ref(), &[Arc::<str>::from("Projected")]);
        let Some(ElementValue::Integer(value)) = properties.get("value") else {
            anyhow::bail!("missing value");
        };
        values.push(*value);
    }
    assert_eq!(values, [2, 5, 8, 11]);

    // The middleware's current state comes from its storage, which the
    // memory index resource makes volatile.
    let state = core
        .output_catalogue()?
        .resolve(&OutputEndpoint::new(
            id("middleware"),
            PortId::try_new("out")?,
        ))?
        .snapshot()
        .await?;
    assert_eq!(state.records().len(), 4);
    assert!(!state.cursor().producer().persistent());
    core.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn volatile_storage_requires_a_volatile_transformer() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("capture.jsonl");
    let plugins = factories();
    let config = resolve(
        readable(&output, json!({"processing": ["indexes"]}), false),
        &plugins,
    )?;
    let core = core("native-processing-durability").await?;
    register_components(&config, &core, &plugins).await?;
    let control = core.computation_control()?;
    let started = control
        .start_requested(
            control.desired_snapshot().revision,
            GraphSelection::Exact(
                ["counter", "middleware", "capture"]
                    .into_iter()
                    .map(id)
                    .collect(),
            ),
        )
        .await?;
    assert_ne!(started.summary, OperationSummary::Completed);
    let failure = core
        .component_by_name(&ComponentName::try_new("middleware")?)?
        .observed()?
        .failure
        .context("a durable claim over volatile storage is refused")?;
    assert!(
        format!("{failure:?}").contains("does not match its actual persistent profile"),
        "{failure:?}"
    );
    core.shutdown().await?;
    Ok(())
}
