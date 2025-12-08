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

# Stream Events Script
# Connects to the SSE stream and displays events in the terminal

set -e

SSE_HOST="${SSE_HOST:-localhost}"
SSE_PORT="${SSE_PORT:-8081}"
SSE_PATH="${SSE_PATH:-/events}"

echo "=== Drasi Server SSE Stream ==="
echo "Connecting to: http://${SSE_HOST}:${SSE_PORT}${SSE_PATH}"
echo "Press Ctrl+C to stop"
echo "================================"
echo

# Connect to SSE stream
# The -N flag prevents curl from buffering and shows events in real-time
curl -N -s "http://${SSE_HOST}:${SSE_PORT}${SSE_PATH}" | while read -r line; do
    # Skip empty lines
    if [ -n "$line" ]; then
        # Add timestamp prefix for readability
        echo "[$(date '+%H:%M:%S')] $line"
    fi
done
