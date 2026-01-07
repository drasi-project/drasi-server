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

//! Tests for Redis helper utilities
//!
//! These tests validate the helper functions used for Redis-based platform integration tests.

mod test_support;

use serde_json::json;
use std::collections::HashMap;
use test_support::redis_helpers::*;

#[tokio::test]
async fn test_setup_redis() {
    let redis = setup_redis().await;
    assert!(redis.url().starts_with("redis://127.0.0.1:"));

    // Verify we can connect
    let client = redis::Client::open(redis.url()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    let _: String = redis::cmd("PING").query_async(&mut conn).await.unwrap();

    // Explicitly cleanup the container
    redis.cleanup().await;
}

#[test]
fn test_build_platform_insert_event_node() {
    let mut props = HashMap::new();
    props.insert("name".to_string(), json!("Alice"));
    props.insert("age".to_string(), json!(30));

    let event = build_platform_insert_event("1", vec!["Person"], props, "node", None, None);

    assert_eq!(event["data"][0]["op"], "i");
    assert_eq!(event["data"][0]["payload"]["after"]["id"], "1");
    assert_eq!(event["data"][0]["payload"]["after"]["labels"][0], "Person");
    assert_eq!(
        event["data"][0]["payload"]["after"]["properties"]["name"],
        "Alice"
    );
    assert_eq!(event["data"][0]["payload"]["source"]["table"], "node");
}

#[test]
fn test_build_platform_insert_event_relation() {
    let mut props = HashMap::new();
    props.insert("since".to_string(), json!("2020"));

    let event =
        build_platform_insert_event("r1", vec!["KNOWS"], props, "rel", Some("1"), Some("2"));

    assert_eq!(event["data"][0]["op"], "i");
    assert_eq!(event["data"][0]["payload"]["after"]["startId"], "1");
    assert_eq!(event["data"][0]["payload"]["after"]["endId"], "2");
}

#[test]
fn test_build_platform_update_event() {
    let mut old_props = HashMap::new();
    old_props.insert("value".to_string(), json!(10));

    let mut new_props = HashMap::new();
    new_props.insert("value".to_string(), json!(20));

    let event = build_platform_update_event(
        "1",
        vec!["Counter"],
        old_props,
        new_props,
        "node",
        None,
        None,
    );

    assert_eq!(event["data"][0]["op"], "u");
    assert_eq!(
        event["data"][0]["payload"]["before"]["properties"]["value"],
        10
    );
    assert_eq!(
        event["data"][0]["payload"]["after"]["properties"]["value"],
        20
    );
}

#[test]
fn test_build_platform_delete_event() {
    let mut props = HashMap::new();
    props.insert("name".to_string(), json!("Bob"));

    let event = build_platform_delete_event("2", vec!["Person"], props, "node", None, None);

    assert_eq!(event["data"][0]["op"], "d");
    assert_eq!(event["data"][0]["payload"]["before"]["id"], "2");
    assert!(event["data"][0]["payload"]["after"].is_null());
}

#[test]
fn test_verify_cloudevent_structure_valid() {
    let cloud_event = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "source": "drasi-core",
        "specversion": "1.0",
        "type": "com.dapr.event.sent",
        "datacontenttype": "application/json",
        "topic": "test-results",
        "time": "2025-01-01T00:00:00.000Z",
        "data": {},
        "pubsubname": "drasi-pubsub",
    });

    assert!(verify_cloudevent_structure(&cloud_event).is_ok());
}

#[test]
fn test_verify_cloudevent_structure_missing_field() {
    let cloud_event = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "source": "drasi-core",
        // Missing specversion
        "type": "com.dapr.event.sent",
        "datacontenttype": "application/json",
        "topic": "test-results",
        "time": "2025-01-01T00:00:00.000Z",
        "data": {},
    });

    assert!(verify_cloudevent_structure(&cloud_event).is_err());
}

#[test]
fn test_verify_cloudevent_structure_invalid_uuid() {
    let cloud_event = json!({
        "id": "not-a-uuid",
        "source": "drasi-core",
        "specversion": "1.0",
        "type": "com.dapr.event.sent",
        "datacontenttype": "application/json",
        "topic": "test-results",
        "time": "2025-01-01T00:00:00.000Z",
        "data": {},
    });

    assert!(verify_cloudevent_structure(&cloud_event).is_err());
}
