use super::*;
use drasi_computation_contracts::{ChangeOperation, ChangeOrdering, GraphProducerProgress};
use drasi_computation_plugin_sdk::wire::Scope;
use serde_json::json;

#[tokio::test]
async fn derived_emitter_owns_progress_through_real_query_chain() -> Result<()> {
    let scope = Scope {
        instance_id: "gpu-test-instance".into(),
        graph_id: "gpu-cluster-test".into(),
        generation: 1,
    };
    let mut previous_epoch = None;
    for _ in 0..2 {
        let mut source = Emitter::new(
            StreamId::try_new("input/out")?,
            ComponentId::try_new("input-fixture")?,
            Some(&scope),
        )?;
        let mut emitter = Emitter::new(
            StreamId::try_new("derived/out")?,
            ComponentId::try_new("processor-worker")?,
            Some(&scope),
        )?;
        let mut upstream = query(
            "producer-input",
            "MATCH (n:Input) RETURN n.value AS value, n.optional AS optional",
        )
        .await?;
        let mut downstream = query(
            "producer-output",
            "MATCH (n:Derived) RETURN n.value AS value, n.optional AS optional",
        )
        .await?;
        let mut identity = None;
        let mut sequence = 0;
        for value in [
            Some(json!({"value": 1, "optional": true})),
            Some(json!({"value": 2})),
            Some(json!({"value": 3, "optional": true})),
            Some(json!({"value": 4})),
            None,
        ] {
            let change = match &value {
                Some(value) => source.record("Input", "one", value)?,
                None => source.remove("Input", "one")?,
            }
            .context("input change")?;
            let input = source.emit(vec![change], None)?.remove(0);
            let mut query_outputs = upstream
                .transform(InputEnvelope {
                    port: PortId::try_new("in")?,
                    envelope: input.envelope,
                })
                .await?;
            upstream.delivery_completed(&query_outputs).await?;
            while upstream.has_pending_emissions() {
                let more = upstream.continue_transform().await?;
                upstream.delivery_completed(&more).await?;
                query_outputs.extend(more);
            }
            let mut derived_outputs = Vec::new();
            for input in query_outputs {
                let mut changes = Vec::new();
                for operation in input.envelope.changes().operations() {
                    changes.extend(match operation {
                        ChangeOperation::Added { after, .. }
                        | ChangeOperation::Updated { after, .. } => {
                            let row = QueryChangeCodec::decode_row(after)?;
                            let mut value = QueryChangeCodec::row_values_to_json(&row.values);
                            if value["optional"].is_null() {
                                value.as_object_mut().unwrap().remove("optional");
                            }
                            emitter.record("Derived", "one", &value)?
                        }
                        ChangeOperation::Deleted { .. } => emitter.remove("Derived", "one")?,
                    });
                }
                for change in &mut changes {
                    let metadata = match change {
                        SourceChange::Insert { element } | SourceChange::Replace { element } => {
                            match element {
                                drasi_core::models::Element::Node { metadata, .. } => metadata,
                                _ => unreachable!(),
                            }
                        }
                        SourceChange::Delete { metadata } => metadata,
                        _ => unreachable!(),
                    };
                    metadata.effective_from = 100 - sequence;
                }
                if value.as_ref().is_some_and(|value| value["value"] == 4) {
                    for change in &mut changes {
                        let SourceChange::Replace {
                            element:
                                drasi_core::models::Element::Node {
                                    metadata,
                                    properties,
                                },
                        } = change
                        else {
                            anyhow::bail!("expected replacement before constructing sparse patch");
                        };
                        let mut patch = drasi_core::models::ElementPropertyMap::new();
                        patch.insert("value", properties.get("value").unwrap().clone());
                        *change = SourceChange::Update {
                            element: drasi_core::models::Element::Node {
                                metadata: metadata.clone(),
                                properties: patch,
                            },
                            removed_properties: ["optional".to_string()].into(),
                        };
                    }
                }
                let outputs = emitter.emit(changes.clone(), Some(&input.envelope))?;
                for output in &outputs {
                    sequence += 1;
                    assert_eq!(
                        output.envelope.system().ordering(),
                        ChangeOrdering::ProducerSequence
                    );
                    assert_eq!(
                        output.envelope.system().timestamp(),
                        input.envelope.system().timestamp()
                    );
                    assert_eq!(GraphChangeCodec::decode_changes(&output.envelope)?, changes);
                    let progress = GraphProducerProgress::from_envelope(&output.envelope)?
                        .context("immediate emitter progress")?;
                    let current = progress.identity();
                    assert_eq!(current.component_id().as_str(), "processor-worker");
                    assert_eq!(current.stream().as_str(), "derived/out");
                    assert_eq!(current.construction_scope(), "gpu-test-instance");
                    assert_eq!(current.graph_id(), "gpu-cluster-test");
                    assert!(!current.persistent());
                    if let Some(identity) = &identity {
                        assert_eq!(current, identity);
                    } else {
                        assert_ne!(Some(current.incarnation()), previous_epoch);
                        identity = Some(current.clone());
                    }
                    assert_eq!(progress.sequence(), sequence);
                }
                derived_outputs.extend(outputs);
            }
            assert!(!derived_outputs.is_empty());
            let rows = project(&mut downstream, &derived_outputs).await?;
            if let Some(value) = value {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0]["value"], value["value"]);
                assert_eq!(rows[0]["optional"], value["optional"]);
            } else {
                assert!(rows.is_empty());
            }
        }
        assert_eq!(sequence, 5);
        previous_epoch = identity.map(|identity| identity.incarnation());
        upstream.stop().await?;
        downstream.stop().await?;
    }
    Ok(())
}

#[tokio::test]
async fn native_status_factory_uses_actual_scope_and_component_identity() -> Result<()> {
    let hub = Arc::new(crate::status::Hub::default());
    let factory = AuxiliaryFactory {
        reaction: false,
        hub: hub.clone(),
    };
    let request = CreateRequest {
        id: ComponentId::try_new("status-worker")?,
        implementation: factory.metadata().implementation,
        configuration_version: 1,
        configuration: json!({"stream": "observations/out"}),
        scope: Some(Scope {
            instance_id: "native-instance".into(),
            graph_id: "native-graph".into(),
            generation: 1,
        }),
        inputs: Vec::new(),
        activation_credentials_bound: false,
    };
    let mut previous_epoch = None;
    for _ in 0..2 {
        let Component::Source(mut source) = factory
            .create(&request, ControlSender::default())?
            .component
        else {
            anyhow::bail!("expected status source");
        };
        source.start().await?;
        let initial = source.next().await?.context("initial status")?;
        let first = GraphProducerProgress::from_envelope(&initial.envelope)?
            .context("status output progress")?;
        assert_eq!(first.identity().component_id(), &request.id);
        assert_eq!(first.identity().stream().as_str(), "observations/out");
        assert_eq!(first.identity().construction_scope(), "native-instance");
        assert_eq!(first.identity().graph_id(), "native-graph");
        assert!(!first.identity().persistent());
        assert_ne!(Some(first.identity().incarnation()), previous_epoch);
        assert_eq!(first.sequence(), 1);
        hub.publish("status-worker", "updated", None, None)?;
        let next = source.next().await?.context("updated status")?;
        let second = GraphProducerProgress::from_envelope(&next.envelope)?
            .context("updated status progress")?;
        assert_eq!(second.identity(), first.identity());
        assert_eq!(second.sequence(), 2);
        previous_epoch = Some(first.identity().incarnation());
        source.stop().await?;
    }
    Ok(())
}
