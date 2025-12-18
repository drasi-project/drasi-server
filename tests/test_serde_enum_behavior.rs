// Testing serde rename_all behavior with internally tagged enums

use serde::{Deserialize, Serialize};

// Test 1: Basic internally tagged enum with rename_all
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ConfigA {
    #[serde(rename = "mock")]
    Mock {
        some_field: String,
        another_field: i32,
    },
}

// Test 2: Internally tagged enum with rename_all and per-variant rename_all
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum ConfigB {
    #[serde(rename = "mock", rename_all = "camelCase")]
    Mock {
        some_field: String,
        another_field: i32,
    },
}

#[test]
fn test_config_a() {
    let val = ConfigA::Mock {
        some_field: "test".to_string(),
        another_field: 42,
    };
    let json = serde_json::to_value(&val).unwrap();
    println!("\nConfigA (enum-level rename_all):");
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    println!("some_field exists: {}", json.get("some_field").is_some());
    println!("someField exists: {}", json.get("someField").is_some());
}

#[test]
fn test_config_b() {
    let val = ConfigB::Mock {
        some_field: "test".to_string(),
        another_field: 42,
    };
    let json = serde_json::to_value(&val).unwrap();
    println!("\nConfigB (variant-level rename_all):");
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    println!("some_field exists: {}", json.get("some_field").is_some());
    println!("someField exists: {}", json.get("someField").is_some());
}
