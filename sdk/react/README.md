# @drasi/react

React hooks and client library for seamless integration with Drasi Server. Build real-time, change-driven applications with continuous query streaming via Server-Sent Events (SSE).

## Features

- 🔄 **Real-time Updates** - Subscribe to continuous queries with automatic SSE streaming
- 🪝 **React Hooks** - Idiomatic React integration with `useQuery`, `useConnectionStatus`
- 🔌 **Auto-Reconnection** - Built-in reconnection logic with exponential backoff
- 📦 **Type-Safe** - Full TypeScript support with generics
- 🎯 **Zero Dependencies** - Only React as peer dependency
- 🧹 **Auto Cleanup** - Automatic resource cleanup on unmount
- ⚡ **Bootstrap Support** - Initial data load before streaming updates
- 🎭 **Separate Providers** - Independent REST and SSE clients for maximum flexibility

## Installation

```bash
npm install @drasi/react
```

## Quick Start

### 1. Configure Providers (main.tsx)

```typescript
import { RestProvider, SSEProvider } from '@drasi/react';

const queries = [
  {
    id: 'my-query',
    query: 'MATCH (n:Person) RETURN n.name AS name, n.age AS age',
    sources: [{ source_id: 'postgres-db' }]
  }
];

ReactDOM.createRoot(document.getElementById('root')!).render(
  <RestProvider 
    config={{ baseUrl: 'http://localhost:8080' }}
    queries={queries}
  >
    <SSEProvider 
      config={{ endpoint: 'http://localhost:50051/events' }}
      queryIds={queries.map(q => q.id)}
    >
      <App />
    </SSEProvider>
  </RestProvider>
);
```

### 2. Subscribe to Queries (App.tsx)

```typescript
import { useQuery } from '@drasi/react';

interface Person {
  name: string;
  age: number;
}

function Dashboard() {
  const { data, loading, error } = useQuery<Person>('my-query', {
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

### `useDrasiClient(config?: DrasiClientConfig)`

Initialize and access the Drasi client singleton.

**Config Options:**
```typescript
interface DrasiClientConfig {
  baseUrl?: string;                    // Default: 'http://localhost:8080'
  queries?: QueryDefinition[];         // Queries to create on init
  reactionId?: string;                 // Default: 'drasi-sse-stream'
  sseConfig?: {
    host?: string;                     // Default: '0.0.0.0'
    port?: number;                     // Default: 50051
    sse_path?: string;                 // Default: '/events'
    heartbeat_interval_ms?: number;    // Default: 15000
  };
  autoInitialize?: boolean;            // Default: true
}
```

**Returns:**
```typescript
{
  client: DrasiClient | null;
  initialized: boolean;
  error: string | null;
}
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

## Advanced Usage

### Dynamic Query Creation

```typescript
import { getDrasiClient } from '@drasi/react';

function CreateQuery() {
  const client = getDrasiClient();

  const handleCreate = async () => {
    await client?.createQuery({
      id: 'dynamic-query',
      query: 'MATCH (s:Stock) WHERE s.price > $minPrice RETURN s',
      sources: [{ source_id: 'market-data' }],
      parameters: { minPrice: 100 }
    });
  };

  return <button onClick={handleCreate}>Create Query</button>;
}
```

### Synthetic Joins

Create relationships between data from different sources:

```typescript
const queries = [{
  id: 'portfolio-with-prices',
  query: `
    MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    RETURN p.quantity * sp.price AS value
  `,
  sources: [
    { source_id: 'postgres-db' },
    { source_id: 'price-feed' }
  ],
  joins: [
    {
      id: 'OWNS_STOCK',
      keys: [
        { label: 'portfolio', property: 'symbol' },
        { label: 'stocks', property: 'symbol' }
      ]
    },
    {
      id: 'HAS_PRICE',
      keys: [
        { label: 'stocks', property: 'symbol' },
        { label: 'stock_prices', property: 'symbol' }
      ]
    }
  ]
}];
```

### Custom Transformations

```typescript
const { data } = useQuery<ProcessedData>('raw-query', {
  transform: (raw) => ({
    id: raw.id,
    displayName: `${raw.first_name} ${raw.last_name}`,
    totalValue: parseFloat(raw.value) * parseFloat(raw.quantity),
    timestamp: new Date(raw.created_at)
  }),
  filter: (item) => item.totalValue > 1000,
  sortBy: (a, b) => b.totalValue - a.totalValue
});
```

### Manual Client Usage

For non-React contexts or advanced control:

```typescript
import { DrasiClient } from '@drasi/react';

const client = new DrasiClient({
  baseUrl: 'http://localhost:8080',
  queries: [/* ... */]
});

await client.initialize();

const unsubscribe = client.subscribe('my-query', (result) => {
  console.log('Query update:', result.data);
});

// Later...
unsubscribe();
await client.disconnect();
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

### 1. Initialize Once at Root

```typescript
// App.tsx
function App() {
  useDrasiClient({ /* config */ });
  return <Router><Routes /></Router>;
}
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
