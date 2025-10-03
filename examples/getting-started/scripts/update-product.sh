#!/bin/bash
# Update an existing product in the catalog

# Example usage:
# ./scripts/update-product.sh prod-001 "Wireless Headphones Pro" 99.99 Electronics true

if [ $# -ne 5 ]; then
  echo "Usage: $0 <id> <name> <price> <category> <in_stock>"
  echo "Example: $0 prod-001 \"Wireless Headphones Pro\" 99.99 Electronics true"
  exit 1
fi

ID=$1
NAME=$2
PRICE=$3
CATEGORY=$4
IN_STOCK=$5

echo "Updating product: $ID to $NAME (Price: \$$PRICE)"

curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d "{
    \"operation\": \"update\",
    \"element\": {
      \"type\": \"node\",
      \"id\": \"$ID\",
      \"labels\": [\"Product\"],
      \"properties\": {
        \"id\": \"$ID\",
        \"name\": \"$NAME\",
        \"price\": $PRICE,
        \"category\": \"$CATEGORY\",
        \"in_stock\": $IN_STOCK
      }
    }
  }"

echo ""
echo "Product updated successfully!"
