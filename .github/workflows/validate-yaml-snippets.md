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
      run_step "cargo build --release" cargo build --release
      run_step "cargo test --test readme_examples_validation_test" cargo test --test readme_examples_validation_test
      run_step "cargo test --test example_configs_validation_test" cargo test --test example_configs_validation_test
      run_step "cargo test --test config_parsing_failure_test" cargo test --test config_parsing_failure_test
      cat "$RESULTS"
safe-outputs:
  add-comment:
    max: 1
timeout-minutes: 15
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

4. **Review runtime validation results** (already computed for you):
   - Before you started, a deterministic setup step built the server
     (`cargo build --release`) and ran the repository's existing snippet/config
     validation tests, which already cover the documented examples:
     - `cargo test --test readme_examples_validation_test`
     - `cargo test --test example_configs_validation_test`
     - `cargo test --test config_parsing_failure_test`
   - Read the results file at `/tmp/gh-aw/agent/validation-results.txt`. Each
     section shows a command, its full output, and an `exit_code` line
     (`exit_code=0` means it passed; any non-zero value means it failed).
   - Base your runtime findings on this file. **Do NOT attempt to run `cargo`,
     build, or start the server yourself** — shell execution of build/test
     commands is blocked in your environment, and the results are already
     provided for you. If the file is missing, or any section reports a
     non-zero `exit_code`, surface that as a finding (quote the relevant
     failing output).

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
- For runtime validation, rely on the pre-computed results file at `/tmp/gh-aw/agent/validation-results.txt`; do not run or start the server yourself
- Only comment if you find issues OR if explicitly running validation on valid configs
- Be concise and actionable in your feedback
