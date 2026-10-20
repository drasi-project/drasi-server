#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail

case "${MODE:-}" in
  override) VERSION_ARGS=("${OVERRIDE:?OVERRIDE is required}") ;;
  bump)
    case "${BUMP:-patch}" in
      major|minor|patch) VERSION_ARGS=(--bump "${BUMP:-patch}") ;;
      *) echo "::error::Invalid bump level" >&2; exit 1 ;;
    esac
    ;;
  *) echo "::error::MODE must be override or bump" >&2; exit 1 ;;
esac

# Keep version-bump and rehearsal on the same policy, including dev/build deps.
METADATA=$(cargo metadata --no-deps --format-version 1)
if ! PACKAGES=$(jq -er '[.packages[].dependencies[].name | select(startswith("drasi-"))] | unique | .[]' <<< "$METADATA"); then
  echo "::error::Could not find drasi-* dependencies in workspace metadata" >&2
  exit 1
fi
UPDATE_ARGS=()
while IFS= read -r package; do
  UPDATE_ARGS+=(--package "$package")
done <<< "$PACKAGES"

cargo upgrade --incompatible "${UPDATE_ARGS[@]}"
cargo update "${UPDATE_ARGS[@]}"
cargo set-version --package drasi-server "${VERSION_ARGS[@]}"
# Preserve unrelated locked dependencies and other workspace package versions.
cargo update --workspace

NEW_VERSION=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
echo "New drasi-server version: $NEW_VERSION"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "new_version=$NEW_VERSION" >> "$GITHUB_OUTPUT"
fi
