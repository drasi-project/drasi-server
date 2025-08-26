#!/bin/bash

# Run all working tests for Drasi Server
# These tests don't require the Python SDK and should work out of the box

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE} Drasi Server Working Tests${NC}"
echo -e "${BLUE}================================${NC}"
echo ""

# Keep track of test results
PASSED=0
FAILED=0
FAILED_TESTS=""

# Function to run a test
run_test() {
    local test_name=$1
    local test_script=$2
    
    echo -e "${YELLOW}Running: ${test_name}${NC}"
    
    if bash "$test_script"; then
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
cargo build --release
echo ""

# Bootstrap Tests
echo -e "${BLUE}=== Bootstrap Tests ===${NC}"
run_test "Basic Bootstrap" "tests/bootstrap/test_bootstrap.sh"
run_test "Bootstrap API" "tests/bootstrap/test_bootstrap_api.sh"
run_test "Mock Bootstrap" "tests/bootstrap/test_mock_bootstrap.sh"

# Integration Tests
echo -e "${BLUE}=== Integration Tests ===${NC}"
run_test "Internal Sources" "tests/integration/test_internal_sources.sh"
run_test "HTTP Source" "tests/integration/test_http_source.sh"

# Rust SDK Tests
if [ -d "tests/sdk/rust" ] && [ "$(ls -A tests/sdk/rust)" ]; then
    echo -e "${BLUE}=== Rust SDK Tests ===${NC}"
    run_test "Rust SDK" "tests/sdk/test_rust_sdk.sh"
fi

# PostgreSQL Tests (if PostgreSQL is available)
if command -v psql &> /dev/null; then
    echo -e "${BLUE}=== PostgreSQL Tests ===${NC}"
    run_test "PostgreSQL Setup" "tests/postgres/scripts/test_setup.sh"
else
    echo -e "${YELLOW}Skipping PostgreSQL tests (psql not found)${NC}"
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
    echo -e "${GREEN}All tests passed!${NC}"
fi