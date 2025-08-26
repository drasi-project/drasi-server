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

# Clean up all dynamically created queries and reactions

echo "Cleaning up all queries and reactions..."

BASE_URL="http:#localhost:8080"

# Delete all queries
QUERIES=$(curl -s "$BASE_URL/queries" | jq -r '.data[]?.id')
for query in $QUERIES; do
    echo "Deleting query: $query"
    curl -X DELETE "$BASE_URL/queries/$query" 2>/dev/null
done

# Delete all reactions
REACTIONS=$(curl -s "$BASE_URL/reactions" | jq -r '.data[]?.id')
for reaction in $REACTIONS; do
    echo "Deleting reaction: $reaction"
    curl -X DELETE "$BASE_URL/reactions/$reaction" 2>/dev/null
done

echo "Cleanup complete!"