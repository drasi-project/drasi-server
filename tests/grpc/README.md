# gRPC Source and Reaction Test

This test suite demonstrates Drasi Server's ability to:
1. Receive events through a gRPC source
2. Process them with Cypher queries
3. Send results to a gRPC reaction endpoint

## Architecture

```
┌─────────────────┐     gRPC      ┌──────────────┐     Query     ┌──────────────┐
│  Test Client    │────Events────▶│ gRPC Source  │────Results───▶│    Query     │
│ (Port 50051)    │                │              │                │  all-rooms   │
└─────────────────┘                └──────────────┘                └──────────────┘
                                                                            │
                                                                      Query Results
                                                                            │
┌─────────────────┐     gRPC      ┌──────────────┐                        ▼
│  Test Server    │◀───Results────│ gRPC Reaction│◀──────────────────Results
│ (Port 50052)    │                │              │
└─────────────────┘                └──────────────┘
```

## Components

### 1. gRPC Source (Port 50051)
- Receives `SourceChange` events via gRPC
- Supports bootstrap requests for initial data
- Handles insert, update, and delete operations

### 2. Query (all-rooms)
- Matches Room entities
- Calculates comfort level based on temperature, humidity, and CO2
- Same logic as the HTTP test for consistency

### 3. gRPC Reaction (Port 50052)
- Sends query results to external gRPC service
- Batches results for efficiency
- Includes retry logic and timeout handling

### 4. Test Client
- Sends Room events to the gRPC source
- Simulates real-world scenarios
- Validates source connectivity

### 5. Test Server  
- Receives and validates reaction outputs
- Logs results for verification
- Confirms end-to-end data flow

## Running the Test

### Prerequisites
- Rust toolchain installed
- Protocol buffer compiler (protoc)
- Port 50051 and 50052 available

### Execute Test
```bash
./run_test.sh
```

This will:
1. Build the Drasi server
2. Start the test gRPC server (reaction endpoint)
3. Start Drasi server with gRPC configuration
4. Run the test client to send events
5. Validate the results

### Manual Testing

#### Start the server:
```bash
RUST_LOG=info cargo run --release -- -c ./tests/grpc/grpc_example.yaml
```

#### Send events using grpcurl:
```bash
# Health check
grpcurl -plaintext localhost:50051 drasi.v1.SourceService/HealthCheck

# Submit an event
grpcurl -plaintext -d '{
  "source_id": "facilities-db",
  "change": {
    "type": "insert",
    "element": {
      "type": "node",
      "metadata": {
        "reference": {
          "sourceId": "facilities-db",
          "elementId": "room-1"
        },
        "labels": ["Room"],
        "effectiveFrom": 1234567890
      },
      "properties": {
        "temperature": "72",
        "humidity": "42",
        "co2": "500",
        "occupancy": "5",
        "building_name": "Building A"
      }
    }
  }
}' localhost:50051 drasi.v1.SourceService/SubmitEvent
```

## Configuration

The test uses `grpc_example.yaml` which configures:
- **gRPC Source**: Internal gRPC source on port 50051
- **Query**: Cypher query for Room comfort calculation
- **gRPC Reaction**: Sends results to localhost:50052

## Test Scenarios

1. **Basic Operations**
   - Insert new Room
   - Update Room properties
   - Delete Room

2. **Comfort Level Calculation**
   - Verify query correctly calculates comfort based on:
     - Temperature deviation from 72°F
     - Humidity deviation from 42%
     - CO2 levels

3. **Batch Processing**
   - Send multiple events
   - Verify batching in reaction

4. **Error Handling**
   - Test connection failures
   - Verify retry logic

## Differences from HTTP Test

| Feature | HTTP Test | gRPC Test |
|---------|-----------|-----------|
| Protocol | HTTP/REST | gRPC/Protocol Buffers |
| Source Port | 9000 | 50051 |
| Reaction Port | 9001 | 50052 |
| Data Format | JSON | Protobuf |
| Streaming | No | Yes (supported) |
| Performance | Good | Better (binary protocol) |

## Troubleshooting

### Port Already in Use
```bash
# Check what's using the ports
lsof -i :50051
lsof -i :50052
```

### Protocol Buffer Issues
```bash
# Regenerate proto files
protoc --rust_out=. test.proto
```

### Connection Refused
- Ensure Drasi server is running
- Check firewall settings
- Verify correct ports in configuration

## Performance Notes

gRPC typically offers:
- 10-20% better throughput than HTTP
- Lower latency for small messages
- Better streaming support
- Efficient binary serialization

## Next Steps

- Add TLS/mTLS support for secure connections
- Implement streaming tests
- Add load testing scenarios
- Benchmark against HTTP implementation