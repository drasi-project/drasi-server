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

set -euo pipefail

BASE_URL="${DRASI_SERVER_URL:-http://localhost:8280}"
INSTANCE_ID="${DRASI_INSTANCE_ID:-trading-server}"
INSTANCE_PATH="$(jq -rn --arg id "$INSTANCE_ID" '$id | @uri')"
API_URL="${BASE_URL%/}/api/v1/instances/${INSTANCE_PATH}"
echo "Cleaning up queries and reactions in instance '$INSTANCE_ID'..."

for kind in reactions queries; do
    response="$(curl -fsS "$API_URL/$kind")"
    jq -e '.success == true and (.data | type == "array")' <<<"$response" >/dev/null
    while IFS= read -r id; do
        encoded_id="$(jq -rn --arg id "$id" '$id | @uri')"
        echo "Deleting $kind/$id"
        result="$(curl -fsS -X DELETE "$API_URL/$kind/$encoded_id")"
        jq -e '.success == true' <<<"$result" >/dev/null
    done < <(jq -r '.data[].id' <<<"$response")
done

echo "Cleanup complete!"