use super::*;
use drasi_lib::management::{DesiredInstance, ManagementResourceResolver};
use drasi_server::computation::{validate_definition, ServerManagementResources};
use std::{collections::BTreeMap, num::NonZeroUsize};

#[tokio::test(flavor = "current_thread")]
async fn server_qos_recovery_recipes_roundtrip_restore_and_refuse_implicit_disable() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let resolver = ServerManagementResources::new(&PluginRegistry::new(), None)?;
    for kind in ["admission", "replay"] {
        let definition = QosChannelDefinition {
            stream: StreamId::try_new("source/out")?,
            capacity: NonZeroUsize::new(4).expect("capacity"),
            durable: true,
            retention: RetentionPolicy::Backpressure,
            subscribers: BTreeMap::from([("consumer".into(), SubscriptionStart::Earliest)]),
        };
        let resource = ResourceSpecification {
            id: ResourceId::try_new("channel")?,
            role: ResourceRole::StateStore,
            ownership: ResourceOwnership::Graph,
            binding: "channel".into(),
        };
        let recovery = if kind == "admission" {
            json!({"kind":kind,"component":"source",
                "failureScope":drasi_core::interface::FailureMode::ProcessRestart,
                "maxProducers":2,"receiptsPerProducer":4})
        } else {
            json!({"kind":kind,"failureScope":drasi_core::interface::FailureMode::ProcessRestart,
                "receiptCapacity":4})
        };
        let recipe = json!({"kind":"qos","definition":definition,
            "path":directory.path().join(kind),"recovery":recovery});
        let mut config = ComputationConfig {
            definition: DesiredInstance::default().topology,
        };
        config.definition.resources.push(resource.clone());
        config
            .definition
            .resource_configurations
            .insert(resource.id.clone(), recipe.clone());
        let roundtrip: ComputationConfig = serde_yaml::from_str(&serde_yaml::to_string(&config)?)?;
        assert_eq!(roundtrip.definition, config.definition);
        validate_definition(&roundtrip)?;
        let graph = &config.definition.graph_id;
        let handle = resolver.resolve("first", graph, &resource, &recipe).await?;
        let channel = handle.get::<QosChannel>()?;
        let journal = channel.output_journal_identity();
        let session = if kind == "admission" {
            Some(
                channel
                    .register_producer(ComponentId::try_new("client")?)
                    .await?,
            )
        } else {
            assert!(journal.is_some());
            None
        };
        channel.shutdown().await?;
        let handle = resolver.resolve("first", graph, &resource, &recipe).await?;
        let reopened = handle.get::<QosChannel>()?;
        assert_eq!(reopened.output_journal_identity(), journal);
        if let Some(session) = &session {
            assert_eq!(
                &reopened
                    .register_producer(ComponentId::try_new("client")?)
                    .await?,
                session
            );
        }
        reopened.shutdown().await?;
        let mut disabled = recipe.clone();
        disabled.as_object_mut().expect("recipe").remove("recovery");
        assert!(resolver
            .resolve("first", graph, &resource, &disabled)
            .await
            .is_err());
        let other = resolver
            .resolve("second", graph, &resource, &recipe)
            .await?;
        let other = other.get::<QosChannel>()?;
        if let Some(session) = session {
            assert!(other.admission_receipt(&session, 1).await.is_err());
        } else {
            assert_ne!(other.output_journal_identity(), journal);
        }
        other.shutdown().await?;
        let mut invalid = config.clone();
        invalid
            .definition
            .resource_configurations
            .get_mut(&resource.id)
            .expect("recipe")["definition"]["durable"] = json!(false);
        assert!(validate_definition(&invalid).is_err());
        invalid = config;
        invalid
            .definition
            .resource_configurations
            .get_mut(&resource.id)
            .expect("recipe")["recovery"]["graphId"] = json!("forged");
        assert!(validate_definition(&invalid).is_err());
    }
    Ok(())
}
