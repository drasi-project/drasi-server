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

# Add Message Script
# Inserts a new message into the database

set -e

# Default values
FROM_NAME="${1:-}"
MESSAGE_TEXT="${2:-}"

# Usage
if [ -z "$FROM_NAME" ] || [ -z "$MESSAGE_TEXT" ]; then
    echo "Usage: $0 <from_name> <message_text>"
    echo
    echo "Examples:"
    echo "  $0 'Alice' 'Hello World'        # Matches hello-world-from query"
    echo "  $0 'Bob' 'Goodbye World'        # Updates message-count only"
    echo "  $0 'Charlie' 'I am Spartacus'   # Matches existing message pattern"
    exit 1
fi

echo "Adding message from '$FROM_NAME': '$MESSAGE_TEXT'"

# Insert message
docker exec getting-started-postgres psql -U drasi_user -d getting_started -c \
    "INSERT INTO message (\"from\", message) VALUES ('$FROM_NAME', '$MESSAGE_TEXT') RETURNING messageid, \"from\", message, created_at;"

echo
echo "Message added successfully!"
echo "Check the Drasi Server console or SSE stream for query updates."
