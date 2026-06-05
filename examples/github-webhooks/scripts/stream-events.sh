#!/bin/bash
# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Stream SSE Events
# Connects to the SSE reaction and streams GitHub events in real-time

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."

# Load environment variables
if [ -f "$EXAMPLE_DIR/.env" ]; then
    set -a
    source "$EXAMPLE_DIR/.env"
    set +a
fi

SSE_URL="http://localhost:${SSE_PORT:-8081}/events"

echo "=== Streaming GitHub Events (SSE) ==="
echo "Connecting to $SSE_URL..."
echo "Press Ctrl+C to stop"
echo "=============================================="
echo

curl -s -N "$SSE_URL"
