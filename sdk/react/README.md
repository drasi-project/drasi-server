# @drasi/react

React hooks and components for seamless integration with Drasi Server. Build real-time, change-driven applications with continuous query streaming via Server-Sent Events (SSE).

## Features

- 🔄 **Real-time Updates** - Subscribe to continuous queries with automatic SSE streaming
- 🪝 **React Hooks** - Idiomatic React integration with `useQuery`, `useConnectionStatus`
- 🔌 **Auto-Reconnection** - Built-in reconnection logic with exponential backoff
- 📦 **Type-Safe** - Full TypeScript support with generics
- ⚡ **Bootstrap Support** - Initial data load before streaming updates
- 🔀 **Multiple Instances** - Support for connecting to multiple Drasi servers

## Installation

```bash
npm install @drasi/react
```

## Quick Start

### 1. Configure SSE Provider

Wrap your app with `SSEProvider` to stream query results from a Drasi SSE reaction:

```typescript
import { SSEProvider } from '@drasi/react';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <SSEProvider 
    config={{ 
      endpoint: 'http://localhost:8080/api/reactions/my-sse-reaction/stream'
    }}
    queryIds={['query-1', 'query-2']}
  >
    <App />
  </SSEProvider>
);
```

### 2. Subscribe to Queries

Use the `useQuery` hook to access real-time query results:

```typescript
import { useQuery } from '@drasi/react';

interface Person {
  name: string;
  age: number;
}

function Dashboard() {
  const { data, loading, error } = useQuery<Person>('query-1', {
    transform: (item) => ({
      name: item.name,
      age: parseInt(item.age)
    }),
    sortBy: (a, b) => a.name.localeCompare(b.name)
  });

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error}</div>;

  return (
    <ul>
      {data?.map(person => (
        <li key={person.name}>{person.name} - {person.age} years old</li>
      ))}
    </ul>
  );
}
```

### 3. Monitor Connection Status

```typescript
import { useConnectionStatus } from '@drasi/react';

function ConnectionIndicator() {
  const status = useConnectionStatus();

  return (
    <div className={status.connected ? 'online' : 'offline'}>
      {status.connected ? '🟢 Connected' : '🔴 Disconnected'}
      {status.reconnecting && ' (Reconnecting...)'}
    </div>
  );
}
```

## API Reference

### `SSEProvider`

Provider component that establishes SSE connection to a Drasi reaction endpoint.

**Props:**
```typescript
interface SSEProviderProps {
  config: {
    endpoint: string;                    // SSE endpoint URL (required)
    reconnectDelay?: number;             // Reconnection delay in ms (default: 1000)
    maxReconnectDelay?: number;          // Max delay in ms (default: 30000)
  };
  queryIds: string[];                    // Query IDs to subscribe to
  children: React.ReactNode;
}
```

**Example:**
```typescript
<SSEProvider 
  config={{ 
    endpoint: 'http://localhost:8080/api/reactions/my-sse-reaction/stream'
  }}
  queryIds={['portfolio', 'stocks', 'prices']}
>
  <App />
</SSEProvider>
```

### `useQuery<T>(queryId: string, options?: QueryOptions)`

Subscribe to a query with real-time updates.

**Options:**
```typescript
interface QueryOptions<TInput, TOutput> {
  transform?: (item: TInput) => TOutput;          // Transform each item
  accumulationStrategy?: 'replace' | 'merge' | 'append';  // Default: 'merge'
  sortBy?: (a: TOutput, b: TOutput) => number;    // Sort function
  filter?: (item: TOutput) => boolean;            // Filter function
  getItemKey?: (item: TOutput) => string;         // Custom key extractor
}
```

**Accumulation Strategies:**
- `replace` - Replace all data on each update
- `merge` - Merge new items with existing (by key), clear on large batches
- `append` - Append all items (for event logs)

**Returns:**
```typescript
{
  data: T[] | null;
  loading: boolean;
  error: string | null;
  lastUpdate: Date | null;
}
```

### `useConnectionStatus()`

Monitor SSE connection health.

**Returns:**
```typescript
{
  connected: boolean;
  error?: string;
  reconnecting?: boolean;
  lastConnected?: Date;
}
```

**Example:**
```typescript
function StatusBar() {
  const { connected, reconnecting, error } = useConnectionStatus();
  
  if (error && !reconnecting) {
    return <div>Connection failed: {error}</div>;
  }
  
  return (
    <div>
      {connected ? '🟢 Live' : '🔴 Offline'}
      {reconnecting && ' (reconnecting...)'}
    </div>
  );
}
```

## Multiple Drasi Instances

You can connect to multiple Drasi servers by nesting providers with different endpoints:

```typescript
<SSEProvider 
  config={{ endpoint: 'http://drasi-prod:8080/api/reactions/main/stream' }}
  queryIds={['prod-portfolio']}
>
  <SSEProvider 
    config={{ endpoint: 'http://drasi-staging:8080/api/reactions/test/stream' }}
    queryIds={['staging-portfolio']}
  >
    <App />
  </SSEProvider>
</SSEProvider>
```

Components will use the nearest provider in the React tree.

## Advanced Usage

### Custom Transformations

```typescript
const { data } = useQuery<ProcessedStock>('stocks', {
  transform: (raw) => ({
    symbol: raw.symbol,
    displayPrice: `$${parseFloat(raw.price).toFixed(2)}`,
    marketCap: parseFloat(raw.price) * parseInt(raw.shares),
    timestamp: new Date(raw.updated_at)
  }),
  filter: (stock) => stock.marketCap > 1000000,
  sortBy: (a, b) => b.marketCap - a.marketCap
});
```

### Accumulation Strategies

Control how query results are accumulated:

```typescript
// Replace all data on each update (snapshot mode)
const { data } = useQuery('stocks', { 
  accumulationStrategy: 'replace' 
});

// Merge updates with existing data (default, best for most cases)
const { data } = useQuery('portfolio', { 
  accumulationStrategy: 'merge' 
});

// Append all updates (event log mode)
const { data } = useQuery('trades', { 
  accumulationStrategy: 'append' 
});
```

### Custom Key Extraction

By default, items are keyed by stringifying the entire object. For better performance with `merge` strategy:

```typescript
const { data } = useQuery<Stock>('stocks', {
  accumulationStrategy: 'merge',
  getItemKey: (item) => item.symbol  // Use symbol as unique key
});
```

## Data Transformations

The library automatically:

1. **Converts snake_case to camelCase**
   ```typescript
   // Drasi returns: { first_name: "John", last_name: "Doe" }
   // Hook provides: { firstName: "John", lastName: "Doe" }
   ```

2. **Parses numeric strings**
   ```typescript
   // Drasi returns: { price: "123.45" }
   // Hook provides: { price: 123.45 }
   ```

You can override with custom `transform` function.

## Error Handling

```typescript
const { data, error } = useQuery('my-query');

if (error) {
  // Handle query subscription error
  return <ErrorDisplay message={error} />;
}
```

Connection errors are available via `useConnectionStatus()`:

```typescript
const { connected, error, reconnecting } = useConnectionStatus();

if (error && !reconnecting) {
  // Connection permanently failed
  return <FatalError />;
}
```

## Best Practices

### 1. Define Queries in Drasi Config

Use Drasi's YAML configuration to define queries server-side:

```yaml
queries:
  - id: "portfolio"
    query: "MATCH (p:Portfolio) RETURN p"
    query_language: "cypher"
    source_subscriptions:
      - source_id: "postgres-db"
    auto_start: true
```

Then simply reference them in your React app:

```typescript
<SSEProvider queryIds={['portfolio', 'stocks', 'prices']}>
  <App />
</SSEProvider>
```

### 2. Use TypeScript Generics

```typescript
interface Stock {
  symbol: string;
  price: number;
}

const { data } = useQuery<Stock>('stocks-query');
// data is typed as Stock[] | null
```

### 3. Memoize Options

```typescript
const options = useMemo(() => ({
  sortBy: (a: Stock, b: Stock) => b.price - a.price,
  filter: (s: Stock) => s.price > 100
}), []);

const { data } = useQuery<Stock>('stocks-query', options);
```

### 4. Handle Loading States

```typescript
if (loading && !data) {
  return <Skeleton />; // Initial load
}

if (loading && data) {
  return <DataWithSpinner data={data} />; // Subsequent updates
}
```

## Examples

See the [trading example](../../examples/trading) for a complete application demonstrating:
- Multiple concurrent queries
- Portfolio calculations with synthetic joins
- Real-time price updates
- Scrolling ticker with animations
- Connection status monitoring

## Requirements

- React 18.0.0 or higher
- Modern browser with EventSource support
- Drasi Server running and accessible

## License

Apache-2.0

## Contributing

Contributions welcome! See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## Links

- [Drasi Documentation](https://drasi.io/)
- [Drasi Core](https://github.com/drasi-project/drasi-core)
- [Issue Tracker](https://github.com/drasi-project/drasi-server/issues)
