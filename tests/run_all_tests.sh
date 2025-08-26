#!/bin/bash

# Run all integration tests
set -e

echo "=== Running Unit Tests ==="
cargo test --lib

echo ""
echo "=== Running Integration Tests ==="

# Test pipeline
if [ -f "./test/integration/test_pipeline.sh" ]; then
    echo "Running pipeline test..."
    ./test/integration/test_pipeline.sh
fi

# Test Python SDK
if [ -f "./test/sdk/test_python_sdk.sh" ]; then
    echo "Running Python SDK test..."
    ./test/sdk/test_python_sdk.sh
fi

# Test Rust SDK
if [ -f "./test/sdk/test_rust_sdk.sh" ]; then
    echo "Running Rust SDK test..."
    ./test/sdk/test_rust_sdk.sh
fi

echo ""
echo "=== All Tests Passed! ==="