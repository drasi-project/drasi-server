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

//! Drasi SSE CLI — stream query change events from a Drasi Server.
//!
//! Creates an SSE Reaction on the server, consumes the event stream, and
//! cleans up the reaction on exit.

use clap::Parser;
use futures_util::StreamExt;
use std::io::Write;
use std::path::PathBuf;

/// Stream SSE events from a Drasi Server query.
///
/// Creates a temporary SSE Reaction subscribed to the specified query,
/// streams events to stdout, and deletes the reaction on Ctrl-C.
#[derive(Parser, Debug)]
#[command(name = "drasi-sse-cli", version, about, long_about = None)]
struct Args {
    /// Drasi Server base URL (e.g. http://localhost:8080)
    #[arg(short, long, env = "DRASI_SERVER_URL")]
    server: String,

    /// Query ID to subscribe to
    #[arg(short, long, env = "DRASI_QUERY")]
    query: String,

    /// Show heartbeat messages (debug mode)
    #[arg(short, long, default_value_t = false)]
    debug: bool,

    /// Log received events to a file
    #[arg(short, long)]
    log_file: Option<PathBuf>,

    /// Port for the SSE Reaction HTTP server
    #[arg(short = 'p', long, default_value_t = 8090)]
    sse_port: u16,
}

/// Create the SSE Reaction on the Drasi Server. Returns the reaction ID.
async fn create_reaction(
    client: &reqwest::Client,
    server: &str,
    reaction_id: &str,
    query_id: &str,
    sse_port: u16,
) -> Result<(), String> {
    let url = format!("{server}/api/v1/reactions");
    let body = serde_json::json!({
        "kind": "sse",
        "id": reaction_id,
        "queries": [query_id],
        "autoStart": true,
        "host": "0.0.0.0",
        "port": sse_port,
        "ssePath": "/events"
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to contact Drasi Server: {e}"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Failed to create reaction ({status}): {text}"))
    }
}

/// Delete the SSE Reaction from the Drasi Server.
async fn delete_reaction(client: &reqwest::Client, server: &str, reaction_id: &str) {
    let url = format!("{server}/api/v1/reactions/{reaction_id}");
    match client.delete(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            eprintln!("Reaction '{reaction_id}' deleted.");
        }
        Ok(resp) => {
            let status = resp.status();
            eprintln!("Warning: failed to delete reaction ({status})");
        }
        Err(e) => {
            eprintln!("Warning: failed to delete reaction: {e}");
        }
    }
}

/// Process one SSE data line. Returns true if something was printed.
fn handle_event(data: &str, debug: bool, log_file: &mut Option<std::fs::File>) -> bool {
    // Try to parse as JSON to detect heartbeats
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
        let is_heartbeat = value.get("type").and_then(|v| v.as_str()) == Some("heartbeat");

        if is_heartbeat {
            if debug {
                let ts = value.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
                eprintln!("[heartbeat] ts={ts}");
            }
            return false;
        }

        // Query update — pretty-print
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| data.to_string());
        println!("{pretty}");

        if let Some(f) = log_file.as_mut() {
            let _ = writeln!(f, "{pretty}");
        }
        return true;
    }

    // Not JSON — print raw
    if debug {
        eprintln!("[raw] {data}");
    }
    false
}

/// Connect to the SSE endpoint and stream events until cancelled.
async fn stream_events(
    client: &reqwest::Client,
    sse_url: &str,
    debug: bool,
    mut log_file: Option<std::fs::File>,
) -> Result<(), String> {
    let resp = client
        .get(sse_url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to SSE endpoint: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("SSE endpoint returned {status}"));
    }

    eprintln!("Connected to SSE stream at {sse_url}");

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {e}"))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete lines
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer = buffer[pos + 1..].to_string();

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if !data.is_empty() {
                    handle_event(data, debug, &mut log_file);
                }
            } else if debug && !line.is_empty() && !line.starts_with(':') {
                eprintln!("[sse] {line}");
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let server = args.server.trim_end_matches('/').to_string();
    let reaction_id = format!("sse-cli-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let sse_url = format!("http://localhost:{}/events", args.sse_port);

    let client = reqwest::Client::new();

    // Create the SSE Reaction
    eprint!("Creating SSE reaction '{reaction_id}' for query '{}'... ", args.query);
    if let Err(e) = create_reaction(&client, &server, &reaction_id, &args.query, args.sse_port).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
    eprintln!("done.");
    eprintln!("Streaming events (Ctrl-C to stop)...\n");

    // Open log file if requested
    let log_file = args.log_file.as_ref().map(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|e| {
                eprintln!("Warning: could not open log file: {e}");
                std::process::exit(1);
            })
    });

    // Wait briefly for the SSE reaction server to start
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Stream events until Ctrl-C
    tokio::select! {
        result = stream_events(&client, &sse_url, args.debug, log_file) => {
            if let Err(e) = result {
                eprintln!("\nStream ended: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nShutting down...");
        }
    }

    // Cleanup
    delete_reaction(&client, &server, &reaction_id).await;
}
