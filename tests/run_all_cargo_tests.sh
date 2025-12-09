#!/bin/bash

# Run all Cargo tests for Drasi Server
# This script runs the complete automated Rust test suite

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE} Drasi Server Test Suite${NC}"
echo -e "${BLUE}================================${NC}"
echo ""

# Keep track of test results
PASSED=0
FAILED=0
FAILED_TESTS=""

# Function to run a test
run_test() {
    local test_name=$1
    local test_command=$2

    echo -e "${YELLOW}Running: ${test_name}${NC}"

    if eval "$test_command"; then
        echo -e "${GREEN}✓ ${test_name} passed${NC}"
        ((PASSED++))
    else
        echo -e "${RED}✗ ${test_name} failed${NC}"
        ((FAILED++))
        FAILED_TESTS="${FAILED_TESTS}\n  - ${test_name}"
    fi
    echo ""
}

# Build server first
echo -e "${BLUE}Building Drasi Server...${NC}"
cargo build
echo ""

# Run Rust Tests
echo -e "${BLUE}=== Rust Unit and Integration Tests ===${NC}"
echo ""

# Library tests (unit tests in src/)
run_test "Library Unit Tests" "cargo test --lib --quiet"

# Integration tests (tests/*.rs and tests/api/*.rs)
run_test "API Tests" "cargo test --test api --quiet"
run_test "API Create Query Joins Test" "cargo test --test api_create_query_joins --quiet"
run_test "Library Integration Tests" "cargo test --test library_integration --quiet"
run_test "Server Integration Tests" "cargo test --test server_integration_test --quiet"
run_test "Server Start/Stop Tests" "cargo test --test server_start_stop_test --quiet"

# Optional: Run all tests at once as verification
echo -e "${BLUE}=== Running All Tests Together (Verification) ===${NC}"
run_test "All Tests (Comprehensive)" "cargo test --quiet"

# gRPC Protocol Tests (if available)
if [ -f "tests/grpc/run_test.sh" ]; then
    echo -e "${BLUE}=== gRPC Protocol Tests ===${NC}"
    echo -e "${YELLOW}Note: These require manual setup and running server${NC}"
    echo -e "${YELLOW}Run manually with: cd tests/grpc && ./run_test.sh${NC}"
    echo ""
fi

# HTTP Protocol Tests (if available)
if [ -f "tests/http/run_test.sh" ]; then
    echo -e "${BLUE}=== HTTP Protocol Tests ===${NC}"
    echo -e "${YELLOW}Note: These require manual setup and running server${NC}"
    echo -e "${YELLOW}Run manually with: cd tests/http && ./run_test.sh${NC}"
    echo ""
fi

# Summary
echo -e "${BLUE}================================${NC}"
echo -e "${BLUE} Test Summary${NC}"
echo -e "${BLUE}================================${NC}"
echo -e "${GREEN}Passed: ${PASSED}${NC}"
echo -e "${RED}Failed: ${FAILED}${NC}"

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed tests:${FAILED_TESTS}${NC}"
    exit 1
else
    echo -e "${GREEN}All automated tests passed!${NC}"
    echo ""
    echo -e "${BLUE}Additional Manual Tests Available:${NC}"
    echo -e "  - gRPC protocol tests: tests/grpc/run_test.sh"
    echo -e "  - HTTP protocol tests: tests/http/run_test.sh"
    echo -e "  - SSE console utility: tests/sse-console/ (Node.js)"
    echo -e "  - Interactive demo: tests/run_interactive_demo.sh"
fi
