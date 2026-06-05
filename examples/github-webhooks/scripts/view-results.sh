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

# View Query Results
# Fetches current results from all queries via the REST API

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."

# Load environment variables
if [ -f "$EXAMPLE_DIR/.env" ]; then
    set -a
    source "$EXAMPLE_DIR/.env"
    set +a
fi

API_URL="http://localhost:${SERVER_PORT:-8080}/api/v1"

echo "=== GitHub Webhooks - Query Results ==="
echo

echo "--- Recent Pushes ---"
curl -s "$API_URL/queries/recent-pushes/results" | python3 -m json.tool 2>/dev/null || curl -s "$API_URL/queries/recent-pushes/results"
echo

echo "--- Open Pull Requests ---"
curl -s "$API_URL/queries/open-pull-requests/results" | python3 -m json.tool 2>/dev/null || curl -s "$API_URL/queries/open-pull-requests/results"
echo

echo "--- Issues Opened ---"
curl -s "$API_URL/queries/issues-opened/results" | python3 -m json.tool 2>/dev/null || curl -s "$API_URL/queries/issues-opened/results"
echo
