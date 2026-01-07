# Drasi Server Test Suite

This directory contains the comprehensive test suite for Drasi Server, including unit tests, integration tests, and test utilities.

## Test Summary

**Total Automated Tests: 282**

| Category | Count | Command |
|----------|-------|---------|
| Unit tests (src/) | 170 | `cargo test --lib` |
| Integration tests (tests/) | 106 | `cargo test --test '*'` |
| Doc tests | 3 | `cargo test --doc` |
| **Total** | **282** | `cargo test` |

## Quick Start

```bash
# Run all automated tests (RECOMMENDED)
cargo test

# Run with logging
RUST_LOG=debug cargo test -- --nocapture

# Run specific test file
cargo test --test api_integration_test
```

## Integration Test Files

All integration tests are at the top level of `tests/` and run automatically with `cargo test`.

### API Tests

| File | Tests | Description |
|------|-------|-------------|
| `api_contract_test.rs` | 17 | API contract validation, request/response serialization |
| `api_integration_test.rs` | 9 | Full REST API integration with DrasiLib core |
| `api_persistence_test.rs` | 7 | Configuration persistence and atomic write operations |
| `api_query_joins_test.rs` | 1 | Query creation with synthetic joins via API |
| `api_state_consistency_test.rs` | 7 | Component state management and consistency |

### Server Tests

| File | Tests | Description |
|------|-------|-------------|
| `server_integration_test.rs` | 4 | Server components working together, data flow |
| `server_start_stop_test.rs` | 2 | Server lifecycle (start/stop/restart) |
| `library_integration_test.rs` | 7 | Using DrasiServer as an embedded library |

### Configuration Tests

| File | Tests | Description |
|------|-------|-------------|
| `config_value_integration_test.rs` | 5 | ConfigValue static/env variable handling |
| `example_configs_validation_test.rs` | 8 | Validates all example YAML configs in config/ |
| `readme_examples_validation_test.rs` | 4 | Validates YAML examples from README.md |

### Storage Tests

| File | Tests | Description |
|------|-------|-------------|
| `persist_index_test.rs` | 13 | RocksDB persistent index provider tests |
| `state_store_test.rs` | 14 | State store provider integration tests |
| `redis_helpers_test.rs` | 8 | Redis helper utilities for platform source |

## Test Support Module

The `test_support/` directory provides shared utilities for integration tests:

```
test_support/
├── mod.rs              # Module exports
├── mock_components.rs  # MockSource and MockReaction implementations
├── config_helpers.rs   # Configuration test utilities
└── redis_helpers.rs    # Redis test utilities
```

### Using Test Support

```rust
mod test_support;

use test_support::mock_components::{create_mock_source, create_mock_reaction};
use test_support::config_helpers::create_temp_config_file;
```

## Manual Protocol Tests

These tests require manual execution and are **NOT** run by `cargo test`:

### gRPC Tests (tests/grpc/)

Protocol-based testing for gRPC sources and reactions.

```bash
cd tests/grpc
./run_test.sh           # Standard gRPC test
./run_test_adaptive.sh  # Adaptive mode with batching
./run_test_debug.sh     # Debug mode
```

**Configuration files:**
- `grpc_example.yaml` - Standard gRPC configuration
- `grpc_adaptive_example.yaml` - Adaptive gRPC with batching

### HTTP Tests (tests/http/)

HTTP source and reaction testing.

```bash
cd tests/http
./run_test.sh           # Standard HTTP test
./run_test_adaptive.sh  # Adaptive mode with batching
```

**Configuration files:**
- `http_example.yaml` - Standard HTTP configuration
- `http_adaptive_example.yaml` - Adaptive HTTP with batching

### SSE Console (tests/sse-console/)

Interactive Server-Sent Events testing utility.

```bash
cd tests/sse-console
npm install
npm start <config-name>  # e.g., npm start watchlist
```

**Requirements:**
- Node.js 16+
- Running Drasi Server instance
- Active data sources

### PostgreSQL Tests (tests/integration/)

End-to-end tests with real PostgreSQL databases.

```bash
cd tests/integration
./run_e2e_test.sh
```

## Directory Structure

```
tests/
├── api_contract_test.rs           # API contract validation
├── api_integration_test.rs        # API integration tests
├── api_persistence_test.rs        # Persistence tests
├── api_query_joins_test.rs        # Query joins tests
├── api_state_consistency_test.rs  # State consistency tests
├── config_value_integration_test.rs
├── example_configs_validation_test.rs
├── library_integration_test.rs    # Library mode tests
├── persist_index_test.rs          # RocksDB index tests
├── readme_examples_validation_test.rs
├── redis_helpers_test.rs          # Redis utilities tests
├── server_integration_test.rs     # Server integration
├── server_start_stop_test.rs      # Server lifecycle
├── state_store_test.rs            # State store tests
├── test_support/                  # Shared test utilities
│   ├── mod.rs
│   ├── mock_components.rs
│   ├── config_helpers.rs
│   └── redis_helpers.rs
├── grpc/                          # Manual gRPC tests
│   ├── grpc_example.yaml
│   ├── grpc_adaptive_example.yaml
│   ├── run_test.sh
│   ├── run_test_adaptive.sh
│   ├── run_test_debug.sh
│   └── README.md
├── http/                          # Manual HTTP tests
│   ├── http_example.yaml
│   ├── http_adaptive_example.yaml
│   ├── run_test.sh
│   └── run_test_adaptive.sh
├── sse-console/                   # SSE testing utility
│   ├── package.json
│   ├── configs.json
│   ├── index.ts
│   └── README.md
├── integration/                   # PostgreSQL e2e tests
│   └── run_e2e_test.sh
└── README.md                      # This file
```

## Running Tests

### All Automated Tests

```bash
# Run everything
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run with debug logging
RUST_LOG=debug cargo test -- --nocapture
```

### Specific Categories

```bash
# Unit tests only (in src/)
cargo test --lib

# Integration tests only
cargo test --test '*'

# Single test file
cargo test --test api_integration_test

# Single test function
cargo test test_create_and_delete_query

# Tests matching pattern
cargo test query
```

### By Component

```bash
# API tests
cargo test --test 'api_*'

# Server tests
cargo test --test 'server_*'

# Storage tests
cargo test --test persist_index_test
cargo test --test state_store_test

# Config validation
cargo test --test '*_validation_test'
```

## Unit Tests in Source

The `src/` directory contains 170 unit tests across these modules:

| Module | Tests | Description |
|--------|-------|-------------|
| `src/init/builder.rs` | 22 | Initialization builder tests |
| `src/init/prompts.rs` | 22+ | CLI prompt tests |
| `src/config/loader.rs` | 15+ | Config loading tests |
| `src/api/joins_tests.rs` | 15+ | Query join logic tests |
| `src/factories.rs` | 15 | Source/reaction factory tests |
| `src/api/shared/error.rs` | 22 | Error handling tests |

Run unit tests:
```bash
cargo test --lib
```

## Test Coverage

**Well-tested areas:**
- REST API endpoints and contracts
- Server lifecycle (start/stop/restart)
- Component state management
- Configuration persistence
- Library mode usage
- Query joins functionality
- Configuration validation
- Error handling and conversion
- Factory pattern (sources, reactions, state stores)

**Manual testing required:**
- gRPC protocol integration
- HTTP protocol integration
- SSE streaming
- PostgreSQL replication

## Test Development Guidelines

### Adding Integration Tests

1. Create file in `tests/` with `_test.rs` suffix:
   ```rust
   // tests/my_feature_test.rs
   mod test_support;

   use test_support::mock_components::create_mock_source;

   #[tokio::test]
   async fn test_my_feature() {
       let source = create_mock_source("test-source");
       // Test implementation
   }
   ```

2. Run the test:
   ```bash
   cargo test --test my_feature_test
   ```

### Adding Unit Tests

Add tests in the source file:
```rust
// src/my_module.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function() {
        // Test implementation
    }
}
```

### Test Best Practices

1. **Use test_support**: Import shared mocks from `test_support/mock_components.rs`
2. **Async tests**: Use `#[tokio::test]` for async functions
3. **Isolation**: Each test should be independent
4. **Cleanup**: Use temp files and directories that auto-cleanup
5. **Timeouts**: Add timeouts for operations that could hang
6. **Naming**: Use descriptive names like `test_create_query_returns_error_for_invalid_config`

## Troubleshooting

### Common Issues

1. **Port conflicts**: Tests may use ports 8080, 9000, 50051, 50052
   ```bash
   lsof -i :8080  # Find process using port
   ```

2. **Test isolation failures**: Run tests sequentially
   ```bash
   cargo test -- --test-threads=1
   ```

3. **Redis tests failing**: Some tests require Redis
   ```bash
   # Skip Redis tests
   cargo test -- --skip redis
   ```

4. **RocksDB lock errors**: Clean up stale lock files
   ```bash
   rm -rf /tmp/drasi_test_*
   ```

### Debug Mode

```bash
# All output
cargo test -- --nocapture

# Debug logs
RUST_LOG=debug cargo test -- --nocapture

# Trace logs for specific module
RUST_LOG=drasi_server::api=trace cargo test --test api_integration_test -- --nocapture
```

## CI/CD Integration

Example GitHub Actions workflow:

```yaml
- name: Run All Tests
  run: cargo test

- name: Run with Coverage
  run: |
    cargo install cargo-tarpaulin
    cargo tarpaulin --out Xml
```

## Additional Resources

- Main repository README: `../README.md`
- CLAUDE.md for development context: `../CLAUDE.md`
- gRPC test documentation: `tests/grpc/README.md`
- SSE console documentation: `tests/sse-console/README.md`
