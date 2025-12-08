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

# Delete Message Script
# Deletes a message from the database by ID

set -e

MESSAGE_ID="${1:-}"

# Usage
if [ -z "$MESSAGE_ID" ]; then
    echo "Usage: $0 <messageid>"
    echo
    echo "Example:"
    echo "  $0 2    # Deletes message with ID 2 (Brian Kernighan's 'Hello World')"
    echo
    echo "Current messages:"
    docker exec getting-started-postgres psql -U drasi_user -d getting_started -c \
        "SELECT messageid, \"from\", message FROM message ORDER BY messageid;"
    exit 1
fi

echo "Deleting message with ID: $MESSAGE_ID"

# Show the message being deleted
echo "Message to delete:"
docker exec getting-started-postgres psql -U drasi_user -d getting_started -c \
    "SELECT messageid, \"from\", message FROM message WHERE messageid = $MESSAGE_ID;"

# Delete the message
DELETED=$(docker exec getting-started-postgres psql -U drasi_user -d getting_started -tAc \
    "DELETE FROM message WHERE messageid = $MESSAGE_ID RETURNING messageid;")

if [ -n "$DELETED" ]; then
    echo "Message deleted successfully!"
    echo "Check the Drasi Server console or SSE stream for query updates."
else
    echo "No message found with ID: $MESSAGE_ID"
fi
