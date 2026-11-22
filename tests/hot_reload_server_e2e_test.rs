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

//! Full `DrasiServer` hot-reload end-to-end test.
//!
//! Kept in its own test binary because `DrasiServer::new` installs the
//! process-wide host logger, which other hot-reload tests may already own.
//! Requires real cdylib plugins built by `make build-local-test-plugins`.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::Duration;

/// Locate the `target/debug/plugins` directory relative to the cargo manifest.
fn test_plugins_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("target/debug/plugins")
}

/// Return the path to the mock-source cdylib, or panic if absent.
fn mock_source_plugin_path() -> PathBuf {
    let dir = test_plugins_dir();
    let path = if cfg!(target_os = "macos") {
        dir.join("libdrasi_source_mock.dylib")
    } else if cfg!(target_os = "windows") {
        dir.join("drasi_source_mock.dll")
    } else {
        dir.join("libdrasi_source_mock.so")
    };
    assert!(
        path.exists(),
        "Mock source plugin not found at {}. Run `make build-local-test-plugins` first.",
        path.display()
    );
    path
}

// =============================================================================
// 10. Full DrasiServer E2E: hotReloadPlugins: true + REST API validation
// =============================================================================

/// Find a free TCP port by binding to port 0.
fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Start a real DrasiServer from a config file with hotReloadPlugins enabled,
/// drop a plugin into the plugins directory, and verify it appears in the
/// REST API plugin list and that its kinds become usable for creating components.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_server_hot_reload_e2e() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().join("plugins");
    std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");

    let port = find_free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    // Write a minimal config with hotReloadPlugins enabled
    let config_path = temp_dir.path().join("server.yaml");
    let config_content = format!(
        r#"
host: 127.0.0.1
port: {port}
logLevel: info
persistConfig: false
hotReloadPlugins: true
hotReloadDebounceMs: 200
verifyPlugins: false
instances:
  - id: hot-reload-e2e-test
    sources: []
    queries: []
    reactions: []
"#
    );
    std::fs::write(&config_path, &config_content).expect("write config");

    // Create the DrasiServer
    let server = drasi_server::DrasiServer::new(
        config_path.clone(),
        port,
        plugins_dir.clone(),
        false, // skip_verification
        false, // enable_ui
    )
    .await
    .expect("DrasiServer::new should succeed");

    // Spawn server.run() in a background task (it blocks on ctrl_c)
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("Server error: {e}");
        }
    });

    // Wait for the server to start listening
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("Server did not start within 10 seconds");
        }
        match client.get(format!("{base_url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => break,
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }

    // Verify no plugins are loaded initially
    let resp = client
        .get(format!("{base_url}/api/v1/plugins"))
        .send()
        .await
        .expect("GET /api/v1/plugins");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let initial_plugins = body["plugins"].as_array().unwrap();
    // Core/built-in plugins may be present; record count
    let initial_count = initial_plugins.len();

    // Copy the mock source plugin into the watched plugins directory
    let source_plugin_path = mock_source_plugin_path();
    let dest_path = plugins_dir.join(source_plugin_path.file_name().unwrap());
    std::fs::copy(&source_plugin_path, &dest_path).expect("copy plugin into plugins dir");

    // Poll until the plugin appears in the REST API
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut found_mock = false;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let resp = client
            .get(format!("{base_url}/api/v1/plugins"))
            .send()
            .await
            .expect("GET /api/v1/plugins");
        let body: serde_json::Value = resp.json().await.unwrap();
        let plugins = body["plugins"].as_array().unwrap();
        if plugins.len() > initial_count {
            // Find the newly loaded plugin
            for plugin in plugins {
                let id = plugin["id"].as_str().unwrap_or("");
                if id.contains("mock") {
                    found_mock = true;
                    // Verify it has the expected status
                    let status = plugin["status"].as_str().unwrap_or("");
                    assert!(
                        status == "Loaded" || status == "loaded",
                        "Hot-loaded plugin should have Loaded status, got: {status}"
                    );
                    // Verify it has kinds
                    let kinds = plugin["kinds"].as_array();
                    assert!(
                        kinds.is_some() && !kinds.unwrap().is_empty(),
                        "Hot-loaded plugin should have registered kinds"
                    );
                    break;
                }
            }
            if found_mock {
                break;
            }
        }
    }

    assert!(
        found_mock,
        "Mock source plugin should appear in /api/v1/plugins after being dropped into plugins dir"
    );

    // Verify we can create a source using the hot-loaded plugin's kind
    let resp = client
        .get(format!("{base_url}/api/v1/plugins"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let plugins = body["plugins"].as_array().unwrap();
    let mock_plugin = plugins
        .iter()
        .find(|p| p["id"].as_str().unwrap_or("").contains("mock"))
        .expect("mock plugin should be in list");
    let source_kind = mock_plugin["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["category"].as_str() == Some("Source"))
        .expect("mock plugin should have a Source kind");
    let kind_name = source_kind["kind"].as_str().unwrap();

    // Try to create a source using the hot-loaded plugin's kind
    let create_source_body = serde_json::json!({
        "kind": kind_name,
        "id": "hot-loaded-test-source",
        "autoStart": false
    });
    let resp = client
        .post(format!(
            "{base_url}/api/v1/instances/hot-reload-e2e-test/sources"
        ))
        .json(&create_source_body)
        .send()
        .await
        .expect("POST instance source");

    // The source should be created successfully (201) or accepted
    assert!(
        resp.status().is_success(),
        "Creating a source with hot-loaded plugin kind '{}' should succeed, got status: {}",
        kind_name,
        resp.status()
    );

    // Clean up: abort the server task
    server_handle.abort();
}
