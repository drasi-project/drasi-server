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

# Simulate a GitHub Push Event
# Sends a realistic push webhook payload to the local Drasi Server

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

# Simulated push event payload
PAYLOAD=$(cat <<'EOF'
{
  "ref": "refs/heads/main",
  "before": "abc123def456",
  "after": "def456ghi789",
  "repository": {
    "id": 12345,
    "full_name": "octocat/hello-world",
    "html_url": "https://github.com/octocat/hello-world"
  },
  "pusher": {
    "name": "octocat",
    "email": "octocat@github.com"
  },
  "size": 1,
  "head_commit": {
    "id": "def456ghi789abc123def456ghi789abc123def45",
    "message": "feat: add new feature for user notifications",
    "timestamp": "2025-01-15T10:30:00Z",
    "url": "https://github.com/octocat/hello-world/commit/def456ghi789",
    "author": {
      "name": "The Octocat",
      "email": "octocat@github.com",
      "username": "octocat"
    }
  }
}
EOF
)

# Compute HMAC-SHA256 signature
SIGNATURE="sha256=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')"

echo "Sending push event to $WEBHOOK_URL"
echo "  Repository: octocat/hello-world"
echo "  Branch: refs/heads/main"
echo "  Commit: feat: add new feature for user notifications"
echo

curl -s -w "\nHTTP Status: %{http_code}\n" \
  -X POST "$WEBHOOK_URL" \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-Hub-Signature-256: $SIGNATURE" \
  -H "X-GitHub-Delivery: $(uuidgen 2>/dev/null || echo "test-delivery-$(date +%s)")" \
  -d "$PAYLOAD"
