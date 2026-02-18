# Drasi SSE CLI

A command-line tool that streams real-time query change events from a [Drasi Server](../../README.md) to your terminal. It dynamically creates a temporary SSE (Server-Sent Events) Reaction on the server, displays incoming events, and automatically cleans up when you exit.

## Purpose

When developing with Drasi, you often want to observe what a continuous query is producing in real time — seeing inserts, updates, and deletes as they happen in the underlying data source. The SSE CLI gives you a zero-setup way to do this from any terminal without writing code or building a UI.

Typical use cases:

- **Debugging queries** — verify that a continuous query produces the expected results when data changes
- **Monitoring** — watch a live stream of changes flowing through a query
- **Testing** — pipe the JSON output to other tools (`jq`, `grep`, test scripts) for automated validation
- **Logging** — capture a session's events to a file for later analysis

## Quick Start

```bash
# Build the tool
cd examples/sse-cli
cargo build --release

# Stream events from a query (Drasi Server must be running)
cargo run -- --server http://localhost:8080 --query all-messages
```

You'll see output like this:

```
Creating SSE reaction 'sse-cli-a3f1b2c4' for query 'all-messages'... done.
Streaming events (Ctrl-C to stop)...

{
  "queryId": "all-messages",
  "results": [
    {
      "op": "i",
      "data": {
        "Id": 5,
        "Sender": "Alice",
        "Message": "Hello from Alice"
      }
    }
  ],
  "timestamp": 1739378400000
}
```

Press **Ctrl-C** to stop. The tool automatically deletes the SSE Reaction it created:

```
^C
Shutting down...
Reaction 'sse-cli-a3f1b2c4' deleted.
```

## Command-Line Arguments

```
Usage: drasi-sse-cli [OPTIONS] --server <SERVER> --query <QUERY>

Options:
  -s, --server <SERVER>      Drasi Server base URL (e.g. http://localhost:8080)
  -q, --query <QUERY>        Query ID to subscribe to
  -d, --debug                Show heartbeat messages (debug mode)
  -l, --log-file <LOG_FILE>  Log received events to a file
  -p, --sse-port <SSE_PORT>  Port for the SSE Reaction HTTP server [default: 8090]
  -h, --help                 Print help
  -V, --version              Print version
```

### `--server` / `-s` (required)

The base URL of the Drasi Server. Can also be set via the `DRASI_SERVER_URL` environment variable.

```bash
# Using the flag
drasi-sse-cli --server http://localhost:8080 --query my-query

# Using the environment variable
export DRASI_SERVER_URL=http://localhost:8080
drasi-sse-cli --query my-query
```

### `--query` / `-q` (required)

The ID of the continuous query to subscribe to. The query must already exist on the Drasi Server. Can also be set via the `DRASI_QUERY` environment variable.

```bash
drasi-sse-cli -s http://localhost:8080 -q all-messages
```

### `--debug` / `-d`

Enables debug mode. In normal mode, only query result change events are displayed. In debug mode, heartbeat messages and raw SSE protocol lines are also shown:

```bash
drasi-sse-cli -s http://localhost:8080 -q all-messages --debug
```

Debug output appears on stderr so it doesn't interfere with piping the JSON data:

```
[heartbeat] ts=1739378415000
[heartbeat] ts=1739378445000
{
  "queryId": "all-messages",
  ...
}
[heartbeat] ts=1739378475000
```

### `--log-file` / `-l`

Path to a file where all query result events will be appended. Each event is written as pretty-printed JSON. The file is created if it doesn't exist. Heartbeats are not logged to the file.

```bash
drasi-sse-cli -s http://localhost:8080 -q all-messages --log-file events.json
```

### `--sse-port` / `-p`

The port on which the SSE Reaction's HTTP server will listen. Defaults to `8090`. Change this if port 8090 is already in use or if you want to run multiple instances of the tool simultaneously.

```bash
# Use a different port
drasi-sse-cli -s http://localhost:8080 -q all-messages --sse-port 9090

# Run two instances side by side
drasi-sse-cli -s http://localhost:8080 -q query-a --sse-port 8090 &
drasi-sse-cli -s http://localhost:8080 -q query-b --sse-port 8091 &
```

## Sample Output

### Insert Event

When a new row is inserted into the data source that matches the query:

```json
{
  "queryId": "all-messages",
  "results": [
    {
      "op": "i",
      "data": {
        "Id": 6,
        "Sender": "Bob",
        "Message": "Goodbye World"
      }
    }
  ],
  "timestamp": 1739378400000
}
```

### Update Event

When an existing row is updated:

```json
{
  "queryId": "all-messages",
  "results": [
    {
      "op": "u",
      "data": {
        "before": { "Id": 6, "Sender": "Bob", "Message": "Goodbye World" },
        "after": { "Id": 6, "Sender": "Bob", "Message": "Hello Again" }
      }
    }
  ],
  "timestamp": 1739378460000
}
```

### Delete Event

When a row is deleted:

```json
{
  "queryId": "all-messages",
  "results": [
    {
      "op": "d",
      "data": {
        "Id": 6,
        "Sender": "Bob",
        "Message": "Hello Again"
      }
    }
  ],
  "timestamp": 1739378520000
}
```

### Heartbeat (debug mode only)

```
[heartbeat] ts=1739378415000
```

## How It Works

The SSE CLI operates in four phases:

### 1. Create Reaction

On startup, the tool generates a unique reaction ID (e.g., `sse-cli-a3f1b2c4`) and sends a `POST /api/v1/reactions` request to the Drasi Server with the following configuration:

```json
{
  "kind": "sse",
  "id": "sse-cli-a3f1b2c4",
  "queries": ["all-messages"],
  "autoStart": true,
  "host": "0.0.0.0",
  "port": 8090,
  "ssePath": "/events"
}
```

This tells the Drasi Server to create an SSE Reaction that:
- Subscribes to the specified query
- Starts an HTTP server on the given port
- Serves an SSE event stream at the `/events` path

### 2. Connect to Stream

After a brief pause for the reaction's HTTP server to start, the tool opens a long-lived HTTP GET connection to `http://localhost:{sse-port}/events`. This is a standard SSE connection — the server holds it open and pushes `data:` frames as query results change.

### 3. Process Events

The tool reads the SSE stream line by line, parsing `data:` frames as JSON:

- **Query result events** (containing `queryId` and `results`) are pretty-printed to stdout and optionally appended to the log file.
- **Heartbeat events** (containing `"type": "heartbeat"`) are silently ignored unless `--debug` is enabled, in which case they are printed to stderr.
- **Raw SSE lines** (like `event:` or `id:` fields) are shown on stderr in debug mode only.

Status messages and diagnostics are always written to stderr, keeping stdout clean for JSON data. This means you can pipe the output:

```bash
drasi-sse-cli -s http://localhost:8080 -q all-messages | jq '.results[].data'
```

### 4. Cleanup on Exit

When the user presses **Ctrl-C**, the tool:

1. Cancels the SSE stream via `tokio::select!`
2. Sends a `DELETE /api/v1/reactions/{id}` request to the Drasi Server to remove the reaction
3. Exits

This ensures no orphaned reactions are left on the server. If the delete request fails (e.g., the server is unreachable), a warning is printed but the tool still exits.

## Design

The tool is a single-file Rust binary (~230 lines) with no external state. Key design decisions:

- **Standalone Cargo project** — has its own `Cargo.toml` independent of the main Drasi Server workspace, so it can be built and distributed separately.
- **Ephemeral reactions** — each run creates a fresh reaction with a UUID-based ID and deletes it on exit. No persistent server-side state.
- **stderr for status, stdout for data** — all operational messages (connection status, heartbeats, errors) go to stderr. Only query result JSON goes to stdout, making it safe to pipe.
- **Manual SSE parsing** — uses `reqwest`'s streaming response with line-by-line parsing rather than a dedicated SSE client library, keeping dependencies minimal.
- **Graceful shutdown** — `tokio::select!` races the SSE stream against `Ctrl-C`, ensuring cleanup always runs.
- **Environment variable fallback** — `--server` and `--query` can be set via `DRASI_SERVER_URL` and `DRASI_QUERY` environment variables, useful for scripting and CI.
