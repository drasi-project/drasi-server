# Drasi Trading Demo

A real-time stock trading application demonstrating Drasi Server's powerful continuous query capabilities. This example showcases how to build a change-driven application that reacts instantly to data changes from multiple sources, using Drasi's graph-based query engine to create synthetic relationships between disparate data sources.

## Overview

The Trading Demo illustrates key Drasi Server concepts:
- **Continuous Queries**: Queries that automatically update as underlying data changes
- **Multi-Source Integration**: Combining PostgreSQL CDC with HTTP-delivered events
- **Synthetic Joins**: Creating relationships between data from different sources without explicit foreign keys
- **Push-Based Architecture**: All UI updates are pushed via Server-Sent Events (SSE)
- **Change-Driven Design**: The application only updates when data actually changes

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  PostgreSQL DB  │────▶│    Drasi Server      │◀────│  HTTP Source    │
│  (Replication)  │ WAL │                      │HTTP │  (Port 9000)    │
│                 │     │  Sources:            │     │                 │
│  Tables:        │     │  - postgres-stocks   │     │  Price Updates  │
│  • stocks       │     │  - price-feed        │     │  from Python    │
│  • portfolio    │     │                      │     │  Generator      │
└─────────────────┘     │  Continuous Queries: │     └─────────────────┘
                        │  - watchlist-query   │
                        │  - portfolio-query   │
                        │  - top-gainers      │
                        │  - top-losers       │
                        │  - high-volume      │
                        │  - ticker-query     │
                        │                      │
                        │  Reaction:           │
                        │  - SSE Stream        │
                        └──────────┬───────────┘
                                   │
                              SSE (Port 50051)
                                   │
                                   ▼
                        ┌──────────────────────┐
                        │     React App        │
                        │                      │
                        │  Panels:             │
                        │  • Watchlist         │
                        │  • Portfolio         │
                        │  • Top Gainers      │
                        │  • Top Losers       │
                        │  • High Volume      │
                        │  • Stock Ticker     │
                        └──────────────────────┘
```

## How It Works

### Data Flow

1. **PostgreSQL Source** (`postgres-stocks`):
   - Uses logical replication (WAL) to stream changes from `stocks` and `portfolio` tables
   - Provides initial bootstrap data and tracks portfolio positions
   - Changes are captured in real-time using CDC (Change Data Capture)

2. **HTTP Source** (`price-feed`):
   - Receives real-time price updates from the Python price generator
   - Simulates market data feeds with continuous price changes

3. **Continuous Queries**:
   - Process changes from both sources using Cypher graph queries
   - Create synthetic `HAS_PRICE` and `OWNS_STOCK` relationships
   - Automatically recalculate when any underlying data changes

4. **SSE Reaction**:
   - Pushes query result changes to connected clients
   - No polling required - updates flow instantly when data changes

5. **React Application**:
   - Connects to SSE stream for real-time updates
   - Updates UI components only when relevant data changes

### Synthetic Joins

The demo uses Drasi's synthetic join capability to connect data without explicit database relationships:

```cypher
# Example: Portfolio Query
MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
```

- `OWNS_STOCK`: Links portfolio positions to stock information (both from PostgreSQL)
- `HAS_PRICE`: Links stocks to their current prices (PostgreSQL to HTTP source)

These relationships are defined in the query configuration, not in the database schema.

## UI Components

### Watchlist Panel
- **Query**: `watchlist-query`
- **Purpose**: Displays real-time prices for selected stocks (AAPL, MSFT, GOOGL, TSLA, NVDA)
- **Updates**: When price changes arrive via HTTP source

### Portfolio Panel
- **Query**: `portfolio-query`
- **Purpose**: Shows current portfolio holdings with profit/loss calculations
- **Features**:
  - Total portfolio value and returns
  - Individual position P&L
  - Real-time value updates as prices change
- **Updates**: When prices change or portfolio positions are modified

### Top Gainers Panel
- **Query**: `top-gainers-query`
- **Purpose**: Lists stocks with highest positive price changes
- **Updates**: Dynamically reorders as prices fluctuate

### Top Losers Panel
- **Query**: `top-losers-query`
- **Purpose**: Lists stocks with largest price declines
- **Updates**: Dynamically reorders as prices fluctuate

### High Volume Panel
- **Query**: `high-volume-query`
- **Purpose**: Shows most actively traded stocks by volume
- **Updates**: When volume data changes

### Stock Ticker
- **Query**: `ticker-query`
- **Purpose**: Scrolling ticker showing all stocks with price changes
- **Updates**: Continuously with all price updates

## Quick Start (Easy Mode)

The simplest way to run the demo:

```bash
# From the examples/trading directory
./start-demo.sh
```

This script will:
1. Check prerequisites (Docker, Node.js, Python 3)
2. Start PostgreSQL database
3. Build and start Drasi Server (if needed)
4. Create queries and reactions
5. Start the React app
6. Start the price generator
7. Open the browser to http://localhost:5173

To stop the demo:
```bash
./stop-demo.sh
```

## Manual Setup (Advanced)

For developers who want more control over the components:

### 1. Start PostgreSQL Database
```bash
cd database
docker-compose up -d
```

### 2. Start Drasi Server
```bash
# From drasi-server root
cargo build --release
./target/release/drasi-server --config examples/trading/server/trading-sources-only.yaml
```

The server starts with only sources configured. Queries will be created dynamically.

### 3. Create Queries and Reactions
```bash
# Create all queries for the app
./create-app-queries.sh
```

### 4. Start the React Application
```bash
cd app
npm install  # First time only
npm run dev
```

The app will be available at http://localhost:5173

### 5. Start Price Generator
```bash
cd mock-generator
pip install -r requirements.txt  # First time only
python3 simple_price_generator.py
```

This will start sending price updates every 2 seconds.

## Configuration Files

- `server/trading-sources-only.yaml`: Drasi Server configuration with sources only
- `database/docker-compose.yml`: PostgreSQL database setup
- `database/init.sql`: Database schema and sample data
- `app/src/hooks/useDrasi.ts`: React hook for Drasi integration
- `create-app-queries.sh`: Script to create all queries via REST API

## Troubleshooting

### Replication Slot Issues
If you see "replication slot already exists" errors:
```bash
./clean-replication.sh
```

### Clean Start
To remove all queries and start fresh:
```bash
./cleanup-app-queries.sh
```

### Database Reset
To completely reset the database:
```bash
cd database
docker-compose down -v
docker-compose up -d
```

## Key Concepts Demonstrated

1. **Change-Driven Architecture**: The UI updates only when data changes, no polling
2. **Multi-Source Queries**: Joining data from PostgreSQL and HTTP sources
3. **Synthetic Relationships**: Creating graph relationships without database foreign keys
4. **Continuous Processing**: Queries automatically update as source data changes
5. **Push-Based Updates**: SSE delivers changes instantly to the UI
6. **Bootstrap + Incremental**: Initial data load followed by incremental updates

## Development Notes

- The app uses TypeScript and React with Vite for fast development
- TailwindCSS provides the dark trading terminal aesthetic
- All queries use Cypher graph query language
- The PostgreSQL source uses logical replication for zero-polling CDC
- Price updates demonstrate high-frequency data ingestion

## Requirements

- Docker and Docker Compose
- Node.js 16+ and npm
- Python 3.7+
- Rust toolchain (if building Drasi Server from source)
- 4GB+ RAM recommended

## Learn More

- [Drasi Documentation](https://drasi.io/)
- [Drasi Core](https://github.com/drasi-project/drasi-core) - The query engine
- [Cypher Query Language](https://neo4j.com/developer/cypher/) - Query syntax reference