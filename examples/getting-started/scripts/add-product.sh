#!/bin/bash
# Add a new product to the catalog

# Example usage:
# ./scripts/add-product.sh prod-009 "Gaming Mouse" 69.99 Electronics true

if [ $# -ne 5 ]; then
  echo "Usage: $0 <id> <name> <price> <category> <in_stock>"
  echo "Example: $0 prod-009 \"Gaming Mouse\" 69.99 Electronics true"
  exit 1
fi

ID=$1
NAME=$2
PRICE=$3
CATEGORY=$4
IN_STOCK=$5

echo "Adding product: $NAME (ID: $ID, Price: \$$PRICE)"

curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d "{
    \"operation\": \"insert\",
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
echo "Product added successfully!"
