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
    - "cargo build --release"
    - "cargo run -- --config *"
    - "find . -name '*.md' -type f"
    - "cat *"
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

4. **Validate at runtime** (where possible):
   - Build the drasi-server: `cargo build --release`
   - For each valid-looking config snippet, save it to a temporary file
   - Try to start the server with that config: `cargo run -- --config /path/to/temp-config.yaml`
   - Check if the server starts without errors or if it reports validation issues

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
- For runtime validation, use a short timeout (e.g., 5 seconds) to see if the server starts
- Only comment if you find issues OR if explicitly running validation on valid configs
- Be concise and actionable in your feedback
