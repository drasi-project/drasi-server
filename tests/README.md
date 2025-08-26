# Drasi Server Test Suite

This directory contains the comprehensive test suite for Drasi Server, including unit tests, integration tests, E2E tests, and test utilities.

## Test Categories

### 1. Rust Unit/Integration Tests (tests/*.rs)

These Rust test files provide comprehensive coverage of core functionality:

#### Bootstrap Tests
- **`bootstrap_test.rs`** - Tests bootstrap request/response flow through channels, label extraction from Cypher queries, and bootstrap with label filtering
  ```bash
  cargo test bootstrap_test
  ```

- **`bootstrap_simple_test.rs`** - Simplified bootstrap mechanism tests
  ```bash
  cargo test bootstrap_simple_test
  ```

- **`bootstrap_e2e_test.rs`** - End-to-end bootstrap functionality testing
  ```bash
  cargo test bootstrap_e2e_test
  ```

#### Server Core Tests
- **`server_start_stop_test.rs`** - Tests server lifecycle (start/stop/restart), state management, and auto-start functionality
  ```bash
  cargo test server_start_stop_test
  ```

- **`server_integration_test.rs`** - Integration tests for server components working together
  ```bash
  cargo test server_integration_test
  ```

#### Component Tests
- **`component_connectivity_test.rs`** - Verifies end-to-end data flow connectivity between sources, queries, and reactions
  ```bash
  cargo test component_connectivity_test
  ```

- **`subscription_validation_test.rs`** - Tests query subscription to sources and data reception after auto-start
  ```bash
  cargo test subscription_validation_test
  ```

- **`race_condition_detection_test.rs`** - Tests for potential race conditions in concurrent operations
  ```bash
  cargo test race_condition_detection_test
  ```

#### Library Mode Tests
- **`library_application_test.rs`** - Tests using DrasiServerCore as a library
  ```bash
  cargo test library_application_test
  ```

- **`library_integration.rs`** - Integration tests for library mode functionality
  ```bash
  cargo test library_integration
  ```

#### Channel Tests
- **`tokio_channel_e2e.rs`** - End-to-end tests for tokio channel communication
  ```bash
  cargo test tokio_channel_e2e
  ```

### 2. API Tests (tests/api/)

Comprehensive REST API testing suite ensuring API stability and correctness:

- **`contract_test.rs`** - API contract validation and serialization tests
- **`integration_test.rs`** - Full API integration testing with DrasiServerCore
- **`state_consistency_test.rs`** - Component state management and consistency
- **`openapi_validation_test.rs`** - OpenAPI documentation validation

Additional API tests:
- **`api.rs`** - Main API test module
- **`api_create_query_joins.rs`** - Tests for query join creation via API

Run API tests:
```bash
cargo test --test api
```

### 3. gRPC Tests (tests/grpc/)

Protocol buffer-based testing for gRPC sources and reactions:

- **`grpc_example.yaml`** - Standard gRPC configuration
- **`grpc_adaptive_example.yaml`** - Adaptive gRPC configuration
- **`run_test.sh`** - Main gRPC test runner
- **`run_test_adaptive.sh`** - Adaptive mode test runner
- **`run_test_debug.sh`** - Debug mode test runner
- **`grpc_integration_test.sh`** - gRPC integration test script

Run gRPC tests:
```bash
./tests/grpc/run_test.sh
```

### 4. HTTP Tests (tests/http/)

HTTP source and reaction testing:

- **`http_example.yaml`** - Standard HTTP configuration
- **`http_adaptive_example.yaml`** - Adaptive HTTP configuration  
- **`run_test.sh`** - Main HTTP test runner
- **`run_test_adaptive.sh`** - Adaptive mode test runner

Run HTTP tests:
```bash
./tests/http/run_test.sh
```

### 5. PostgreSQL Tests (tests/postgres/)

Comprehensive PostgreSQL testing suite:

#### Scripts (tests/postgres/scripts/)
- **`test_postgres_wal.sh`** - Tests PostgreSQL WAL (Write-Ahead Log) source functionality
- **`test_postgres_wal_docker.sh`** - PostgreSQL WAL testing using Docker containers
- **`setup_postgres_standalone.sh`** - Sets up standalone PostgreSQL for testing
- **`configure_postgres_wal.sh`** - Configures PostgreSQL for WAL replication
- **`test_setup.sh`** - Tests PostgreSQL setup and configuration
- **`connect.sh`** - PostgreSQL connection utility
- **`clean.sh`** - Cleanup script for PostgreSQL test environment
- **`stop.sh`** - Stops PostgreSQL test containers

#### Docker Configuration (tests/postgres/docker/)
- **`postgres-internal-compose.yml`** - Docker Compose for internal PostgreSQL source
- **`postgres-setup-compose.yml`** - Docker Compose for PostgreSQL setup
- **`postgres-wal.conf`** - PostgreSQL configuration for WAL

#### SQL Scripts (tests/postgres/sql/)
- Test data and operations for PostgreSQL testing

### 6. SSE Console Utility (tests/sse-console/)

Server-Sent Events testing utility for real-time monitoring:

- **Interactive SSE client** for testing SSE reactions
- **Configurable server URL** to test any Drasi Server instance
- **Multiple test profiles** (price-ticker, portfolio, watchlist, etc.)
- **Real-time event logging** with colored output

Run SSE console:
```bash
cd tests/sse-console
npm install
npm start <config-name>  # e.g., npm start watchlist
```

See `tests/sse-console/README.md` for detailed usage.

### 7. SDK Tests (tests/sdk/rust/)

Rust SDK tests for internal components:

- **`internal_source_test.rs`** - Tests for internal source SDK functionality
- **`internal_reaction_test.rs`** - Tests for internal reaction SDK functionality

Run SDK tests:
```bash
cargo test --test internal_source_test
cargo test --test internal_reaction_test
```

### 8. Test Runners

Test execution scripts:

- **`run_working_tests.sh`** - Main test runner with proper error handling and summary
  ```bash
  ./tests/run_working_tests.sh
  ```
- **`run_all.sh`** - Alternative test runner
- **`run_interactive_demo.sh`** - Runs an interactive demonstration
- **`grpc_integration_test.sh`** - Standalone gRPC integration test

Note: `run_all_tests.sh` has incorrect paths and should not be used.

## Test Organization

### Directory Structure
```
tests/
├── *.rs                    # Rust test files (unit/integration)
├── api/                    # REST API test suite
├── grpc/                   # gRPC protocol tests
├── http/                   # HTTP protocol tests
├── postgres/               # PostgreSQL-specific tests
│   ├── configs/           # PostgreSQL test configs
│   ├── docker/            # Docker compose files
│   ├── scripts/           # Test scripts
│   └── sql/               # SQL test data
├── sdk/rust/              # Rust SDK tests
├── sse-console/           # SSE testing utility
└── run_working_tests.sh   # Main test runner
```

## Running Tests

### Run All Tests
```bash
# Run all working tests with summary
./tests/run_working_tests.sh

# Run all Rust tests
cargo test

# Run specific test category
cargo test bootstrap
cargo test server
```

### Run Specific Test Categories
```bash
# API tests
cargo test --test api

# gRPC tests
./tests/grpc/run_test.sh

# HTTP tests
./tests/http/run_test.sh

# PostgreSQL tests
./tests/postgres/scripts/test_postgres_wal.sh

# SSE console (interactive)
cd tests/sse-console && npm start watchlist
```

### Run with Logging
```bash
# Enable debug logging
RUST_LOG=debug cargo test

# Run shell test with debug logging
RUST_LOG=drasi_server=debug,drasi_core=info ./tests/grpc/run_test.sh

# Run with specific component logging
RUST_LOG=drasi_server::api=debug cargo test --test api
```

## Test Development Guidelines

### Adding New Tests

1. **Rust Tests**: Place in appropriate file or create new file in `tests/`
   ```rust
   #[tokio::test]
   async fn test_new_functionality() -> Result<()> {
       // Test implementation
   }
   ```

2. **Shell Tests**: Create executable script in appropriate subdirectory
   ```bash
   #!/bin/bash
   set -e
   # Test implementation
   ```

3. **Update Documentation**: Add test description to this README

### Test Best Practices

1. **Isolation**: Tests should not depend on external services unless testing integration
2. **Cleanup**: Always clean up resources (processes, files, containers)
3. **Timeouts**: Use appropriate timeouts to prevent hanging tests
4. **Logging**: Use debug logging for troubleshooting
5. **Error Handling**: Provide clear error messages and exit codes

## CI/CD Integration

Example GitHub Actions workflow:
```yaml
- name: Run Unit Tests
  run: cargo test --lib

- name: Run Integration Tests  
  run: ./tests/run_working_tests.sh

- name: Run E2E Tests
  run: |
    docker-compose -f tests/e2e/docker-compose.yml up -d
    cargo test --test '*e2e*'
    docker-compose -f tests/e2e/docker-compose.yml down
```

## Troubleshooting

### Common Issues

1. **Port Conflicts**: Ensure ports 8080, 9000, 9001 are available
2. **PostgreSQL Tests**: Require PostgreSQL installed or Docker
3. **Permissions**: Ensure test scripts are executable (`chmod +x`)
4. **Dependencies**: Some tests require specific tools (psql, docker, etc.)

### Debug Mode

Run tests with debug logging:
```bash
RUST_LOG=drasi_server=debug,drasi_core=debug cargo test -- --nocapture
```

## Test Coverage

Current test coverage includes:
- ✅ REST API endpoints and contracts
- ✅ Server lifecycle (start/stop/restart)
- ✅ Component connectivity
- ✅ Internal sources (mock, PostgreSQL)
- ✅ HTTP/gRPC sources and reactions
- ✅ Query subscription and data flow
- ✅ Bootstrap mechanism
- ✅ Library mode usage
- ✅ Configuration validation
- ✅ Error handling and recovery
- ✅ Query joins functionality
- ✅ SSE reactions

## Maintenance Notes

### Generated Files to Clean
The following files are generated during test runs and can be safely deleted:
- `*.log` files in any test directory
- `*.js` files in sse-console (compiled TypeScript)
- `target/` directories (Rust build artifacts)
- `Cargo.lock` files in test subdirectories

### Recommended Cleanup
```bash
# Clean generated files
find tests -name "*.log" -delete
find tests -name "target" -type d -exec rm -rf {} +
rm -f tests/sse-console/*.js
```

See `tests/CLEANUP_RECOMMENDATIONS.md` for detailed cleanup guidelines.