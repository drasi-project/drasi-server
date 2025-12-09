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

# Open Viewer Script
# Serves the viewer via HTTP and opens it in the default browser.
# This is needed because browsers block cross-origin SSE requests from file:// URLs.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VIEWER_DIR="$SCRIPT_DIR/../viewer"
PORT=5173

# Check if port is already in use
if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "Viewer already running at http://localhost:$PORT"
    open "http://localhost:$PORT"
    exit 0
fi

echo "Starting viewer server on port $PORT..."
cd "$VIEWER_DIR"

# Start Python HTTP server in background
python3 -m http.server $PORT &
SERVER_PID=$!

# Give it a moment to start
sleep 1

# Open in browser
echo "Opening http://localhost:$PORT in your browser..."
open "http://localhost:$PORT"

echo ""
echo "Viewer running at: http://localhost:$PORT"
echo "Press Ctrl+C to stop the viewer server"
echo ""

# Wait for the server (allows Ctrl+C to kill it)
wait $SERVER_PID
