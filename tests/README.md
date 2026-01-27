# Drasi Server Test Suite

This directory contains the comprehensive test suite for Drasi Server, including unit tests, integration tests, and test utilities.

## Quick Start

```bash
# Run all automated tests (recommended)
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run with debug logging
RUST_LOG=debug cargo test -- --nocapture

# Run a specific test file
cargo test --test api_integration_test
```

## Test Summary

| Category | Count | Command |
|----------|------:|---------|
| Unit tests (in `src/`) | 206 | `cargo test --lib` |
| Integration tests (in `tests/`) | 218 | `cargo test --test '*'` |
| Doc tests | 3 | `cargo test --doc` |
| Binary tests (`main.rs`) | 59 | (included in `cargo test`) |
| **Total Automated** | **486** | `cargo test` |

---

## Integration Test Files

All `.rs` files in this directory are integration tests that run automatically with `cargo test`.

### API Tests (41 tests)

| File | Tests | Description |
|------|------:|-------------|
| `api_contract_test.rs` | 17 | REST API contract validation - request/response formats, status codes, JSON schemas |
| `api_integration_test.rs` | 9 | Full API integration with DrasiLib core - dynamic source/reaction creation |
| `api_persistence_test.rs` | 7 | Configuration persistence - atomic file writes, YAML format validation |
| `api_query_joins_test.rs` | 1 | Query creation with synthetic joins via API handlers |
| `api_state_consistency_test.rs` | 7 | Component state management and lifecycle consistency |

### Server Tests (13 tests)

| File | Tests | Description |
|------|------:|-------------|
| `server_integration_test.rs` | 4 | Server data flow, restart handling, error recovery |
| `server_start_stop_test.rs` | 2 | Basic server lifecycle (start/stop cycles) |
| `library_integration_test.rs` | 7 | DrasiServerBuilder as embedded library, graceful shutdown |

### Configuration Tests (83 tests)

| File | Tests | Description |
|------|------:|-------------|
| `config_parsing_failure_test.rs` | 45 | Config validation - rejects snake_case fields, unknown fields, invalid values |
| `config_value_integration_test.rs` | 5 | ConfigValue with static values and environment variable resolution |
| `example_configs_validation_test.rs` | 29 | Validates all YAML configs in `examples/` directory |
| `readme_examples_validation_test.rs` | 4 | Validates YAML code blocks extracted from README.md |

### CLI Command Tests (37 tests)

| File | Tests | Description |
|------|------:|-------------|
| `init_output_test.rs` | 18 | Validates `drasi-server init` generates valid configs with camelCase fields |
| `validate_command_test.rs` | 19 | Tests `drasi-server validate` CLI command - valid/invalid configs, error messages |

### Storage Tests (35 tests)

| File | Tests | Description |
|------|------:|-------------|
| `persist_index_test.rs` | 13 | RocksDB persistent index provider creation and integration |
| `state_store_test.rs` | 14 | REDB state store provider - serialization, multi-instance configs |
| `redis_helpers_test.rs` | 8 | Redis helper utilities - CloudEvent building, platform integration |

### Serialization Tests (9 tests)

| File | Tests | Description |
|------|------:|-------------|
| `dto_camelcase_test.rs` | 3 | Verifies DTO fields serialize to camelCase (Postgres, Mock, HTTP) |
| `enum_serialization_test.rs` | 6 | Enum serialization for SourceConfig/ReactionConfig with flattened DTOs |

---

## Running Tests

### Run All Tests

```bash
cargo test
```

### Run by Category

```bash
# Unit tests only (tests inside src/)
cargo test --lib

# Integration tests only (tests in tests/)
cargo test --test '*'

# Doc tests only
cargo test --doc
```

### Run Specific Test Files

```bash
# Single test file
cargo test --test api_integration_test

# Multiple related test files
cargo test --test 'api_*'
cargo test --test 'server_*'
cargo test --test '*_validation_test'
```

### Run Specific Test Functions

```bash
# By exact name
cargo test test_create_and_delete_query

# By pattern
cargo test query
cargo test config
```

### Run with Options

```bash
# Show all output (including println!)
cargo test -- --nocapture

# Run tests sequentially (useful for debugging)
cargo test -- --test-threads=1

# Show which tests are running
cargo test -- --show-output
```

### Run with Logging

```bash
# Debug logging
RUST_LOG=debug cargo test -- --nocapture

# Trace logging for specific module
RUST_LOG=drasi_server::api=trace cargo test --test api_integration_test -- --nocapture
```

---

## Test Support Module

The `test_support/` directory provides shared utilities for integration tests:

```
test_support/
├── mod.rs              # Module exports
├── mock_components.rs  # MockSource and MockReaction implementations
├── config_helpers.rs   # Configuration test utilities
└── redis_helpers.rs    # Redis testcontainer setup
```

### Using Test Support in Your Tests

```rust
mod test_support;

use test_support::mock_components::{create_mock_source, create_mock_reaction};
use test_support::config_helpers::create_temp_config_file;
```

---

## Manual Tests

### PostgreSQL Integration Test

The `integration/getting-started/` directory contains an end-to-end test with a real PostgreSQL database.

**Location:** `tests/integration/getting-started/`

**Prerequisites:**
- PostgreSQL installed and running
- Database configured with the setup script

**How to Run:**

```bash
cd tests/integration/getting-started

# Set up the PostgreSQL database
./setup-postgres.sh

# Run the integration test
./run-integration-test.sh
```

**Configuration:** The test uses `config.yaml` which follows the current schema format.

---

## Helper Scripts

### `run_all_cargo_tests.sh`

Runs all Cargo tests with formatted output and summary.

```bash
./tests/run_all_cargo_tests.sh
```

**Features:**
- Builds the server first
- Runs tests in categories
- Provides pass/fail summary
- Color-coded output

### `run_all.sh`

Legacy test runner script. Note: Some referenced tests may not exist.

```bash
./tests/run_all.sh
```

---

## Directory Structure

```
tests/
├── api_contract_test.rs           # API contract validation
├── api_integration_test.rs        # API integration tests
├── api_persistence_test.rs        # Config persistence tests
├── api_query_joins_test.rs        # Query joins tests
├── api_state_consistency_test.rs  # State consistency tests
├── config_parsing_failure_test.rs # Config validation (snake_case rejection)
├── config_value_integration_test.rs # ConfigValue tests
├── dto_camelcase_test.rs          # DTO camelCase serialization
├── enum_serialization_test.rs     # Enum serialization tests
├── example_configs_validation_test.rs # Example config validation
├── init_output_test.rs            # Init command output tests
├── library_integration_test.rs    # Library mode tests
├── persist_index_test.rs          # RocksDB index tests
├── readme_examples_validation_test.rs # README YAML validation
├── redis_helpers_test.rs          # Redis utilities tests
├── server_integration_test.rs     # Server integration tests
├── server_start_stop_test.rs      # Server lifecycle tests
├── state_store_test.rs            # State store tests
├── validate_command_test.rs       # Validate CLI command tests
├── test_support/                  # Shared test utilities
│   ├── mod.rs
│   ├── mock_components.rs
│   ├── config_helpers.rs
│   └── redis_helpers.rs
├── integration/                   # Manual integration tests
│   └── getting-started/
│       ├── config.yaml
│       ├── run-integration-test.sh
│       ├── setup-postgres.sh
│       └── README.md
├── run_all.sh                     # Test runner script
├── run_all_cargo_tests.sh         # Cargo test wrapper
└── README.md                      # This file
```

---

## Writing New Tests

### Adding an Integration Test

1. Create a new file in `tests/` with the `_test.rs` suffix:

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

2. Run your test:

```bash
cargo test --test my_feature_test
```

### Adding a Unit Test

Add tests in the source file using a `tests` module:

```rust
// src/my_module.rs

pub fn my_function() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function() {
        assert!(my_function());
    }
}
```

### Test Naming Conventions

- **Files:** Use `_test.rs` suffix (e.g., `my_feature_test.rs`)
- **Functions:** Use `test_` prefix with descriptive names
- **Example:** `test_create_query_returns_error_for_invalid_config`

### Best Practices

1. **Use test_support:** Import shared mocks from `test_support/`
2. **Async tests:** Use `#[tokio::test]` for async functions
3. **Isolation:** Each test should be independent and not rely on other tests
4. **Cleanup:** Use `tempfile` crate for temporary files that auto-cleanup
5. **Timeouts:** Add timeouts for operations that could hang
6. **Assertions:** Use descriptive assertion messages

---

## Troubleshooting

### Port Conflicts

Tests may use ports 8080, 9000, 50051, 50052. If you see "address in use" errors:

```bash
# Find process using a port
lsof -i :8080

# Kill a specific process
kill <PID>
```

### Test Isolation Failures

If tests interfere with each other, run them sequentially:

```bash
cargo test -- --test-threads=1
```

### Redis Tests Failing

Some tests require Redis. Skip them if Redis is not available:

```bash
cargo test -- --skip redis
```

### RocksDB Lock Errors

Clean up stale lock files:

```bash
rm -rf /tmp/drasi_test_*
```

### Seeing Test Output

By default, Rust captures test output. To see it:

```bash
cargo test -- --nocapture
```

---

## CI/CD Integration

Example GitHub Actions workflow:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Run Tests
        run: cargo test
        
      - name: Run Clippy
        run: cargo clippy --all-targets --all-features
```

---

## Additional Resources

- [Main README](../README.md) - Repository overview and usage
- [CLAUDE.md](../CLAUDE.md) - Development context and conventions
- [PostgreSQL Integration Test](integration/getting-started/README.md) - E2E test documentation
