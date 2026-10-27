// Copyright 2026 The Drasi Authors.
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

//! Query `inputs` / `result` authoring: shorthand and expanded forms, strictness,
//! and the DTO to drasi-lib mapping.

use drasi_lib::config::{QueryInputConfig, QueryResultConfig};
use drasi_server::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use drasi_server::api::models::{QueryConfigDto, QueryInputConfigDto, QueryInputObjectDto};

const CHAINED_YAML: &str = r#"
id: floor-comfort
query: "MATCH (r:RoomComfort) RETURN r.FloorId AS FloorId, avg(r.ComfortLevel) AS ComfortLevel"
queryLanguage: Cypher
inputs:
  - room-comfort
  - component: facilities
    port: out
    nodes: [Floor]
    relations: [PART_OF_FLOOR]
    pipeline: [decode]
result:
  label: FloorComfort
"#;

fn map(dto: &QueryConfigDto) -> Result<drasi_lib::QueryConfig, String> {
    QueryConfigMapper
        .map(dto, &DtoMapper::new())
        .map_err(|error| error.to_string())
}

#[test]
fn shorthand_and_expanded_inputs_round_trip_through_yaml_and_json() {
    let dto: QueryConfigDto = serde_yaml::from_str(CHAINED_YAML).unwrap();
    assert_eq!(
        dto.inputs[0],
        QueryInputConfigDto::Component("room-comfort".into())
    );
    assert_eq!(
        dto.inputs[1],
        QueryInputConfigDto::Expanded(QueryInputObjectDto {
            component: "facilities".into(),
            port: Some("out".into()),
            nodes: vec!["Floor".into()],
            relations: vec!["PART_OF_FLOOR".into()],
            pipeline: vec!["decode".into()],
        })
    );
    assert_eq!(dto.result.label.as_deref(), Some("FloorComfort"));

    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["inputs"][0], "room-comfort");
    assert_eq!(json["inputs"][1]["component"], "facilities");
    assert_eq!(json["inputs"][1]["port"], "out");
    assert_eq!(json["result"]["label"], "FloorComfort");
    assert!(json.get("sources").is_none());
    assert_eq!(serde_json::from_value::<QueryConfigDto>(json).unwrap(), dto);

    let yaml = serde_yaml::to_string(&dto).unwrap();
    assert_eq!(serde_yaml::from_str::<QueryConfigDto>(&yaml).unwrap(), dto);
}

#[test]
fn mapping_preserves_ports_label_selectors_pipelines_and_result_label() {
    let dto: QueryConfigDto = serde_yaml::from_str(CHAINED_YAML).unwrap();
    let config = map(&dto).unwrap();
    assert_eq!(config.inputs[0], QueryInputConfig::new("room-comfort"));
    assert_eq!(
        config.inputs[1],
        QueryInputConfig {
            component: "facilities".into(),
            port: Some("out".into()),
            nodes: vec!["Floor".into()],
            relations: vec!["PART_OF_FLOOR".into()],
            pipeline: vec!["decode".into()],
        }
    );
    assert_eq!(config.result.label.as_deref(), Some("FloorComfort"));

    let round_trip = QueryConfigDto::try_from(config).unwrap();
    assert_eq!(round_trip.inputs, dto.inputs);
    assert_eq!(round_trip.result, dto.result);
}

#[test]
fn removed_and_unknown_fields_are_rejected() {
    for yaml in [
        "id: q\nquery: MATCH (n) RETURN n\nsources:\n  - sourceId: s\n",
        "id: q\nquery: MATCH (n) RETURN n\nsources: [s]\n",
        "id: q\nquery: MATCH (n) RETURN n\ninputs:\n  - sourceId: s\n",
        "id: q\nquery: MATCH (n) RETURN n\ninputs:\n  - component: s\n    extra: true\n",
        "id: q\nquery: MATCH (n) RETURN n\ninputs:\n  - port: out\n",
        "id: q\nquery: MATCH (n) RETURN n\ninputs: [1]\n",
        "id: q\nquery: MATCH (n) RETURN n\nresult:\n  label: A\n  colour: red\n",
    ] {
        assert!(
            serde_yaml::from_str::<QueryConfigDto>(yaml).is_err(),
            "{yaml} must be rejected"
        );
    }
}

#[test]
fn malformed_declarations_fail_before_creation() {
    let base = "id: q\nquery: MATCH (n) RETURN n\n";
    for (tail, reason) in [
        ("inputs: ['']\n", "empty component"),
        ("inputs:\n  - component: a\n    port: two words\n", "malformed port"),
        ("inputs: [a, a]\n", "duplicate input"),
        ("inputs: [q]\n", "self input"),
        ("inputs: [a]\nresult:\n  label: ''\n", "empty result label"),
    ] {
        let dto: QueryConfigDto = serde_yaml::from_str(&format!("{base}{tail}")).unwrap();
        assert!(map(&dto).is_err(), "{reason} must be rejected");
    }
}

#[test]
fn omitted_inputs_and_result_use_defaults() {
    let dto: QueryConfigDto = serde_yaml::from_str("id: q\nquery: MATCH (n) RETURN n\n").unwrap();
    assert!(dto.inputs.is_empty());
    assert_eq!(dto.result, Default::default());
    let config = map(&dto).unwrap();
    assert_eq!(config.result, QueryResultConfig::default());
    let json = serde_json::to_value(&dto).unwrap();
    assert!(json.get("result").is_none());
}
