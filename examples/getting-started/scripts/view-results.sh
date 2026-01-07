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

# View Results Script
# Queries the Drasi Server API for current query results

set -e

API_HOST="${API_HOST:-localhost}"
API_PORT="${API_PORT:-8080}"
QUERY_ID="${1:-}"

# Check for jq (optional but recommended)
if command -v jq &> /dev/null; then
    FORMAT_CMD="jq ."
else
    FORMAT_CMD="cat"
fi

# Function to get query results
get_results() {
    local query_id=$1
    echo "=== Query: $query_id ==="
    curl -s "http://${API_HOST}:${API_PORT}/api/v1/queries/${query_id}/results" | $FORMAT_CMD
    echo
}

# Function to get query status
get_status() {
    local query_id=$1
    echo "=== Status: $query_id ==="
    curl -s "http://${API_HOST}:${API_PORT}/api/v1/queries/${query_id}" | $FORMAT_CMD
    echo
}

# Main logic
if [ -n "$QUERY_ID" ]; then
    # View specific query results
    get_results "$QUERY_ID"
else
    # View all query results
    echo "=== Drasi Server Query Results ==="
    echo "API: http://${API_HOST}:${API_PORT}"
    echo

    for query in "hello-world-from" "message-count" "inactive-people"; do
        get_results "$query"
    done

    echo "Tip: Use '$0 <query-id>' to view a specific query"
    echo "     Use '$0 status' to view query statuses"
fi

# Handle status command
if [ "$QUERY_ID" = "status" ]; then
    echo "=== Query Statuses ==="
    for query in "hello-world-from" "message-count" "inactive-people"; do
        get_status "$query"
    done
fi
