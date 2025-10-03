#!/bin/bash
# View current query results from the Drasi API

echo "Fetching results from 'expensive-products' query..."
echo ""

curl -s http://localhost:8080/queries/expensive-products/results | jq '.'

echo ""
