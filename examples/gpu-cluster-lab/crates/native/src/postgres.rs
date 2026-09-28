use async_trait::async_trait;
use drasi_core::{
    interface::{
        ElementIndex, MiddlewareError, MiddlewareSetupError, SourceMiddleware,
        SourceMiddlewareFactory,
    },
    models::{SourceChange, SourceMiddlewareConfig},
};
use std::{collections::BTreeMap, sync::Arc};

pub const POSTGRES_JSON: &str = "gpu-postgres-json";
pub const JSON_COLUMNS: &[(&str, &[&str])] = &[
    (
        "placement_policies",
        &[
            "allowed_regions",
            "allowed_purposes",
            "allowed_classifications",
        ],
    ),
    ("workload_requirements", &["allowed_gpu_models"]),
    ("gpu_placements", &["assignments", "decision_details"]),
];

pub struct PostgresJsonFactory;

struct PostgresJson {
    parsers: BTreeMap<&'static str, Vec<Arc<dyn SourceMiddleware>>>,
}

impl SourceMiddlewareFactory for PostgresJsonFactory {
    fn name(&self) -> String {
        POSTGRES_JSON.into()
    }

    fn create(
        &self,
        config: &SourceMiddlewareConfig,
    ) -> Result<Arc<dyn SourceMiddleware>, MiddlewareSetupError> {
        if !config.config.is_empty() {
            return Err(MiddlewareSetupError::InvalidConfiguration(
                "GPU PostgreSQL JSON decoding does not accept options".into(),
            ));
        }
        let factory = drasi_middleware::parse_json::ParseJsonFactory::new();
        let mut parsers = BTreeMap::new();
        for &(label, properties) in JSON_COLUMNS {
            let fields = properties
                .iter()
                .map(|property| {
                    factory.create(&SourceMiddlewareConfig {
                        name: format!("{POSTGRES_JSON}/{label}/{property}").into(),
                        kind: "parse_json".into(),
                        config: serde_json::json!({
                            "target_property": property, "on_error": "fail",
                            "max_json_size": 1_048_576, "max_nesting_depth": 20,
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            parsers.insert(label, fields);
        }
        Ok(Arc::new(PostgresJson { parsers }))
    }
}

#[async_trait]
impl SourceMiddleware for PostgresJson {
    async fn process(
        &self,
        source_change: SourceChange,
        index: &dyn ElementIndex,
    ) -> Result<Vec<SourceChange>, MiddlewareError> {
        let parsers = match &source_change {
            SourceChange::Insert { element } | SourceChange::Update { element } => element
                .get_metadata()
                .labels
                .iter()
                .find_map(|label| self.parsers.get(label.as_ref())),
            SourceChange::Delete { .. } | SourceChange::Future { .. } => None,
        };
        let Some(parsers) = parsers else {
            return Ok(vec![source_change]);
        };
        let mut changes = vec![source_change];
        for parser in parsers {
            let mut decoded = Vec::new();
            for change in changes {
                decoded.extend(parser.process(change, index).await?);
            }
            changes = decoded;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_core::{
        in_memory_index::in_memory_element_index::InMemoryElementIndex,
        models::{Element, ElementMetadata, ElementReference, ElementValue},
    };
    use serde_json::json;

    #[tokio::test]
    async fn every_published_json_column_is_decoded() -> anyhow::Result<()> {
        let middleware = PostgresJsonFactory.create(&SourceMiddlewareConfig {
            name: POSTGRES_JSON.into(),
            kind: POSTGRES_JSON.into(),
            config: Default::default(),
        })?;
        let index = InMemoryElementIndex::new();
        for &(label, fields) in JSON_COLUMNS {
            let properties = fields
                .iter()
                .map(|field| {
                    (
                        (*field).to_owned(),
                        json!(if *field == "decision_details" {
                            "{}"
                        } else {
                            "[\"synthetic\"]"
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            let decoded = middleware
                .process(
                    SourceChange::Update {
                        element: Element::Node {
                            metadata: ElementMetadata {
                                reference: ElementReference::new("postgres", label),
                                labels: vec![label.into()].into(),
                                effective_from: 1,
                            },
                            properties: serde_json::Value::Object(properties).into(),
                        },
                    },
                    &index,
                )
                .await?;
            let SourceChange::Update { element } = &decoded[0] else {
                panic!("update changed kind")
            };
            for field in fields {
                assert!(
                    matches!(
                        element.get_properties().get(field),
                        Some(ElementValue::List(_) | ElementValue::Object(_))
                    ),
                    "{label}.{field}"
                );
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn postgres_json_decoding_is_strict_and_preserves_versions_and_identity(
    ) -> anyhow::Result<()> {
        let middleware = PostgresJsonFactory.create(&SourceMiddlewareConfig {
            name: POSTGRES_JSON.into(),
            kind: POSTGRES_JSON.into(),
            config: Default::default(),
        })?;
        let index = InMemoryElementIndex::new();
        let metadata = ElementMetadata {
            reference: ElementReference::new("postgres", "gpu_placements:demo"),
            labels: vec!["gpu_placements".into()].into(),
            effective_from: 47,
        };
        let element = Element::Node {
            metadata: metadata.clone(),
            properties: json!({"plan_version":9_007_199_254_740_993_i64,
                "assignments":"[{\"replica_index\":0}]", "decision_details":"{\"scenario\":\"baseline\"}"}).into(),
        };
        for original in [
            SourceChange::Insert {
                element: element.clone(),
            },
            SourceChange::Update { element },
        ] {
            let decoded = middleware.process(original.clone(), &index).await?;
            assert_eq!(decoded.len(), 1);
            let (SourceChange::Insert { element } | SourceChange::Update { element }) = &decoded[0]
            else {
                panic!("operation kind changed")
            };
            assert_eq!(
                std::mem::discriminant(&decoded[0]),
                std::mem::discriminant(&original)
            );
            assert_eq!(element.get_metadata(), &metadata);
            assert_eq!(
                element.get_properties().get("plan_version"),
                Some(&ElementValue::Integer(9_007_199_254_740_993))
            );
            assert!(matches!(
                element.get_properties().get("assignments"),
                Some(ElementValue::List(_))
            ));
            assert!(matches!(
                element.get_properties().get("decision_details"),
                Some(ElementValue::Object(_))
            ));
        }
        let deleted = SourceChange::Delete {
            metadata: metadata.clone(),
        };
        assert_eq!(
            middleware.process(deleted.clone(), &index).await?,
            vec![deleted]
        );
        let invalid = SourceChange::Insert {
            element: Element::Node {
                metadata,
                properties: json!({"assignments":"not-json","decision_details":"{}"}).into(),
            },
        };
        assert!(middleware.process(invalid, &index).await.is_err());
        let unrelated = SourceChange::Insert {
            element: Element::Node {
                metadata: ElementMetadata {
                    reference: ElementReference::new("postgres", "gpu-1"),
                    labels: vec!["gpu_inventory".into()].into(),
                    effective_from: 48,
                },
                properties: json!({"gpu_id":"gpu-1"}).into(),
            },
        };
        assert_eq!(
            middleware.process(unrelated.clone(), &index).await?,
            vec![unrelated]
        );
        Ok(())
    }
}
