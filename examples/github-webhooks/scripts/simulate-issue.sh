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

# Simulate a GitHub Issues Event
# Sends a realistic issues webhook payload to the local Drasi Server

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."

# Load environment variables
if [ -f "$EXAMPLE_DIR/.env" ]; then
    set -a
    source "$EXAMPLE_DIR/.env"
    set +a
fi

WEBHOOK_URL="http://localhost:${WEBHOOK_PORT:-9000}/github/events"
SECRET="${GITHUB_WEBHOOK_SECRET:-my-webhook-secret}"

# Simulated issue event payload
PAYLOAD=$(cat <<'EOF'
{
  "action": "opened",
  "issue": {
    "id": 112233445,
    "number": 38,
    "state": "open",
    "title": "Dashboard is hard to read in low-light environments",
    "user": {
      "login": "monalisa",
      "id": 2,
      "avatar_url": "https://avatars.githubusercontent.com/u/2?v=4"
    },
    "body": "When using the dashboard at night, the bright white background causes eye strain.\n\nWould be great to have a dark mode option.",
    "created_at": "2025-01-14T16:00:00Z",
    "updated_at": "2025-01-14T16:00:00Z",
    "html_url": "https://github.com/octocat/hello-world/issues/38",
    "labels": [
      {"name": "enhancement"},
      {"name": "ui"}
    ]
  },
  "repository": {
    "id": 12345,
    "full_name": "octocat/hello-world",
    "html_url": "https://github.com/octocat/hello-world"
  },
  "sender": {
    "login": "monalisa",
    "id": 2
  }
}
EOF
)

# Compute HMAC-SHA256 signature
SIGNATURE="sha256=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')"

echo "Sending issues event to $WEBHOOK_URL"
echo "  Repository: octocat/hello-world"
echo "  Issue #38: Dashboard is hard to read in low-light environments"
echo "  Author: monalisa"
echo "  Action: opened"
echo

curl -s -w "\nHTTP Status: %{http_code}\n" \
  -X POST "$WEBHOOK_URL" \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: issues" \
  -H "X-Hub-Signature-256: $SIGNATURE" \
  -H "X-GitHub-Delivery: $(uuidgen 2>/dev/null || echo "test-delivery-$(date +%s)")" \
  -d "$PAYLOAD"
