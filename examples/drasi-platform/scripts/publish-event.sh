#!/bin/bash
# Publish test events to Redis stream for the Drasi Platform example

# Default values
REDIS_HOST="localhost"
REDIS_PORT="6379"
STREAM_KEY="hello-world-change"
MESSAGE_FROM="${1:-Alice}"

echo "==================================="
echo "Drasi Platform Example - Publish Event"
echo "==================================="
echo ""

# Check if redis-cli is available
if ! command -v redis-cli &> /dev/null; then
    echo "ERROR: redis-cli is not installed or not in PATH"
    echo ""
    echo "Options:"
    echo "  1. Install redis-cli:"
    echo "     - macOS: brew install redis"
    echo "     - Ubuntu/Debian: sudo apt-get install redis-tools"
    echo "     - RHEL/CentOS: sudo yum install redis"
    echo ""
    echo "  2. Use Docker:"
    echo "     docker exec -it drasi-redis redis-cli XADD $STREAM_KEY '*' ..."
    exit 1
fi

# Check if Redis is accessible
if ! redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping &> /dev/null; then
    echo "ERROR: Cannot connect to Redis at $REDIS_HOST:$REDIS_PORT"
    echo ""
    echo "Make sure Redis is running:"
    echo "  ./examples/drasi-platform/scripts/setup-redis.sh"
    exit 1
fi

# Generate unique message ID with timestamp
MESSAGE_ID="msg-$(date +%s)"
TIMESTAMP=$(date +%s%3N)

# Create platform-formatted JSON event
# The platform source expects this specific structure:
# - type: "i" (insert), "u" (update), "d" (delete)
# - elementType: "node" or "rel"
# - time: timestamp object with "ms" field
# - after: element data (or "before" for deletes)
EVENT_JSON=$(cat <<EOF
{
  "type": "i",
  "elementType": "node",
  "time": {"ms": $TIMESTAMP},
  "after": {
    "id": "$MESSAGE_ID",
    "labels": ["Message"],
    "properties": {
      "MessageId": "$MESSAGE_ID",
      "Message": "Hello World",
      "From": "$MESSAGE_FROM"
    }
  }
}
EOF
)

# Compact the JSON (remove newlines and extra spaces)
COMPACT_JSON=$(echo "$EVENT_JSON" | jq -c .)

echo "Publishing event to Redis stream..."
echo "  Stream: $STREAM_KEY"
echo "  Message ID: $MESSAGE_ID"
echo "  From: $MESSAGE_FROM"
echo ""

# Publish to Redis stream using XADD
redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" XADD "$STREAM_KEY" "*" "event" "$COMPACT_JSON" > /dev/null

echo "✓ Event published successfully!"
echo ""
echo "The Drasi query should now process this event and log the result."
echo ""
echo "To publish another event with a different sender:"
echo "  ./examples/drasi-platform/scripts/publish-event.sh Bob"
echo ""
echo "To view query results:"
echo "  ./examples/drasi-platform/scripts/view-results.sh"
