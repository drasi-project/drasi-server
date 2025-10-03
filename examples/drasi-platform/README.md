# Drasi Platform Example - Redis Stream Integration

This example demonstrates how to use the **platform source** in Drasi Server to consume events from Redis Streams. The platform source is specifically designed to bridge external Drasi Platform infrastructure with drasi-server-core queries via Redis.

## Overview

This example shows:
- How to configure a platform source to connect to Redis Streams
- Real-time event processing without bootstrap data
- Continuous Cypher query filtering
- Log-based output of query results

The example consumes messages from a Redis stream, filters them using a Cypher query, and outputs matching results to the console.

## Architecture

```
┌─────────────┐       ┌──────────────────┐       ┌─────────────┐       ┌──────────────┐
│   Redis     │       │  Platform Source │       │    Query    │       │     Log      │
│   Stream    │──────▶│  (Redis Client)  │──────▶│  (Cypher)   │──────▶│   Reaction   │
│             │       │                  │       │             │       │              │
│ hello-world │       │  Consumes from   │       │  Filters    │       │  Outputs to  │
│  -change    │       │  stream with     │       │  "Hello     │       │  console     │
│             │       │  consumer group  │       │  World"     │       │              │
└─────────────┘       └──────────────────┘       └─────────────┘       └──────────────┘
```

### Data Flow

1. **Redis Stream**: Events are published to the `hello-world-change` stream
2. **Platform Source**: Consumes events using Redis consumer groups
3. **Query**: Filters nodes with label `Message` and property `Message: "Hello World"`
4. **Reaction**: Logs matching results with `MessageId` and `From` fields

## Prerequisites

Before running this example, ensure you have:

1. **Rust and Cargo**: Required to build the Drasi server
   - Install from: https://rustup.rs/

2. **Docker**: Used to run Redis
   - macOS: https://docs.docker.com/desktop/install/mac-install/
   - Linux: https://docs.docker.com/engine/install/
   - Windows: https://docs.docker.com/desktop/install/windows-install/

3. **redis-cli** (optional but recommended): Used to publish events
   - macOS: `brew install redis`
   - Ubuntu/Debian: `sudo apt-get install redis-tools`
   - RHEL/CentOS: `sudo yum install redis`
   - Alternative: Use Docker exec commands

4. **jq** (optional): For formatted JSON output
   - macOS: `brew install jq`
   - Ubuntu/Debian: `sudo apt-get install jq`

## Quick Start

Follow these steps to run the example:

### 1. Start Redis

```bash
cd examples/drasi-platform/scripts
./setup-redis.sh
```

This will start a Redis container named `drasi-redis` on port 6379.

### 2. Start the Drasi Server

In a new terminal:

```bash
cd examples/drasi-platform/scripts
./start-server.sh
```

This will:
- Build the Drasi server in release mode
- Start the server with the platform example configuration
- Connect to Redis and begin listening for events

### 3. Publish Test Events

In another terminal:

```bash
cd examples/drasi-platform/scripts
./publish-event.sh Alice
```

You should see the event logged in the Drasi server terminal, showing:
```
MessageId: msg-1234567890
MessageFrom: Alice
```

### 4. View Query Results via API

```bash
cd examples/drasi-platform/scripts
./view-results.sh
```

This queries the Drasi REST API to retrieve current query results.

### 5. Cleanup

When finished:

```bash
cd examples/drasi-platform/scripts
./cleanup.sh
```

This stops the Drasi server and removes the Redis container.

## How It Works

### Platform Source Configuration

The platform source is configured in `server-config.yaml`:

```yaml
sources:
  - id: platform-redis-source
    source_type: platform
    auto_start: true
    properties:
      redis_url: "redis://localhost:6379"
      stream_key: "hello-world-change"
      consumer_group: "drasi-core"
      consumer_name: "consumer-1"
      batch_size: 10
      block_ms: 5000
      start_id: ">"
```

**Key Properties:**
- `redis_url`: Connection URL for Redis
- `stream_key`: The Redis stream to consume from
- `consumer_group`: Redis consumer group name
- `consumer_name`: Unique consumer identifier
- `batch_size`: Number of events to read per batch (default: 10)
- `block_ms`: Milliseconds to block waiting for events (default: 5000)
- `start_id`: Stream position - `">"` for new events, `"0"` to replay all

### Query

The Cypher query filters messages:

```cypher
MATCH
  (m:Message {Message: 'Hello World'})
RETURN
  m.MessageId AS MessageId,
  m.From AS MessageFrom
```

This query:
- Matches nodes with label `Message`
- Filters for property `Message` equal to `'Hello World'`
- Returns `MessageId` and `From` properties

### Reaction

The log reaction outputs results to the console:

```yaml
reactions:
  - id: log-hello-world
    reaction_type: log
    auto_start: true
    queries:
      - hello-world-from
```

## Platform Event Format

The platform source expects events in this specific JSON structure:

```json
{
  "type": "i",
  "elementType": "node",
  "time": {"ms": 1234567890},
  "after": {
    "id": "msg-001",
    "labels": ["Message"],
    "properties": {
      "MessageId": "msg-001",
      "Message": "Hello World",
      "From": "Alice"
    }
  }
}
```

**Field Descriptions:**
- `type`: Operation type - `"i"` (insert), `"u"` (update), `"d"` (delete)
- `elementType`: Element type - `"node"` or `"rel"` (relationship)
- `time`: Timestamp object with `ms` field (milliseconds since epoch)
- `after`: Element data for inserts/updates (use `before` for deletes)
  - `id`: Unique element identifier
  - `labels`: Array of labels for nodes
  - `properties`: Key-value map of properties

## Testing

### Test Matching Events

Publish an event that matches the query:

```bash
./scripts/publish-event.sh Alice
./scripts/publish-event.sh Bob
./scripts/publish-event.sh Charlie
```

All these events should appear in the Drasi server logs.

### Test Non-Matching Events

To test that filtering works, manually publish a non-matching event:

```bash
docker exec -it drasi-redis redis-cli XADD hello-world-change '*' event '{
  "type": "i",
  "elementType": "node",
  "time": {"ms": 1234567890},
  "after": {
    "id": "msg-002",
    "labels": ["Message"],
    "properties": {
      "MessageId": "msg-002",
      "Message": "Goodbye",
      "From": "David"
    }
  }
}'
```

This event should NOT appear in the logs because `Message` is not `"Hello World"`.

### View Current Results

Query the API to see accumulated results:

```bash
./scripts/view-results.sh
```

Or use curl directly:

```bash
curl http://localhost:8080/api/queries/hello-world-from/results | jq
```

## API Reference

The Drasi server exposes a REST API on port 8080:

### Health Check
```bash
GET http://localhost:8080/health
```

### Query Results
```bash
GET http://localhost:8080/api/queries/hello-world-from/results
```

### OpenAPI Documentation
```bash
GET http://localhost:8080/openapi.json
GET http://localhost:8080/swagger-ui/
```

### Component Management
```bash
# Get source status
GET http://localhost:8080/api/sources/platform-redis-source

# Get query status
GET http://localhost:8080/api/queries/hello-world-from

# Get reaction status
GET http://localhost:8080/api/reactions/log-hello-world

# Stop a component
POST http://localhost:8080/api/sources/platform-redis-source/stop

# Start a component
POST http://localhost:8080/api/sources/platform-redis-source/start
```

See `requests.http` for more examples using VSCode REST Client.

## Troubleshooting

### Redis Connection Failed

**Error**: `Cannot connect to Redis at localhost:6379`

**Solution**:
1. Ensure Redis is running: `docker ps | grep drasi-redis`
2. If not running: `./scripts/setup-redis.sh`
3. Check Redis logs: `docker logs drasi-redis`

### No Events Appearing

**Problem**: Events are published but don't appear in logs

**Checklist**:
1. Verify Redis stream exists and has data:
   ```bash
   docker exec -it drasi-redis redis-cli XLEN hello-world-change
   ```
2. Check Drasi server logs for errors
3. Verify event format matches platform expectations
4. Ensure `Message` property equals `"Hello World"` (case-sensitive)
5. Confirm source, query, and reaction are all running:
   ```bash
   curl http://localhost:8080/api/sources/platform-redis-source
   ```

### Consumer Group Issues

**Error**: `BUSYGROUP Consumer Group name already exists`

**Solution**: This is normal. The platform source will use the existing consumer group.

To reset the consumer group (re-process all events):
```bash
docker exec -it drasi-redis redis-cli XGROUP DESTROY hello-world-change drasi-core
```

### Build Errors

**Error**: Cargo build fails

**Solution**:
1. Ensure you're using Rust edition 2021 or later: `rustc --version`
2. Update Rust: `rustup update`
3. Clean build artifacts: `cargo clean && cargo build --release`

### Port Already in Use

**Error**: `Address already in use (port 8080)`

**Solution**:
1. Stop existing Drasi server processes:
   ```bash
   pkill -f drasi-server
   ```
2. Or specify a different port:
   ```bash
   cargo run -- --port 8081 --config examples/drasi-platform/server-config.yaml
   ```

## Advanced Usage

### Multiple Consumers

To scale horizontally, run multiple server instances with different consumer names:

```yaml
consumer_name: "consumer-2"
```

Events will be distributed across consumers in the same group.

### Replay All Events

To process all events from the beginning of the stream:

```yaml
start_id: "0"
```

### Monitoring Consumer Lag

Check consumer group information:

```bash
docker exec -it drasi-redis redis-cli XINFO GROUPS hello-world-change
```

View pending messages:

```bash
docker exec -it drasi-redis redis-cli XPENDING hello-world-change drasi-core
```

## What's Next?

After running this example, explore:

1. **Different Reactions**: Replace the log reaction with:
   - `sse` (Server-Sent Events) for web clients
   - `http` (webhooks) to call external APIs
   - `grpc` for gRPC streaming

2. **Complex Queries**: Modify the query to:
   - Join multiple node types using relationships
   - Aggregate data (COUNT, SUM, AVG)
   - Filter with complex WHERE clauses

3. **Multiple Sources**: Add additional sources:
   - PostgreSQL for relational data
   - HTTP polling for REST APIs
   - gRPC for streaming services

4. **Bootstrap Data**: Add a bootstrap provider to load initial data:
   ```yaml
   bootstrap_provider:
     type: scriptfile
     file_paths:
       - examples/drasi-platform/data/initial-messages.jsonl
   ```

## Learn More

- **Drasi Server Documentation**: See `CLAUDE.md` in the repository root
- **Cypher Query Language**: https://neo4j.com/docs/cypher-manual/current/
- **Redis Streams**: https://redis.io/docs/data-types/streams/

## Files in This Example

```
examples/drasi-platform/
├── README.md                     # This file
├── server-config.yaml            # Drasi server configuration
├── requests.http                 # VSCode REST Client examples
└── scripts/
    ├── setup-redis.sh           # Start Redis container
    ├── start-server.sh          # Build and start Drasi server
    ├── publish-event.sh         # Publish test events to Redis
    ├── view-results.sh          # Query results via API
    └── cleanup.sh               # Stop server and remove Redis
```
