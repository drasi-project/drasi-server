# Drasi Playground

An interactive web UI for experimenting with Drasi functionality. This example application provides a hands-on environment to explore Drasi's continuous query capabilities without any setup.

## Features

- **Dynamic Source Management**: Create and delete data sources through the web UI
- **Interactive Query Builder**: Create and manage Cypher queries with Monaco Editor
- **Real-time Data Tables**: View and edit source data with instant query updates
- **Live Results**: Query results stream in real-time via Server-Sent Events (SSE)
- **No Setup Required**: Just start the app and begin exploring

## Quick Start

There are three ways to run the Drasi Playground:

### Option 1: Dev Container (Recommended)

Open this repository in VS Code and select **"Reopen in Container"** from the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`). When prompted, choose **"Drasi Server - Playground"**. The playground will build and start automatically — once the container is ready, open **http://localhost:5373** in your browser.

### Option 2: GitHub Codespaces

Click **Code → Codespaces → New codespace** on the repository's GitHub page. Select the **"Drasi Server - Playground"** dev container configuration. The playground will start automatically once the codespace is ready. Open the forwarded port **5373** from the Ports tab.

### Option 3: Run Locally

Requires Rust toolchain and Node.js installed on your machine.

```bash
# Build Drasi Server (from repo root)
cargo build --release

# Start the playground
./start.sh

# Visit http://localhost:5373 in your browser

# Stop the playground
./stop.sh
```

## Getting Started Tutorial

### Step 1: Create a Data Source

1. Navigate to the **Sources** tab
2. Click **Create Source**
3. Fill in the form:
   - **Source ID**: `my-data-source`
   - **Source Type**: Select `http` (for manual data injection)
   - **Auto Start**: Check this box
4. Click **Create**

### Step 2: Add Sample Data

1. In the **Sources** tab, find your newly created source
2. The data table below shows the current data (empty initially)
3. Click **Add Row** and enter sample data:
   ```json
   {
     "id": 1,
     "name": "Product A",
     "price": 99.99,
     "category": "Electronics",
     "stock": 50
   }
   ```
4. Click **Save** - the data is now in your source!

### Step 3: Create a Query

1. Navigate to the **Queries** tab
2. Click **Create Query**
3. Fill in the form:
   - **Query ID**: `all-products`
   - **Query**: Enter this Cypher query:
     ```cypher
     MATCH (p:products)
     RETURN p.id AS id,
            p.name AS name,
            p.price AS price,
            p.category AS category,
            p.stock AS stock
     ```
   - **Sources**: Check `my-data-source`
   - **Auto Start**: Check this box
4. Click **Create**

### Step 4: View Real-Time Results

1. Navigate to the **Results** tab
2. Select `all-products` from the dropdown
3. You'll see your data displayed in real-time!
4. Go back to **Sources** tab and add more data or edit existing rows
5. Watch the **Results** tab update instantly via SSE!

## Example Use Cases

### E-commerce Product Catalog

Track inventory and find low-stock items:

```cypher
MATCH (p:products)
WHERE p.stock < 10
RETURN p.name AS product, p.stock AS remaining, p.category
```

### Price Monitoring

Find premium products over a threshold:

```cypher
MATCH (p:products)
WHERE p.price > 100
RETURN p.name AS product, p.price, p.category
ORDER BY p.price DESC
```

### Category Analysis

Group products by category:

```cypher
MATCH (p:products)
RETURN p.category AS category, count(*) AS product_count
```

## Architecture

The playground uses the same proven architecture as the trading example:

- **Frontend**: React 18 + TypeScript + Vite (port 5373)
- **UI Framework**: TailwindCSS with custom theme
- **Code Editor**: Monaco Editor for Cypher queries
- **Data Tables**: TanStack React Table
- **Backend**: Drasi Server REST API (port 8380)
- **Real-time Updates**: SSE reaction (port 8381)
- **Dynamic Resources**: All queries and reactions created via API

### Data Flow

```
User Input (Web UI)
    ↓
HTTP Source (port 9000)
    ↓
Drasi Server Core
    ↓
Continuous Queries (Cypher)
    ↓
SSE Reaction (port 8381)
    ↓
Web UI Updates (Real-time)
```

## API Endpoints

The playground interacts with Drasi Server through these endpoints:

- `GET /health` - Health check
- `GET /api/versions` - List available API versions
- `GET /api/v1/sources` - List all sources
- `POST /api/v1/sources` - Create a new source
- `DELETE /api/v1/sources/{id}` - Delete a source
- `POST /sources/{id}/events` - Inject data into source (direct to HTTP source port)
- `GET /api/v1/queries` - List all queries
- `POST /api/v1/queries` - Create a new query
- `DELETE /api/v1/queries/{id}` - Delete a query
- `GET /api/v1/queries/{id}/results` - Get query results
- `GET /api/v1/reactions` - List all reactions
- `POST /api/v1/reactions` - Create a new reaction

## Troubleshooting

### Port Already in Use

If you see "port already in use" errors:

```bash
# Kill processes on ports 8380 or 5373
lsof -ti:8380 | xargs kill
lsof -ti:5373 | xargs kill
```

### Drasi Server Won't Start

Check if the binary exists:

```bash
ls -la ../../target/release/drasi-server
```

If not found, build it:

```bash
cd ../.. && cargo build --release
```

### UI Not Updating

1. Check SSE connection status (top-right indicator)
2. Open browser console for errors
3. Verify queries are running (status should be "Running")
4. Check that the SSE reaction exists and includes your query

### No Data Showing

1. Ensure you've added data through the **Sources** tab
2. Verify your query's Cypher syntax is correct
3. Check that node labels in data match labels in query (e.g., `products`)
4. View logs at `logs/drasi-server.log` for errors

## Advanced Features

### Creating Joins

You can create queries that join data from multiple sources:

```cypher
MATCH (o:orders)-[:FOR_PRODUCT]->(p:products)
RETURN o.id AS order_id,
       p.name AS product_name,
       o.quantity,
       (o.quantity * p.price) AS total_price
```

Define the join in the query creation form:

```json
{
  "id": "FOR_PRODUCT",
  "keys": [
    { "label": "orders", "property": "product_id" },
    { "label": "products", "property": "id" }
  ]
}
```

### Data Injection Format

When adding data through the UI, it's converted to this format:

```json
{
  "operation": "insert",
  "element": {
    "type": "node",
    "id": "unique_id",
    "labels": ["products"],
    "properties": {
      "id": 1,
      "name": "Product A",
      "price": 99.99
    }
  },
  "timestamp": 1699876200000000000
}
```

## Learn More

- [Drasi Documentation](https://drasi.io)
- [Cypher Query Language](https://neo4j.com/docs/cypher-manual/current/)
- [React Table Docs](https://tanstack.com/table/latest)
- [Monaco Editor](https://microsoft.github.io/monaco-editor/)
