---
description: Validates YAML snippets in markdown files against Rust models and runtime behavior
on:
  pull_request:
    paths:
      - '**.md'
  workflow_dispatch:
permissions:
  contents: read
  pull-requests: read
  issues: read
tools:
  github:
    toolsets: [default]
  bash:
    - "find . -name"
    - "cat"
steps:
  - name: Build server and run config validation tests
    run: |
      set +e
      mkdir -p /tmp/gh-aw/agent
      sudo apt-get update -y
      RESULTS=/tmp/gh-aw/agent/validation-results.txt
      : > "$RESULTS"
      run_step() {
        local title="$1"; shift
        echo "## $title" >> "$RESULTS"
        echo '```' >> "$RESULTS"
        "$@" >> "$RESULTS" 2>&1
        local rc=$?
        echo '```' >> "$RESULTS"
        echo "exit_code=$rc" >> "$RESULTS"
        echo >> "$RESULTS"
      }
      echo "# Runtime validation results" >> "$RESULTS"
      echo >> "$RESULTS"
      run_step "install build dependencies" sudo apt-get install -y libjq-dev libonig-dev protobuf-compiler
      export JQ_LIB_DIR=/usr/lib/x86_64-linux-gnu
      run_step "cargo test --test readme_examples_validation_test" cargo test --test readme_examples_validation_test
      run_step "cargo test --test example_configs_validation_test" cargo test --test example_configs_validation_test
      run_step "cargo test --test config_parsing_failure_test" cargo test --test config_parsing_failure_test
      cat "$RESULTS"
safe-outputs:
  add-comment:
    max: 1
timeout-minutes: 30
---

# YAML Snippet Validator

You are an AI agent that validates YAML configuration snippets found in markdown files against the Drasi Server's Rust models and runtime behavior.

## Your Task

1. **Find all markdown files** in the repository that contain YAML code blocks (look for ```yaml or ```yml fenced code blocks)

2. **Extract YAML snippets** that appear to be Drasi Server configuration examples (look for config files with fields like `sources:`, `queries:`, `reactions:`, `instances:`, etc.)

3. **Validate against Rust models**:
   - Examine the Rust structs in `src/api/models/` to understand the expected schema
   - Check if the YAML fields match the serde-serialized Rust types
   - Look for common issues: typos, wrong types, missing required fields, extra fields

4. **Review runtime validation results**:
   - A setup step has already run the repository's existing
     snippet/config validation tests that cover the documented examples
     (`readme_examples_validation_test`, `example_configs_validation_test`,
     `config_parsing_failure_test`).
   - Their combined output is saved at `/tmp/gh-aw/agent/validation-results.txt`,
     with an `exit_code=` line after each command (`exit_code=0` means it passed;
     any non-zero value means it failed).
   - Read that file and incorporate the results into your findings. If the file
     is missing or a section reports a non-zero exit code, note it as a finding
     and quote the relevant failing output.

5. **Report findings**:
   - If all YAML snippets are valid, comment: "✅ All YAML snippets validated successfully!"
   - If issues are found, list each problem with:
     - File path and line number
     - The problematic YAML snippet
     - What's wrong (field name mismatch, type error, runtime error)
     - Suggested fix based on the Rust models

## Guidelines

- Focus on YAML snippets that look like server configuration (not random YAML)
- Be specific about which Rust struct the YAML should match
- If a snippet is intentionally incomplete or an example fragment, note that
- For runtime validation, use the pre-computed results file at `/tmp/gh-aw/agent/validation-results.txt`
- Only comment if you find issues OR if explicitly running validation on valid configs
- Be concise and actionable in your feedback
