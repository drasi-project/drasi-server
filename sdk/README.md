# Drasi React SDK

This directory contains the official React SDK for Drasi Server integration.

## Package: @drasi/react

A production-ready React library providing hooks and utilities for building real-time, change-driven applications with Drasi's continuous query streaming.

### Key Features

- 🔄 **Real-time SSE Streaming** - Automatic subscription to continuous queries
- 🪝 **React Hooks** - `useQuery`, `useDrasiClient`, `useConnectionStatus`
- 🔌 **Auto-Reconnection** - Built-in resilience with exponential backoff
- 📦 **Type-Safe** - Full TypeScript support with generics
- 🧹 **Auto Cleanup** - Lifecycle management for queries and connections
- ⚡ **Bootstrap Support** - Initial data load + incremental updates

### Quick Start

```bash
cd sdk/react
npm install
npm run build
```

### Usage

```typescript
import { useDrasiClient, useQuery } from '@drasi/react';

function App() {
  useDrasiClient({
    baseUrl: 'http://localhost:8080',
    queries: [{
      id: 'my-query',
      query: 'MATCH (n:Person) RETURN n',
      sources: [{ source_id: 'db' }]
    }]
  });

  return <Dashboard />;
}

function Dashboard() {
  const { data, loading } = useQuery('my-query');
  
  if (loading) return <div>Loading...</div>;
  return <div>{data?.length} items</div>;
}
```

### Development

```bash
# Build the package
npm run build

# Watch mode for development
npm run dev

# Run tests
npm test

# Type checking
npm run typecheck

# Linting
npm run lint
```

### Publishing

The package is designed to be published to npm as `@drasi/react`:

```bash
npm run build
npm publish --access public
```

### License

Apache-2.0 - See [LICENSE](../LICENSE)

### Support

- 📚 [Documentation](https://drasi.io/)
- 🐛 [Issue Tracker](https://github.com/drasi-project/drasi-server/issues)
