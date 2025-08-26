# SSE Console Test Utility

A utility for testing Server-Sent Events (SSE) reactions from any Drasi Server instance.

## Features

- **Configuration-based**: Multiple query/reaction configurations in a single JSON file
- **Dual logging**: Outputs to both console and timestamped log files
- **Auto-setup**: Automatically creates queries and reactions if they don't exist
- **Real-time monitoring**: Live display of SSE events with formatted output
- **Multiple configurations**: Pre-configured for various trading scenarios

## Installation

```bash
npm install
```

## Usage

### Basic Usage

Run with a specific configuration:
```bash
npm start <config-name>

# Examples:
npm start price-ticker
npm start portfolio
npm start watchlist
```

### Using NPM Scripts

Convenient scripts for common configurations:
```bash
npm run start:ticker    # Price ticker stream
npm run start:portfolio  # Portfolio with joins
npm run start:watchlist  # FAANG stocks watchlist
npm run start:gainers    # Top gaining stocks
npm run start:losers     # Top losing stocks  
npm run start:volume     # High volume stocks
npm run start:test       # Simple test configuration
```

### List Available Configurations

```bash
npm run list
# or
npm start --list
```

### Help

```bash
npm start --help
```

## Configuration File

Configurations are stored in `configs.json`. Each configuration contains:

- `name`: Configuration identifier
- `description`: Human-readable description
- `server`: Drasi Server URL (e.g., `http://localhost:8080`)
- `queries`: Array of query definitions, each including:
  - `id`: Unique query identifier
  - `query`: Cypher query text
  - `sources`: Array of source IDs
  - `joins`: Optional synthetic join definitions
  - `auto_start`: Whether to auto-start the query
- `reaction`: SSE reaction definition including:
  - `id`: Unique reaction identifier
  - `reaction_type`: Should be "sse"
  - `properties`: SSE configuration (host, port, path, heartbeat)
  - `auto_start`: Whether to auto-start the reaction

### Example Configuration (Single Query)

```json
{
  "price-ticker": {
    "name": "price-ticker",
    "description": "Simple price ticker for all stocks",
    "server": "http://localhost:8080",
    "queries": [
      {
        "id": "price-ticker-query",
        "query": "MATCH (sp:stock_prices) RETURN sp.symbol AS symbol...",
        "sources": ["price-feed"],
        "auto_start": true
      }
    ],
    "reaction": {
      "id": "sse-ticker-reaction",
      "reaction_type": "sse",
      "properties": {
        "host": "0.0.0.0",
        "port": 50051,
        "sse_path": "/events",
        "heartbeat_interval_ms": 15000
      },
      "auto_start": true
    }
  }
}
```

### Example Configuration (Multiple Queries)

```json
{
  "watchlist": {
    "name": "watchlist",
    "description": "Track specific stocks with multiple queries for debugging",
    "server": "http://localhost:8080",
    "queries": [
      {
        "id": "watchlist-stocks-query",
        "query": "MATCH (s:stocks) WHERE s.symbol IN ['AAPL', 'MSFT'] RETURN s",
        "sources": ["postgres-stocks"],
        "auto_start": true
      },
      {
        "id": "watchlist-prices-query",
        "query": "MATCH (sp:stock_prices) WHERE sp.symbol IN ['AAPL', 'MSFT'] RETURN sp",
        "sources": ["price-feed"],
        "auto_start": true
      },
      {
        "id": "watchlist-joined-query",
        "query": "MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices) WHERE s.symbol IN ['AAPL', 'MSFT'] RETURN s, sp",
        "sources": ["postgres-stocks", "price-feed"],
        "joins": [
          {
            "id": "HAS_PRICE",
            "keys": [
              { "label": "stocks", "property": "symbol" },
              { "label": "stock_prices", "property": "symbol" }
            ]
          }
        ],
        "auto_start": true
      }
    ],
    "reaction": {
      "id": "sse-watchlist-reaction",
      "reaction_type": "sse",
      "properties": {
        "host": "0.0.0.0",
        "port": 50053,
        "sse_path": "/events"
      },
      "auto_start": true
    }
  }
}
```

### Multi-Query Support

The SSE console now supports multiple queries per configuration. This is particularly useful for debugging:

1. **Sequential Creation**: Queries are created sequentially with a 1-second delay between each
2. **Individual Inspection**: Each query can be inspected separately via the Drasi Server REST API
3. **Error Handling**: If a query fails to create after retries, the console will continue with successfully created queries
4. **Reaction Subscription**: The SSE reaction automatically subscribes to all successfully created queries

Benefits:
- Debug individual data sources before joining them
- Verify that each source is providing expected data
- Identify which part of a complex query is causing issues
- Test synthetic joins incrementally

## Available Configurations

| Configuration | Description | Port | Queries | Features |
|--------------|-------------|------|---------|----------|
| `price-ticker` | Simple price ticker for all stocks | 50051 | 1 | Basic price updates |
| `portfolio` | Portfolio tracking with synthetic joins | 50052 | 1 | Multiple sources, joins |
| `watchlist` | Track FAANG stocks with debugging queries | 50053 | 3 | Individual source queries + joined |
| `all` | Monitor all data streams | 50054 | 1 | Complete data visibility |

## Log Files

Log files are created with the pattern:
```
sse-events-{config-name}-{date}.log
```

Each log entry is a JSON object containing:
- `timestamp`: ISO 8601 timestamp
- `level`: Log level (info, debug, error, event, data)
- `message`: Log message
- `data`: Optional additional data

## Troubleshooting

### Common Issues

1. **"Configuration not found"**
   - Check that the configuration name exists in `configs.json`
   - Use `npm run list` to see available configurations

2. **"SSE connection error"**
   - Ensure Drasi Server is running
   - Check that sources are active and generating data
   - Verify the SSE port is not in use

3. **"Query/Reaction already exists"**
   - This is normal - the app will use existing components
   - To recreate, delete via Drasi Server API first

4. **No events received**
   - Check that the source is generating data
   - Verify the query is returning results
   - Look for errors in Drasi Server logs
   - For multi-query configs: Check each query individually via REST API

5. **Query creation fails**
   - Check Cypher syntax in the query
   - Verify all referenced sources exist
   - For joins: Ensure node labels match exactly
   - The app will retry failed queries up to 2 times
   - Partial success: App continues with successfully created queries

### Debug Mode

For more verbose logging, check the generated log files which contain detailed API calls and responses.

## Development

### Adding New Configurations

1. Edit `configs.json`
2. Add a new configuration object with unique IDs
3. Ensure ports don't conflict with existing configs
4. Add a corresponding npm script in `package.json`

### TypeScript Types

Type definitions are in `types.ts`:
- `ConfigFile`: Top-level configuration structure
- `ConfigEntry`: Individual configuration
- `QueryDefinition`: Query configuration
- `ReactionDefinition`: Reaction configuration

## Clean Up

Remove generated log files:
```bash
npm run clean
```

## Requirements

- Node.js 16+
- TypeScript
- Running Drasi Server instance
- Active data sources configured in Drasi Server