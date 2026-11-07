// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use drasi_lib::config::{ComponentId, ComponentReference};
use drasi_server::{api::models::ReactionConfig, factories::bind_reaction_inputs};
use serde_json::json;

const FIRST: &str = "b20c42ee-59a1-441f-a549-87a72be1f1e9";
const SECOND: &str = "e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481";

#[test]
fn dashboard_bindings_change_component_references_not_dashboard_or_business_ids() -> anyhow::Result<()> {
    let mut config: ReactionConfig = serde_json::from_value(json!({
        "kind": "dashboard", "name": "display", "queries": ["rooms", "floors"],
        "predefinedDashboards": [{
            "id": "rooms", "name": "Rooms",
            "widgets": [
                {"id": "floors", "type": "table", "config": {"queryId": "rooms", "businessId": "rooms"}},
                {"id": "count", "type": "kpi", "config": {"queryId": "floors"}}
            ]
        }]
    }))?;
    bind_reaction_inputs(&mut config, vec![ComponentId::try_new(FIRST)?, ComponentId::try_new(SECOND)?])?;
    let dashboard = &config.config["predefinedDashboards"][0];
    assert_eq!(dashboard["id"], "rooms");
    assert_eq!(dashboard["widgets"][0]["id"], "floors");
    assert_eq!(dashboard["widgets"][0]["config"]["queryId"], FIRST);
    assert_eq!(dashboard["widgets"][0]["config"]["businessId"], "rooms");
    assert_eq!(dashboard["widgets"][1]["config"]["queryId"], SECOND);
    assert_eq!(config.queries, [
        ComponentReference::by_id(ComponentId::try_new(FIRST)?),
        ComponentReference::by_id(ComponentId::try_new(SECOND)?),
    ]);
    Ok(())
}

#[test]
fn uuid_shaped_names_and_explicit_ids_can_bind_distinct_inputs_without_routes() -> anyhow::Result<()> {
    let mut config: ReactionConfig = serde_json::from_value(json!({
        "kind": "sse", "name": "events", "queries": [FIRST, {"id": FIRST}],
    }))?;
    bind_reaction_inputs(&mut config, vec![ComponentId::try_new(SECOND)?, ComponentId::try_new(FIRST)?])?;
    assert_eq!(config.resolved_query_ids()?, [SECOND, FIRST]);
    Ok(())
}

#[test]
fn ambiguous_dashboard_reference_is_an_error_not_an_arbitrary_binding() -> anyhow::Result<()> {
    let mut config: ReactionConfig = serde_json::from_value(json!({
        "kind": "dashboard", "name": "display", "queries": [FIRST, {"id": FIRST}],
        "predefinedDashboards": [{
            "id": "display",
            "widgets": [{"id": "widget", "config": {"queryId": FIRST}}]
        }]
    }))?;
    let error = bind_reaction_inputs(
        &mut config, vec![ComponentId::try_new(SECOND)?, ComponentId::try_new(FIRST)?],
    ).unwrap_err();
    assert!(error.to_string().contains("ambiguous"));
    Ok(())
}

#[test]
fn opaque_plugin_configuration_is_not_rewritten_by_matching_strings() -> anyhow::Result<()> {
    let value = json!({
        "routes": {"orders": {"value": "orders"}},
        "predefinedDashboards": [{"widgets": [{"config": {"queryId": "orders"}}]}],
    });
    let mut config = ReactionConfig {
        kind: "custom".into(), name: "custom".into(), id: None,
        queries: vec![ComponentReference::by_name("orders")?],
        auto_start: false, identity_provider: None, config: value.clone(),
    };
    bind_reaction_inputs(&mut config, vec![ComponentId::try_new(FIRST)?])?;
    assert_eq!(config.config, value);
    Ok(())
}
