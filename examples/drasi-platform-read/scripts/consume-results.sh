#!/bin/bash
# Consume query results from Redis stream
# This script reads CloudEvent-formatted results from the platform reaction

set -e

REDIS_HOST="localhost"
REDIS_PORT="6379"
STREAM_KEY="hello-world-from-results"

echo "==================================="
echo "Drasi Platform - Consume Results"
echo "==================================="
echo ""
echo "Stream: $STREAM_KEY"
echo "Redis: $REDIS_HOST:$REDIS_PORT"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Check if jq is available for pretty printing
if command -v jq &> /dev/null; then
    USE_JQ=true
    echo "Using jq for formatted output"
else
    USE_JQ=false
    echo "Install jq for formatted JSON output: brew install jq (macOS) or apt-get install jq (Linux)"
fi

echo ""
echo "--- CloudEvents from $STREAM_KEY ---"
echo ""

# Check if stream exists
if ! docker exec drasi-redis redis-cli EXISTS "$STREAM_KEY" | grep -q "1"; then
    echo "⚠️  Stream '$STREAM_KEY' does not exist yet."
    echo ""
    echo "The stream will be created when:"
    echo "  1. The Drasi server is started with the platform reaction"
    echo "  2. Query results are published"
    echo ""
    echo "Try:"
    echo "  ./start-server.sh          # Start the server"
    echo "  ./publish-event.sh Alice   # Publish a test event"
    echo ""
    exit 1
fi

# Get stream length
STREAM_LENGTH=$(docker exec drasi-redis redis-cli XLEN "$STREAM_KEY")
echo "Stream contains $STREAM_LENGTH messages"
echo ""

# Function to format and display messages
display_messages() {
    while IFS= read -r line; do
        # Skip empty lines and Redis array indicators
        if [[ -z "$line" || "$line" =~ ^[0-9]+\)$ ]]; then
            continue
        fi

        # Check if this is a stream ID line (format: 1234567890-0)
        if [[ "$line" =~ ^[0-9]+-[0-9]+$ ]]; then
            echo "═══════════════════════════════════════"
            echo "Message ID: $line"
            continue
        fi

        # Check if this is the 'event' field containing JSON
        if [[ "$line" == *"event"* ]]; then
            # Extract JSON after 'event'
            JSON_DATA="${line#*event}"
            JSON_DATA="${JSON_DATA#"${JSON_DATA%%[![:space:]]*}"}" # trim leading whitespace

            if [ "$USE_JQ" = true ]; then
                echo "$JSON_DATA" | jq '.'
            else
                echo "$JSON_DATA"
            fi
        fi
    done
}

# Read all messages from the stream
echo "Reading all messages..."
echo ""
docker exec drasi-redis redis-cli XRANGE "$STREAM_KEY" - + | display_messages

echo ""
echo "═══════════════════════════════════════"
echo ""
echo "To continuously monitor new messages, run:"
echo "  docker exec drasi-redis redis-cli XREAD BLOCK 0 STREAMS $STREAM_KEY \\$"
echo ""
echo "To view only the latest 10 messages:"
echo "  docker exec drasi-redis redis-cli XREVRANGE $STREAM_KEY + - COUNT 10"
echo ""
