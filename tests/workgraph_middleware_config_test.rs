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

use drasi_lib::DrasiLib;
use drasi_server::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use drasi_server::api::models::QueryConfigDto;

#[tokio::test]
async fn workgraph_middleware_config_maps_and_factories_are_registered() {
    let yaml = r#"
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

    let dto: QueryConfigDto =
        serde_yaml::from_str(yaml).expect("WorkGraph middleware YAML should deserialize");
    let config = QueryConfigMapper
        .map(&dto, &DtoMapper::new())
        .expect("WorkGraph middleware config should map");

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

    let core = DrasiLib::builder()
        .build()
        .await
        .expect("DrasiLib should build");
    let registry = core.middleware_registry();

    for middleware in &config.middleware {
        registry
            .get(&middleware.kind)
            .unwrap_or_else(|| panic!("{} factory should be registered", middleware.kind))
            .create(middleware)
            .unwrap_or_else(|error| panic!("{} config should be valid: {error}", middleware.kind));
    }

    assert!(
        registry.get("promote").is_some(),
        "Promote factory should be registered"
    );
}
