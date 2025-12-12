# Drasi Platform Example - Redis Stream Integration

This example demonstrates how to use the **platform source** in Drasi Server to consume events from Redis Streams. The platform source is specifically designed to bridge external Drasi Platform infrastructure with drasi-server-core queries via Redis.

## Overview

This example shows:
- How to configure a **platform source** to connect to Redis Streams
- How to use a **platform reaction** to publish query results back to Redis Streams
- Real-time event processing with platform bootstrap support
- Continuous Cypher query filtering
- Dual-reaction pattern: console logging + Redis stream publishing
- CloudEvent format for standards-based integration

The example demonstrates a complete Drasi Platform integration: consuming events from a Redis stream, filtering them with a Cypher query, and publishing results to another Redis stream in CloudEvent format for downstream consumption.

## Architecture

```
                                                    ┌──────────────────┐
                                              ┌────▶│   Log Reaction   │
                                              │     │  (Console Output)│
┌─────────────┐      ┌──────────────────┐    │     └──────────────────┘
│   Redis     │      │  Platform Source │    │
│   Stream    │─────▶│  (Redis Client)  │────┤     ┌──────────────────┐       ┌─────────────┐
│             │      │                  │    │     │    Platform      │       │   Redis     │
│ hello-world │      │  Consumes from   │    └────▶│    Reaction      │──────▶│   Stream    │
│  -change    │      │  stream with     │          │  (CloudEvents)   │       │             │
│  (INPUT)    │      │  consumer group  │          │                  │       │ hello-world │
│             │      │                  │          └──────────────────┘       │ -from       │
└─────────────┘      └──────────────────┘                                     │ -results    │
                              │                                               │  (OUTPUT)   │
                              │                                               └─────────────┘
                              ▼
                     ┌─────────────────┐
                     │      Query      │
                     │    (Cypher)     │
                     │                 │
                     │  Filters for    │
                     │  "Hello World"  │
                     │  messages       │
                     └─────────────────┘
```

### Data Flow

1. **Input Stream**: External events published to `hello-world-change` Redis stream
2. **Platform Source**: Consumes events using Redis consumer groups with platform bootstrap
3. **Query**: Continuous Cypher query filters nodes with label `Message` and property `Message: "Hello World"`
4. **Dual Reactions**:
   - **Log Reaction**: Outputs matching results to console for debugging
   - **Platform Reaction**: Publishes results to `hello-world-from-results` stream in CloudEvent format
5. **Output Stream**: Downstream consumers read CloudEvents from `hello-world-from-results`

### Why Dual Reactions?

This example uses **both** log and platform reactions to demonstrate a real-world pattern:
- **Log Reaction**: Provides immediate visibility into query results (development/debugging)
- **Platform Reaction**: Enables downstream integration and microservices architecture (production)

Both reactions process the same query results simultaneously with minimal overhead.

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

You should see the event logged in the Drasi server terminal (log reaction), showing:
```
MessageId: msg-1234567890
MessageFrom: Alice
```

### 4. Consume Results from Redis Stream

View the CloudEvent-formatted results published by the platform reaction:

```bash
cd examples/drasi-platform/scripts
./consume-results.sh
```

This will display all CloudEvents from the `hello-world-from-results` stream, including:
- Control events (BootstrapStarted, Running)
- Data change events with query results

### 5. View Query Results via API

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
  - kind: platform
    id: platform-redis-source
    auto_start: true
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

### Reactions

This example uses **two reactions** to demonstrate the dual-reaction pattern:

#### 1. Log Reaction (Development/Debugging)

Outputs query results to the console for immediate visibility:

```yaml
reactions:
  - kind: log
    id: log-hello-world
    auto_start: true
    queries:
      - hello-world-from
```

#### 2. Platform Reaction (Production Integration)

Publishes query results to a Redis stream in CloudEvent format for downstream consumption:

```yaml
  - kind: platform
    id: platform-hello-world-results
    auto_start: true
    queries:
      - hello-world-from
    redis_url: "redis://localhost:6379"
    pubsub_name: "drasi-pubsub"
    source_name: "drasi-core"
    max_stream_length: 10000
    emit_control_events: true
```

**Key Properties:**
- `redis_url`: Redis connection URL (same instance as platform source)
- `pubsub_name`: Dapr pubsub component name (default: "drasi-pubsub")
- `source_name`: CloudEvent source identifier (default: "drasi-core")
- `max_stream_length`: Maximum messages in results stream (uses MAXLEN ~)
- `emit_control_events`: Publish lifecycle events (default: true)

**Output Stream Naming:**
Results are automatically published to a stream named: `{query-id}-results`

For the `hello-world-from` query, results go to: `hello-world-from-results`

### Platform Reaction Output Format

The platform reaction publishes query results in **Dapr CloudEvent** format:

```json
{
  "data": {
    "addedResults": [
      {
        "MessageId": "msg-001",
        "MessageFrom": "Alice"
      }
    ],
    "updatedResults": [],
    "deletedResults": [],
    "kind": "change",
    "queryId": "hello-world-from",
    "sequence": 1,
    "sourceTimeMs": 1759716317494,
    "metadata": {
      "tracking": {
        "query": {
          "dequeue_ns": 1759503510251715012,
          "enqueue_ns": 1759503510250255429,
          "queryEnd_ns": 1759503510254644804,
          "queryStart_ns": 1759503510251874804
        }
      }
    }
  },
  "datacontenttype": "application/json",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "pubsubname": "drasi-pubsub",
  "source": "drasi-core",
  "specversion": "1.0",
  "time": "2021-01-01T00:00:00.000Z",
  "topic": "hello-world-from-results",
  "type": "com.dapr.event.sent"
}
```

**CloudEvent Fields:**
- `data.addedResults`: Array of newly matched results (always present, may be empty)
- `data.updatedResults`: Array of modified results (always present, may be empty)
- `data.deletedResults`: Array of removed results (always present, may be empty)
- `data.kind`: Event type (`"change"` for query result changes, `"control"` for lifecycle events)
- `data.queryId`: Source query identifier
- `data.sequence`: Monotonic sequence number for ordering
- `data.sourceTimeMs`: Source timestamp in milliseconds (from the original event)
- `data.metadata.tracking`: Performance tracking information with query execution times
- `pubsubname`: Dapr pubsub component name
- `source`: Event source identifier
- `topic`: Stream/topic name (derived from query ID)

**Important Notes:**
- The `addedResults`, `updatedResults`, and `deletedResults` arrays are **always present** in change events, even if empty
- Results contain only the fields specified in the query's `RETURN` clause
- The `metadata.tracking` object provides query execution timing for performance monitoring

### Control Events

When `emit_control_events: true`, the platform reaction publishes lifecycle events:

- **BootstrapStarted**: Bootstrap phase begins
- **BootstrapCompleted**: Bootstrap phase completes
- **Running**: Reaction is actively processing query results
- **Stopped**: Reaction has been stopped
- **Deleted**: Reaction has been deleted

Control events use `data.kind: "control"` and include a `controlEvent` field instead of result arrays.

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

All these events should appear in:
1. Drasi server console logs (log reaction)
2. Results stream (platform reaction)

Verify both reactions are working:

```bash
# Check console logs (you should see MessageFrom: Alice, Bob, Charlie)

# Check results stream
./scripts/consume-results.sh
```

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

This event should NOT appear in the logs or results stream because `Message` is not `"Hello World"`.

### Verify Platform Reaction Output

Check that the platform reaction is publishing CloudEvents to Redis:

```bash
# Check if results stream exists
docker exec drasi-redis redis-cli EXISTS hello-world-from-results

# Check message count
docker exec drasi-redis redis-cli XLEN hello-world-from-results

# View all CloudEvents
docker exec drasi-redis redis-cli XRANGE hello-world-from-results - +

# Or use the convenience script
./scripts/consume-results.sh
```

Expected output includes:
- Control events: `BootstrapStarted`, `BootstrapCompleted`, `Running`
- Data change events with `addedResults`, `updatedResults`, `deletedResults`

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

# Get log reaction status
GET http://localhost:8080/api/reactions/log-hello-world

# Get platform reaction status
GET http://localhost:8080/api/reactions/platform-hello-world-results

# Stop a reaction
POST http://localhost:8080/api/reactions/log-hello-world/stop
POST http://localhost:8080/api/reactions/platform-hello-world-results/stop

# Start a reaction
POST http://localhost:8080/api/reactions/log-hello-world/start
POST http://localhost:8080/api/reactions/platform-hello-world-results/start
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

### Results Stream is Empty

**Problem**: Platform reaction is running but results stream has no messages

**Checklist**:
1. Verify platform reaction is running:
   ```bash
   curl http://localhost:8080/api/reactions/platform-hello-world-results
   ```
2. Check that query has results:
   ```bash
   curl http://localhost:8080/api/queries/hello-world-from/results
   ```
3. Verify results stream exists:
   ```bash
   docker exec drasi-redis redis-cli EXISTS hello-world-from-results
   ```
4. Check stream length:
   ```bash
   docker exec drasi-redis redis-cli XLEN hello-world-from-results
   ```
5. Look for control events (should appear even if no data events):
   ```bash
   docker exec drasi-redis redis-cli XRANGE hello-world-from-results - + | grep -i running
   ```

### CloudEvents Not Formatted Correctly

**Problem**: Events in results stream don't match CloudEvent spec

**Solution**:
1. Verify you're using the platform reaction (not log reaction)
2. Check platform reaction configuration has correct properties
3. Ensure drasi-server-core is up to date
4. View raw stream data:
   ```bash
   docker exec drasi-redis redis-cli XRANGE hello-world-from-results - +
   ```

### Stream Growing Too Large

**Problem**: Results stream consuming too much memory

**Solutions**:
1. Configure `max_stream_length` in platform reaction:
   ```yaml
   max_stream_length: 10000  # Keep last 10,000 messages
   ```
2. Implement a consumer to process and archive messages
3. Manually trim the stream:
   ```bash
   docker exec drasi-redis redis-cli XTRIM hello-world-from-results MAXLEN ~ 1000
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

1. **Consume Results in Your Application**:
   - Build a microservice that reads from `hello-world-from-results` stream
   - Process CloudEvents in your application code
   - Integrate with Dapr for pub/sub across languages
   - Use the standard CloudEvent format for interoperability

2. **Different Reactions**: Add or replace reactions:
   - `sse` (Server-Sent Events) for web clients
   - `http` (webhooks) to call external APIs
   - `grpc` for gRPC streaming
   - Multiple reactions on the same query

3. **Complex Queries**: Modify the query to:
   - Join multiple node types using relationships
   - Aggregate data (COUNT, SUM, AVG)
   - Filter with complex WHERE clauses
   - Use graph patterns for complex matching

4. **Multiple Sources**: Add additional sources:
   - PostgreSQL for relational data
   - HTTP polling for REST APIs
   - gRPC for streaming services
   - Combine multiple platform sources

5. **Production Deployment**:
   - Use Redis Cluster for high availability
   - Configure multiple consumers for horizontal scaling
   - Set `max_stream_length` to prevent unbounded growth
   - Implement monitoring and alerting on control events
   - Archive CloudEvents for long-term storage

## Learn More

- **Drasi Server Documentation**: See `CLAUDE.md` in the repository root
- **Cypher Query Language**: https://neo4j.com/docs/cypher-manual/current/
- **Redis Streams**: https://redis.io/docs/data-types/streams/
- **CloudEvents Specification**: https://cloudevents.io/
- **Dapr Pub/Sub**: https://docs.dapr.io/developing-applications/building-blocks/pubsub/

## Files in This Example

```
examples/drasi-platform/
├── README.md                     # This file
├── server-config.yaml            # Drasi server configuration
├── requests.http                 # VSCode REST Client examples
└── scripts/
    ├── setup-redis.sh           # Start Redis container
    ├── start-server.sh          # Build and start Drasi server
    ├── publish-event.sh         # Publish test events to Redis input stream
    ├── consume-results.sh       # Consume CloudEvents from results stream (NEW)
    ├── view-results.sh          # Query results via API
    └── cleanup.sh               # Stop server and remove Redis
```

### Script Descriptions

- **setup-redis.sh**: Starts a Redis Docker container for event streaming
- **start-server.sh**: Builds and starts the Drasi server with platform configuration
- **publish-event.sh**: Publishes test events to the `hello-world-change` input stream
- **consume-results.sh**: Reads CloudEvent-formatted query results from `hello-world-from-results`
- **view-results.sh**: Queries current results via the Drasi REST API
- **cleanup.sh**: Stops the server and removes the Redis container
