// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use drasi_core::{
    in_memory_index::in_memory_element_index::InMemoryElementIndex,
    interface::ElementIndex,
    middleware::{MiddlewareContainer, SourceMiddlewarePipeline},
    models::{
        Element, ElementMetadata, ElementPropertyMap, ElementReference, ElementValue, SourceChange,
    },
};
use drasi_lib::{DrasiLib, QueryConfig};
use drasi_server::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use drasi_server::api::models::QueryConfigDto;
use serde_json::json;
use std::sync::Arc;

const WORKGRAPH_QUERY_YAML: &str = r#"
id: workgraph-events
query: "MATCH (n) RETURN n"
queryLanguage: Cypher
middleware:
  - name: extract-event
    kind: regex_extract
    config:
      target_property: body
      pattern: '(?s)^WorkGraphEvent/v1\s*\n```json\s*(?<payload>.*?)\s*```'
      capture_group: payload
      output_property: workgraph_event
      on_missing: passthrough
      on_no_match: passthrough
      on_error: fail
  - name: parse-event
    kind: parse_json
    config:
      target_property: workgraph_event
      on_missing: passthrough
      on_error: skip
  - name: derive-workgraph
    kind: jq
    config:
      preserve_input: true
      include_source_metadata: true
      reconcile: true
      mappings:
        WorkGraphEvent:
          insert:
            - elementType: Node
              label: '"WorkGraphEvent"'
              id: .id
              query: >-
                if (.workgraph_event | type) == "object" then .workgraph_event else empty end
          update:
            - elementType: Node
              label: '"WorkGraphEvent"'
              id: .id
              query: >-
                if (.workgraph_event | type) == "object" then .workgraph_event else empty end
"#;

fn workgraph_query_config() -> QueryConfig {
    let dto: QueryConfigDto = serde_yaml::from_str(WORKGRAPH_QUERY_YAML)
        .expect("WorkGraph middleware YAML should deserialize");
    QueryConfigMapper
        .map(&dto, &DtoMapper::new())
        .expect("WorkGraph middleware config should map")
}

async fn workgraph_pipeline(config: &QueryConfig) -> (DrasiLib, SourceMiddlewarePipeline) {
    let core = DrasiLib::builder()
        .build()
        .await
        .expect("DrasiLib should build");
    let registry = core.middleware_registry();
    let container = MiddlewareContainer::new(
        registry.as_ref(),
        config.middleware.iter().cloned().map(Arc::new).collect(),
    )
    .expect("WorkGraph middleware container should build");
    let pipeline = SourceMiddlewarePipeline::new(
        &container,
        vec![
            "extract-event".into(),
            "parse-event".into(),
            "derive-workgraph".into(),
        ],
    )
    .expect("WorkGraph middleware pipeline should build");
    (core, pipeline)
}

fn workgraph_change(operation: &str, body: &str) -> SourceChange {
    let element = Element::Node {
        metadata: ElementMetadata {
            reference: ElementReference::new("github", "comment:event"),
            labels: Arc::new([Arc::from("WorkGraphEvent")]),
            effective_from: 10,
        },
        properties: ElementPropertyMap::from(json!({ "body": body })),
    };
    match operation {
        "insert" => SourceChange::Insert { element },
        "update" => SourceChange::Update { element },
        _ => panic!("unsupported operation"),
    }
}

async fn index_outputs(index: &InMemoryElementIndex, changes: &[SourceChange]) {
    for change in changes {
        if let SourceChange::Insert { element } | SourceChange::Update { element } = change {
            index
                .set_element(element, &Vec::new())
                .await
                .expect("pipeline output should be indexed");
        }
    }
}

#[tokio::test]
async fn workgraph_middleware_config_maps_and_factories_are_registered() {
    let config = workgraph_query_config();

    assert_eq!(
        config.middleware[0].config["output_property"],
        "workgraph_event"
    );
    assert_eq!(
        config.middleware[1].config["target_property"],
        "workgraph_event"
    );
    assert_eq!(
        config.middleware[1].config["on_missing"], "passthrough",
        "parse_json must preserve changes without the extracted property"
    );
    assert_eq!(
        config.middleware[1].config["on_error"], "skip",
        "parse_json must preserve malformed edits for reconciliation"
    );
    assert!(
        !config.middleware[1].config.contains_key("output_property"),
        "parse_json must replace workgraph_event in place"
    );
    assert_eq!(config.middleware[2].config["preserve_input"], true);
    assert_eq!(config.middleware[2].config["include_source_metadata"], true);
    assert_eq!(config.middleware[2].config["reconcile"], true);
    assert_eq!(
        config.middleware[2].config["mappings"]["WorkGraphEvent"]["update"][0]["query"],
        r#"if (.workgraph_event | type) == "object" then .workgraph_event else empty end"#
    );

    let (core, _) = workgraph_pipeline(&config).await;
    let registry = core.middleware_registry();

    assert!(
        registry.get("promote").is_some(),
        "Promote factory should be registered"
    );
}

#[tokio::test]
async fn malformed_edit_reaches_jq_and_reconciles_previous_output() {
    let config = workgraph_query_config();
    let (_core, pipeline) = workgraph_pipeline(&config).await;
    let index = Arc::new(InMemoryElementIndex::new());

    let created = pipeline
        .process(
            workgraph_change(
                "insert",
                "WorkGraphEvent/v1\n```json\n{\"id\":\"work:1\",\"title\":\"Build\"}\n```",
            ),
            index.clone(),
        )
        .await
        .expect("valid event should produce a derived node and preserve its source");
    assert!(created.iter().any(|change| {
        matches!(
            change,
            SourceChange::Insert { element }
                if element.get_reference().element_id.as_ref() == "work:1"
        )
    }));
    index_outputs(index.as_ref(), &created).await;

    let malformed = pipeline
        .process(
            workgraph_change("update", "WorkGraphEvent/v1\n```json\n{\"id\":\n```"),
            index,
        )
        .await
        .expect("parse_json skip and jq type gate should handle malformed edits");

    assert!(malformed.iter().any(|change| {
        matches!(
            change,
            SourceChange::Delete { metadata }
                if metadata.reference.element_id.as_ref() == "work:1"
        )
    }));
    assert!(matches!(
        malformed.last(),
        Some(SourceChange::Update { element })
            if matches!(element.get_property("workgraph_event"), ElementValue::String(_))
    ));
}
