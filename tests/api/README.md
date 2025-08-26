# Drasi Server API Testing Guide

This directory contains comprehensive tests for the Drasi Server REST API, ensuring the API remains stable, well-documented, and functional over time.

## Test Categories

### 1. Contract Tests (`contract_test.rs`)
Tests that validate API contracts remain stable:
- **Request/Response Formats**: Validates JSON serialization/deserialization
- **Schema Validation**: Ensures data structures match expected schemas
- **Type Safety**: Verifies type conversions and edge cases
- **Edge Cases**: Tests unicode, special characters, empty values, etc.

### 2. Integration Tests (`integration_test.rs`)
Tests complete data flow from API to DrasiServerCore:
- **Component Lifecycle**: Full CRUD operations via API
- **Auto-Start Behavior**: Validates automatic component startup
- **Error Handling**: Tests error responses and status codes
- **Read-Only Mode**: Ensures write operations are blocked appropriately
- **Idempotent Operations**: Verifies repeated creates are handled gracefully

### 3. State Consistency Tests (`state_consistency_test.rs`)
Tests that ensure consistent state across components:
- **State Transitions**: Validates component state machine
- **Router Registration**: Tests data/subscription/bootstrap router cleanup
- **Concurrent Operations**: Ensures thread-safe operations
- **Update Preservation**: Verifies state is maintained during updates
- **Cascading Effects**: Tests dependent component behavior

### 4. OpenAPI Validation Tests (`openapi_validation_test.rs`)
Automated tests to ensure documentation stays in sync:
- **Schema Completeness**: All types are documented
- **Endpoint Coverage**: All routes have OpenAPI definitions
- **Response Codes**: Proper HTTP status codes documented
- **Parameter Documentation**: Path and query parameters described
- **Reference Validation**: All $ref pointers are valid

## Running the Tests

### Run All API Tests
```bash
cargo test --test api -- --nocapture
```

### Run Specific Test Category
```bash
# Contract tests only
cargo test --test api contract_test -- --nocapture

# Integration tests only
cargo test --test api integration_test -- --nocapture

# State consistency tests only
cargo test --test api state_consistency_test -- --nocapture

# OpenAPI validation tests only
cargo test --test api openapi_validation_test -- --nocapture
```

### Run Individual Test
```bash
cargo test --test api test_source_lifecycle_via_api -- --nocapture
```

### Run Tests in Parallel
```bash
cargo test --test api -- --test-threads=4
```

### Generate Test Coverage Report
```bash
# Install tarpaulin if not already installed
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --test api --out Html --output-dir target/coverage
```

## Adding New Tests

### 1. Contract Test Example
```rust
#[test]
fn test_new_field_serialization() {
    let config = SourceConfig {
        id: "test".to_string(),
        source_type: "internal.mock".to_string(),
        auto_start: false,
        properties: HashMap::new(),
        // new_field: "value".to_string(), // Add new field
    };
    
    let json = serde_json::to_value(&config).unwrap();
    assert_eq!(json["new_field"], "value");
}
```

### 2. Integration Test Example
```rust
#[tokio::test]
async fn test_new_endpoint() {
    let (router, _, _, _) = create_test_router().await;
    
    let response = router
        .oneshot(
            Request::builder()
                .uri("/new-endpoint")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}
```

### 3. State Test Example
```rust
#[tokio::test]
async fn test_new_state_behavior() {
    let runtime_config = Arc::new(RuntimeConfig::default());
    let manager = Arc::new(SourceManager::new(runtime_config));
    
    // Test state transitions
    // Test concurrent access
    // Test cleanup
}
```

## Test Fixtures and Helpers

### Common Test Router Setup
The `create_test_router()` function in `integration_test.rs` provides a fully configured test router with all dependencies injected.

### Mock Components
Tests use `internal.mock` source type which doesn't require external dependencies.

### Async Test Support
All integration and state tests use `#[tokio::test]` for async support.

## Known Limitations

1. **Database Tests**: Currently tests don't use real databases. Consider using testcontainers for PostgreSQL tests.
2. **Performance Tests**: Load testing is not yet implemented. Consider adding with criterion.rs.
3. **WebSocket Tests**: Streaming endpoints not yet tested (if applicable).
4. **Authentication**: No auth tests as API doesn't currently have authentication.

## Troubleshooting Test Failures

### Test Timeout
If tests hang, check for:
- Deadlocks in async code
- Infinite loops in state machines
- Missing `await` on async functions

### Port Conflicts
Integration tests may fail if port 8080 is in use. Tests create in-memory routers, so this shouldn't normally be an issue.

### Flaky Tests
If tests pass individually but fail when run together:
- Check for shared state between tests
- Ensure proper cleanup in test teardown
- Consider using `serial_test` crate for tests that can't run in parallel

### Compilation Errors
Ensure you have the required features enabled:
```toml
[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
hyper = { version = "0.14", features = ["full"] }
tower = { version = "0.4", features = ["full"] }
```

## CI/CD Integration

### GitHub Actions Example
```yaml
name: API Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run API tests
        run: cargo test --test api
      - name: Generate coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --test api --out Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v2
```

## Future Improvements

1. **Property-Based Testing**: Add proptest for fuzzing API inputs
2. **Contract Testing**: Consider using Pact for consumer-driven contracts
3. **Load Testing**: Add performance benchmarks with criterion
4. **Mutation Testing**: Use cargo-mutants to verify test effectiveness
5. **API Versioning**: Add tests for backward compatibility when v2 is introduced
6. **Security Testing**: Add tests for input validation and injection attacks
7. **Documentation Generation**: Auto-generate API client from OpenAPI spec

## Contributing

When adding new API endpoints:
1. Add contract tests for request/response formats
2. Add integration tests for the full flow
3. Add state consistency tests if the endpoint affects state
4. Update OpenAPI documentation and add validation tests
5. Update this README with any new patterns or considerations

## Related Documentation

- [Drasi Server README](../../../README.md)
- [API Handler Documentation](../../../src/api/README.md)
- [OpenAPI Specification](../../../src/api/openapi.rs)
- [Example API Requests](../../../examples/web_api_query.http)