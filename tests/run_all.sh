#!/bin/bash

# Run all tests for Drasi Server

set -e

echo "🧪 Drasi Server Test Suite"
echo "========================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TOTAL=0
PASSED=0
FAILED=0

# Function to run a test
run_test() {
    local test_name=$1
    local test_script=$2
    
    echo -n "Running $test_name... "
    TOTAL=$((TOTAL + 1))
    
    if timeout 60s $test_script > logs/test_output.log 2>&1; then
        echo -e "${GREEN}✅ PASSED${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}❌ FAILED${NC}"
        FAILED=$((FAILED + 1))
        echo "  Output:"
        tail -n 20 logs/test_output.log | sed 's/^/    /'
    fi
}

# Check prerequisites
echo "Checking prerequisites..."
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Cargo not found${NC}"
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo -e "${YELLOW}⚠️  Python3 not found - some tests will be skipped${NC}"
fi

echo ""
echo "Running Integration Tests:"
echo "--------------------------"

# Integration tests
if [ -f "tests/integration/test_pipeline.sh" ]; then
    run_test "Pipeline Test" "tests/integration/test_pipeline.sh"
fi

if [ -f "tests/integration/test_internal_sources.sh" ]; then
    run_test "Internal Sources" "tests/integration/test_internal_sources.sh"
fi

if [ -f "tests/integration/test_source_change.sh" ]; then
    run_test "Source Changes" "tests/integration/test_source_change.sh"
fi

echo ""
echo "Running SDK Tests:"
echo "------------------"

# SDK tests
if [ -f "tests/sdk/test_rust_sdk.sh" ]; then
    run_test "Rust SDK" "tests/sdk/test_rust_sdk.sh"
fi

echo ""
echo "Test Summary:"
echo "============="
echo -e "Total:  $TOTAL"
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"

if [ $FAILED -eq 0 ]; then
    echo -e "\n${GREEN}🎉 All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}❌ Some tests failed${NC}"
    exit 1
fi