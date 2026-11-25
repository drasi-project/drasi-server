#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail
IMAGE="${1:?Usage: smoke-release-container.sh IMAGE@DIGEST SHA ARCH OUTPUT_DIR}"
SHA="${2:?Candidate SHA required}"
ARCH="${3:?Architecture required}"
OUTPUT="${4:?Evidence directory required}"
mkdir -p "$OUTPUT"
CONTAINER=
fail() {
  echo "::error::$*" >&2
  exit 1
}
cleanup() {
  local status=$?
  if [[ -n "$CONTAINER" ]]; then
    if ! docker logs "$CONTAINER" > "$OUTPUT/container.log" 2>&1; then
      echo "::error::Could not collect container logs" >&2
      status=1
    fi
    if ! docker rm -f "$CONTAINER" > /dev/null; then
      echo "::error::Could not remove smoke container $CONTAINER" >&2
      status=1
    fi
  fi
  exit "$status"
}
trap cleanup EXIT

[[ "$IMAGE" =~ @sha256:[a-f0-9]{64}$ ]] || fail "An immutable image digest is required"
docker pull --platform "linux/$ARCH" "$IMAGE"
[[ "$(docker image inspect --format '{{.Architecture}}' "$IMAGE")" == "$ARCH" ]] \
  || fail "Pulled image architecture differs from $ARCH"
[[ "$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$IMAGE")" == "$SHA" ]] \
  || fail "Pulled image revision differs from the candidate SHA"
USER_ID=$(docker image inspect --format '{{.Config.User}}' "$IMAGE")
[[ -n "$USER_ID" && "$USER_ID" != root && "$USER_ID" != 0 && "$USER_ID" != 0:* && "$USER_ID" != root:* ]] \
  || fail "Runtime image must declare a non-root user"

# First exercise precisely the shipped USER, ENTRYPOINT and CMD with no mounts.
CONTAINER=$(docker run -d -p 127.0.0.1::8080 "$IMAGE")
PORT=$(docker port "$CONTAINER" 8080/tcp | cut -d: -f2)
python3 tests/integration/hello-world/assert-api.py \
  --url "http://127.0.0.1:$PORT" --default-startup --output "$OUTPUT/default.json"
[[ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER")" == true ]] \
  || fail "Default runtime exited during smoke tests"
docker logs "$CONTAINER" > "$OUTPUT/default.log" 2>&1
docker rm -f "$CONTAINER" > /dev/null
CONTAINER=

# Only the configuration changes. No root, entrypoint override or test-only libraries.
CONTAINER=$(docker run -d -p 127.0.0.1::8080 \
  --mount "type=bind,source=$(pwd)/tests/integration/hello-world/config.yaml,target=/smoke.yaml,readonly" \
  "$IMAGE" --config /smoke.yaml)
PORT=$(docker port "$CONTAINER" 8080/tcp | cut -d: -f2)
python3 tests/integration/hello-world/assert-api.py \
  --url "http://127.0.0.1:$PORT" --output "$OUTPUT/runtime.json"
[[ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER")" == true ]] \
  || fail "Hello-world runtime exited during smoke tests"
