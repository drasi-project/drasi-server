# Drasi Server Test Suite

This directory contains the comprehensive test suite for Drasi Server, including unit tests, integration tests, and test utilities.

## Current Test Structure

### 1. Rust Unit/Integration Tests (tests/*.rs)

**Available test files:**

- **`api.rs`** - Main API test module entry point
  ```bash
  cargo test --test api
  ```

- **`api_create_query_joins.rs`** - Tests for creating queries with synthetic joins via API
  ```bash
  cargo test --test api_create_query_joins
  ```

- **`library_integration.rs`** - Integration tests for using DrasiServer as a library
  ```bash
  cargo test --test library_integration
  ```

- **`server_integration_test.rs`** - Integration tests for server components working together
  ```bash
  cargo test --test server_integration_test
  ```

- **`server_start_stop_test.rs`** - Tests server lifecycle (start/stop/restart) and state management
  ```bash
  cargo test --test server_start_stop_test
  ```

### 2. API Tests (tests/api/)

Comprehensive REST API testing suite ensuring API stability and correctness:

- **`contract_test.rs`** - API contract validation, serialization/deserialization tests
- **`integration_test.rs`** - Full API integration testing with DrasiServerCore
- **`state_consistency_test.rs`** - Component state management and consistency
- **`persistence_test.rs`** - Configuration persistence and atomic write tests

Run all API tests:
```bash
cargo test --test api
```

### 3. gRPC Protocol Tests (tests/grpc/)

Protocol-based testing for gRPC sources and reactions:

**Configuration files:**
- **`grpc_example.yaml`** - Standard gRPC configuration
- **`grpc_adaptive_example.yaml`** - Adaptive gRPC configuration with batching

**Test scripts:**
- **`run_test.sh`** - Main gRPC test runner
- **`run_test_adaptive.sh`** - Adaptive mode test runner
- **`run_test_debug.sh`** - Debug mode test runner

**README.md** - gRPC test documentation

Run gRPC tests (requires manual setup):
```bash
cd tests/grpc
./run_test.sh
```

### 4. HTTP Protocol Tests (tests/http/)

HTTP source and reaction testing:

**Configuration files:**
- **`http_example.yaml`** - Standard HTTP configuration
- **`http_adaptive_example.yaml`** - Adaptive HTTP configuration with batching

**Test scripts:**
- **`run_test.sh`** - Main HTTP test runner
- **`run_test_adaptive.sh`** - Adaptive mode test runner

Run HTTP tests (requires manual setup):
```bash
cd tests/http
./run_test.sh
```

### 5. SSE Console Utility (tests/sse-console/)

Interactive Server-Sent Events testing utility for real-time monitoring:

**Purpose:**
- Interactive SSE client for testing SSE reactions
- Configurable server URL to test any Drasi Server instance
- Multiple test profiles (price-ticker, portfolio, watchlist, etc.)
- Real-time event logging with colored output

**Requirements:**
- Node.js 16+
- Running Drasi Server instance
- Active data sources

Run SSE console:
```bash
cd tests/sse-console
npm install
npm start <config-name>  # e.g., npm start watchlist
```

See `tests/sse-console/README.md` for detailed usage.

### 6. Test Support Utilities (tests/test_support/)

Helper utilities for integration tests:

- **`mod.rs`** - Module exports
- **`redis_helpers.rs`** - Redis test utilities for platform source tests

Used by tests that require Redis (platform source integration tests).

### 7. Test Runner Scripts

**Available scripts:**

- **`run_all_cargo_tests.sh`** ⭐ **RECOMMENDED** - Comprehensive Cargo test runner
  ```bash
  ./tests/run_all_cargo_tests.sh
  ```
  Runs all automated Rust tests with clear output and summary.

- **`run_all_tests.sh`** - Generic test runner
- **`run_all.sh`** - Alternative test runner
- **`run_interactive_demo.sh`** - Interactive demonstration
- **`grpc_integration_test.sh`** - Standalone gRPC integration test

## Directory Structure

```
tests/
├── *.rs                    # Rust integration test files
├── api/                    # REST API test suite
│   ├── contract_test.rs
│   ├── integration_test.rs
│   ├── persistence_test.rs
│   ├── state_consistency_test.rs
│   └── README.md
├── grpc/                   # gRPC protocol tests
│   ├── grpc_example.yaml
│   ├── grpc_adaptive_example.yaml
│   ├── run_test.sh
│   ├── run_test_adaptive.sh
│   ├── run_test_debug.sh
│   └── README.md
├── http/                   # HTTP protocol tests
│   ├── http_example.yaml
│   ├── http_adaptive_example.yaml
│   ├── run_test.sh
│   └── run_test_adaptive.sh
├── sse-console/           # SSE testing utility (Node.js)
│   ├── package.json
│   ├── configs.json
│   ├── index.ts
│   └── README.md
├── test_support/          # Test helper utilities
│   ├── mod.rs
│   └── redis_helpers.rs
├── run_all_cargo_tests.sh # Main automated test runner ⭐
├── grpc_integration_test.sh
├── run_all_tests.sh
├── run_all.sh
├── run_interactive_demo.sh
└── README.md              # This file
```

## Running Tests

### Quick Start

```bash
# Run all automated Rust tests (RECOMMENDED)
./tests/run_all_cargo_tests.sh

# Run all Rust tests directly
cargo test

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

### Run Specific Test Categories

```bash
# Library unit tests (in src/)
cargo test --lib

# API tests
cargo test --test api

# Integration tests
cargo test --test server_integration_test
cargo test --test library_integration

# Specific test file
cargo test --test server_start_stop_test
```

### Run Manual/Protocol Tests

```bash
# gRPC tests (requires setup)
cd tests/grpc && ./run_test.sh

# HTTP tests (requires setup)
cd tests/http && ./run_test.sh

# SSE console (interactive)
cd tests/sse-console && npm install && npm start watchlist
```

### Run with Debug Logging

```bash
# Enable debug logging for all components
RUST_LOG=debug cargo test -- --nocapture

# Specific component logging
RUST_LOG=drasi_server::api=debug cargo test --test api -- --nocapture

# Mixed log levels
RUST_LOG=drasi_server=debug,drasi_server_core=info cargo test
```

## Test Coverage Summary

**Current automated test count: 78 tests**

Breakdown:
- **18** library unit tests (src/)
- **44** API tests (tests/api/)
- **1** API query joins test
- **7** library integration tests
- **4** server integration tests
- **4** server start/stop tests

**Test coverage includes:**
- ✅ REST API endpoints and contracts
- ✅ Server lifecycle (start/stop/restart)
- ✅ Component state management
- ✅ Configuration persistence
- ✅ Library mode usage
- ✅ Query joins functionality
- ✅ Configuration validation
- ✅ Error handling and recovery
- ✅ Atomic write operations
- ✅ Read-only mode enforcement

**Manual/Interactive tests:**
- gRPC protocol tests (requires setup)
- HTTP protocol tests (requires setup)
- SSE console utility (Node.js, requires running server)

## Test Development Guidelines

### Adding New Rust Tests

1. Create test file in `tests/` or add to `tests/api/`
   ```rust
   #[tokio::test]
   async fn test_new_functionality() {
       // Test implementation
   }
   ```

2. Run the test:
   ```bash
   cargo test test_new_functionality
   ```

3. Update this README with test description

### Adding Shell Script Tests

1. Create executable script in appropriate subdirectory
   ```bash
   #!/bin/bash
   set -e
   # Test implementation
   ```

2. Make it executable:
   ```bash
   chmod +x tests/subdir/test_script.sh
   ```

3. Document in this README

### Test Best Practices

1. **Isolation**: Tests should not depend on external services unless explicitly testing integration
2. **Cleanup**: Always clean up resources (files, processes)
3. **Timeouts**: Use appropriate timeouts to prevent hanging tests
4. **Logging**: Use debug logging for troubleshooting
5. **Error Handling**: Provide clear error messages and exit codes
6. **Async**: Use `#[tokio::test]` for async tests

## Troubleshooting

### Common Issues

1. **Port Conflicts**
   - Tests may use ports 8080, 9000, 50051, 50052, etc.
   - Kill processes using these ports or change test configurations

2. **Test File Not Found**
   - Ensure test file is in `tests/` directory
   - Check file naming matches Cargo conventions

3. **Script Permission Denied**
   - Make scripts executable: `chmod +x tests/script.sh`

4. **Redis Tests Failing**
   - Some tests require Redis for platform source testing
   - Install Redis or skip these specific tests

### Debug Mode

Run tests with verbose output:
```bash
# Show all test output
cargo test -- --nocapture

# Show debug logs
RUST_LOG=debug cargo test -- --nocapture

# Run single test with logs
RUST_LOG=debug cargo test test_name -- --nocapture
```

## CI/CD Integration

Example GitHub Actions workflow:
```yaml
- name: Run Unit Tests
  run: cargo test --lib

- name: Run All Tests
  run: ./tests/run_all_cargo_tests.sh

- name: Run with Coverage
  run: |
    cargo install cargo-tarpaulin
    cargo tarpaulin --out Xml
```

## Missing Test Infrastructure

The following test directories are **not currently implemented** but may be added in the future:

- `tests/bootstrap/` - Bootstrap-specific tests (referenced in old scripts)
- `tests/integration/` - Standalone integration tests
- `tests/postgres/` - PostgreSQL-specific tests
- `tests/sdk/rust/` - Rust SDK tests

If you need these, they should be created as part of test suite expansion.

## Maintenance

### Cleanup Generated Files

```bash
# Clean Rust build artifacts
cargo clean

# Clean test logs
find tests -name "*.log" -delete

# Clean SSE console builds
cd tests/sse-console && npm run clean
```

### Running Full Test Suite

```bash
# Complete test run
./tests/run_all_cargo_tests.sh

# Verify all tests pass
cargo test --quiet
```

## Additional Resources

- Main repository README: `../README.md`
- CLAUDE.md for AI assistant context: `../CLAUDE.md`
- gRPC test documentation: `tests/grpc/README.md`
- SSE console documentation: `tests/sse-console/README.md`
