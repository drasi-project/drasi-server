#!/bin/bash
# View query results from the Drasi Platform example

DRASI_HOST="localhost"
DRASI_PORT="8080"
QUERY_ID="hello-world-from"

echo "==================================="
echo "Drasi Platform Example - Query Results"
echo "==================================="
echo ""

# Check if curl is available
if ! command -v curl &> /dev/null; then
    echo "ERROR: curl is not installed or not in PATH"
    exit 1
fi

echo "Fetching results for query: $QUERY_ID"
echo "Endpoint: http://$DRASI_HOST:$DRASI_PORT/api/queries/$QUERY_ID/results"
echo ""

# Fetch query results
RESPONSE=$(curl -s -w "\n%{http_code}" "http://$DRASI_HOST:$DRASI_PORT/api/queries/$QUERY_ID/results")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

# Check HTTP status
if [ "$HTTP_CODE" != "200" ]; then
    echo "ERROR: Failed to fetch results (HTTP $HTTP_CODE)"
    echo ""
    echo "Response:"
    echo "$BODY"
    echo ""
    echo "Make sure the Drasi server is running:"
    echo "  ./examples/drasi-platform/scripts/start-server.sh"
    exit 1
fi

# Try to format with jq if available
if command -v jq &> /dev/null; then
    echo "Results (formatted):"
    echo "$BODY" | jq '.'
else
    echo "Results (raw JSON):"
    echo "$BODY"
    echo ""
    echo "Tip: Install jq for formatted output (brew install jq)"
fi

echo ""
echo "To publish more events:"
echo "  ./examples/drasi-platform/scripts/publish-event.sh [FromName]"
