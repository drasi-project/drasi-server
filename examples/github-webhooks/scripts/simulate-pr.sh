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

# Simulate a GitHub Pull Request Event
# Sends a realistic pull_request webhook payload to the local Drasi Server

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

# Simulated pull request event payload
PAYLOAD=$(cat <<'EOF'
{
  "action": "opened",
  "number": 42,
  "pull_request": {
    "id": 987654321,
    "number": 42,
    "state": "open",
    "title": "Add dark mode support to the dashboard",
    "user": {
      "login": "octocat",
      "id": 1,
      "avatar_url": "https://avatars.githubusercontent.com/u/1?v=4"
    },
    "body": "This PR adds dark mode support to the main dashboard.\n\nCloses #38",
    "created_at": "2025-01-15T11:00:00Z",
    "updated_at": "2025-01-15T11:00:00Z",
    "html_url": "https://github.com/octocat/hello-world/pull/42",
    "head": {
      "ref": "feature/dark-mode",
      "sha": "abc123def456ghi789"
    },
    "base": {
      "ref": "main",
      "sha": "def456ghi789abc123"
    }
  },
  "repository": {
    "id": 12345,
    "full_name": "octocat/hello-world",
    "html_url": "https://github.com/octocat/hello-world"
  },
  "sender": {
    "login": "octocat",
    "id": 1
  }
}
EOF
)

# Compute HMAC-SHA256 signature
SIGNATURE="sha256=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')"

echo "Sending pull_request event to $WEBHOOK_URL"
echo "  Repository: octocat/hello-world"
echo "  PR #42: Add dark mode support to the dashboard"
echo "  Author: octocat"
echo "  Branch: feature/dark-mode -> main"
echo

curl -s -w "\nHTTP Status: %{http_code}\n" \
  -X POST "$WEBHOOK_URL" \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: pull_request" \
  -H "X-Hub-Signature-256: $SIGNATURE" \
  -H "X-GitHub-Delivery: $(uuidgen 2>/dev/null || echo "test-delivery-$(date +%s)")" \
  -d "$PAYLOAD"
