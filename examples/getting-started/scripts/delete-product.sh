#!/bin/bash
# Delete a product from the catalog

# Example usage:
# ./scripts/delete-product.sh prod-005

if [ $# -ne 1 ]; then
  echo "Usage: $0 <id>"
  echo "Example: $0 prod-005"
  exit 1
fi

ID=$1

echo "Deleting product: $ID"

curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d "{
    \"operation\": \"delete\",
    \"id\": \"$ID\",
    \"labels\": [\"Product\"]
  }"

echo ""
echo "Product deleted successfully!"
