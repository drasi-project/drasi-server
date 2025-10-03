# Getting Started with Drasi Server

This is a minimal example to help you quickly understand Drasi Server's core concepts. You'll learn how to:

- Ingest data using an **HTTP Source**
- Load initial data with a **Bootstrap Provider**
- Process data with a **Continuous Query**
- Output results using a **Reaction**
- View query results via API

## What You'll Build

A simple product catalog system that:
1. Loads initial products from a file
2. Accepts product updates via HTTP
3. Continuously queries for products over $50
4. Logs matching products to the console

## Architecture

```
HTTP Source (port 9000)          Continuous Query              Log Reaction
     │                                  │                            │
     │  POST /products        ┌─────────▼─────────┐                 │
     ├───────────────────────>│  MATCH (p:Product) │                │
     │                        │  WHERE p.price > 50│                │
     │  Bootstrap Provider    │  RETURN p.*        │                │
     │  (products.jsonl)      └─────────┬─────────┘                 │
     └───────────────────────>          │                           │
                                         └──────> Results ──────────>│
                                                                     │
                                                            Console Output
```

## Prerequisites

- Rust and Cargo (1.70 or later)
- `curl` command-line tool
- `jq` for JSON formatting (optional, for view-results.sh)

## Quick Start

### 1. Start the Drasi Server

```bash
./scripts/start-server.sh
```

This builds and starts the server with:
- **API Server** on http://localhost:8080
- **HTTP Source** on http://localhost:9000

### 2. View Bootstrap Data in Logs

Watch the console output. You'll see the initial products loaded from `data/products.jsonl`:

```
[LOG REACTION] expensive-products:
  - Wireless Headphones ($79.99)
  - Coffee Maker ($129.99)
  - Running Shoes ($89.99)
  - Bluetooth Speaker ($59.99)
  ...
```

### 3. Add a New Product

In a new terminal, add a product priced over $50:

```bash
curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d '{
    "operation": "insert",
    "element": {
      "type": "node",
      "id": "prod-009",
      "labels": ["Product"],
      "properties": {
        "id": "prod-009",
        "name": "Gaming Mouse",
        "price": 69.99,
        "category": "Electronics",
        "in_stock": true
      }
    }
  }'
```

Or use the helper script:

```bash
./scripts/add-product.sh prod-009 "Gaming Mouse" 69.99 Electronics true
```

Watch the server logs - you'll see the new product appear in the query results!

### 4. View Query Results via API

Get current query results:

```bash
curl http://localhost:8080/queries/expensive-products/results | jq '.'
```

Or use the helper script:

```bash
./scripts/view-results.sh
```

You'll see JSON output with all products priced over $50.

### 5. Update a Product

Update the price of an existing product:

```bash
curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d '{
    "operation": "update",
    "element": {
      "type": "node",
      "id": "prod-001",
      "labels": ["Product"],
      "properties": {
        "id": "prod-001",
        "name": "Wireless Headphones Pro",
        "price": 99.99,
        "category": "Electronics",
        "in_stock": true
      }
    }
  }'
```

Or use the helper script:

```bash
./scripts/update-product.sh prod-001 "Wireless Headphones Pro" 99.99 Electronics true
```

The query automatically re-evaluates and updates the results!

### 6. Delete a Product

Remove a product from the catalog:

```bash
curl -X POST http://localhost:9000/sources/products-source/events \
  -H "Content-Type: application/json" \
  -d '{
    "operation": "delete",
    "id": "prod-005",
    "labels": ["Product"]
  }'
```

Or use the helper script:

```bash
./scripts/delete-product.sh prod-005
```

The product is removed from query results immediately.

## HTTP Source API Reference

The HTTP source accepts events in **Direct Format** at `http://localhost:9000/sources/products-source/events`:

### Node Insert
```json
{
  "operation": "insert",
  "element": {
    "type": "node",
    "id": "unique-id",
    "labels": ["Product"],
    "properties": {
      "id": "unique-id",
      "name": "Product Name",
      "price": 99.99,
      "category": "Category",
      "in_stock": true
    }
  }
}
```

### Node Update
```json
{
  "operation": "update",
  "element": {
    "type": "node",
    "id": "existing-id",
    "labels": ["Product"],
    "properties": {
      "id": "existing-id",
      "name": "Updated Name",
      "price": 149.99,
      "category": "Category",
      "in_stock": false
    }
  }
}
```

### Node Delete
```json
{
  "operation": "delete",
  "id": "existing-id",
  "labels": ["Product"]
}
```

## Viewing Results

### Method 1: Server Logs
The log reaction prints results to the console in real-time. Watch for lines starting with `[LOG REACTION]`.

### Method 2: REST API
Query the current results at any time:

```bash
GET http://localhost:8080/queries/expensive-products/results
```

The API returns a JSON array of all current query results.

## Understanding the Cypher Query

The query in this example is:

```cypher
MATCH (p:Product)
WHERE p.price > 50
RETURN p.id, p.name, p.price, p.category, p.in_stock
```

Breaking it down:
- `MATCH (p:Product)` - Find all nodes with label "Product"
- `WHERE p.price > 50` - Filter for products priced over $50
- `RETURN p.*` - Return all properties of matching products

This is a **continuous query** - it automatically updates whenever data changes!

## Understanding Bootstrap

The bootstrap provider loads initial data when the server starts:

```yaml
bootstrap:
  provider: script_file
  config:
    file_path: examples/getting-started/data/products.jsonl
```

The `products.jsonl` file uses **JSON Lines format** (one JSON object per line):

```jsonl
{"op":"Header","desc":"Initial product catalog","ts":0}
{"op":"NodeInsert","labels":["Product"],"id":"prod-001","properties":{...}}
{"op":"NodeInsert","labels":["Product"],"id":"prod-002","properties":{...}}
```

This provides initial state before accepting live updates.

## Configuration File

The example uses `server-config.yaml` with:

1. **HTTP Source** - Receives data on port 9000
2. **Bootstrap Provider** - Loads initial products from JSONL file
3. **Continuous Query** - Filters products over $50
4. **Log Reaction** - Outputs results to console

All components use `auto_start: true` for convenience.

## Troubleshooting

**Server won't start:**
- Check if ports 8080 or 9000 are already in use
- Ensure you're in the `drasi-server` directory
- Verify the bootstrap file path is correct

**Bootstrap data not loading:**
- Ensure `data/products.jsonl` exists
- Check file format (one JSON object per line)
- Look for bootstrap errors in server logs

**Query results not updating:**
- Verify the HTTP request format matches the examples
- Check that `Content-Type: application/json` header is set
- Ensure the node ID is unique for inserts

**Can't view results via API:**
- Confirm server is running on port 8080
- Check query ID matches "expensive-products"
- Use `curl -v` to see detailed HTTP response

## What's Next?

Now that you understand the basics:

1. **Modify the Query** - Try different filters (e.g., `p.category = "Electronics"`)
2. **Add Relationships** - Extend the data model with relationships between nodes
3. **Explore Complex Examples** - Check out `examples/trading/` for advanced patterns
4. **Read the Docs** - Learn about more source types, query features, and reactions

### Advanced Examples

- **Trading Example** (`examples/trading/`) - Complex multi-source system with PostgreSQL, HTTP, and gRPC

### API Documentation

View the full API docs at: http://localhost:8080/swagger-ui/

## Helper Scripts Reference

- `scripts/start-server.sh` - Build and start the server
- `scripts/add-product.sh <id> <name> <price> <category> <in_stock>` - Add a product
- `scripts/update-product.sh <id> <name> <price> <category> <in_stock>` - Update a product
- `scripts/delete-product.sh <id>` - Delete a product
- `scripts/view-results.sh` - Fetch current query results

## Learn More

- [Drasi Documentation](https://drasi.io) - Official Drasi docs
- [Cypher Query Language](https://neo4j.com/docs/cypher-manual/current/) - Query syntax reference
- [OpenCypher](https://opencypher.org/) - Open standard for graph queries
