# drasi-react

Reusable [React](https://react.dev/) building blocks for UIs whose content is
kept continuously up to date by [Drasi](https://drasi.io) Continuous Queries.

[Drasi](https://drasi.io) is an open-source Data Change Processing platform that
simplifies building change-driven solutions. You declare a **Continuous Query** —
an [openCypher](https://opencypher.org/) or GQL query describing exactly the data
you care about — and Drasi incrementally maintains that query's result set as the
underlying sources change, without polling or re-scanning them. Each change to the
result set is delivered to subscribed **Reactions** as a precise set of added,
updated, and deleted rows. A single query can span multiple sources, and the
meaning of "a change" is defined by your query rather than by the source systems.

`drasi-react` is the client side of that picture: it binds a React app to one or
more Continuous Queries through Drasi Server's **SSE Reaction** and surfaces each
query's live result set as idiomatic React state:

- **One shared connection.** A single [SSE](https://developer.mozilla.org/docs/Web/API/Server-sent_events)
  connection is opened to a Drasi SSE Reaction and **multiplexed** across every
  Continuous Query in your app — not one connection per query.
- **Hooks for live results.** `useDrasiQuery` folds a query's added/updated/
  deleted result changes into a ready‑to‑render array.
- **A batteries‑included table.** `QueryTable` renders a sortable, animated,
  full‑screen‑capable table bound to a query, with a built‑in “view the code”
  dialog.

The library is **completely application agnostic**: it knows nothing about your
queries, your data shapes, or how rows should be keyed/sorted. You provide all of
that through props and options, which makes it reusable across any Drasi project.

> This package lives inside the Drasi `examples/trading` example, where it is
> consumed by the Trading dashboard exactly as an external, independent
> dependency (`@drasi/react`). See [Using it in the Trading example](#using-it-in-the-trading-example).

## Table of contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Concepts](#concepts)
- [API reference](#api-reference)
  - [`DrasiProvider`](#drasiprovider)
  - [`useDrasiQuery`](#usedrasiquery)
  - [`useDrasiConnectionStatus`](#usedrasiconnectionstatus)
  - [`useDrasiServerUiUrl`](#usedrasiserveruiurl)
  - [`useDrasiQueryDefinition`](#usedrasiquerydefinition)
  - [`QueryTable`](#querytable)
  - [Low‑level classes](#low-level-classes)
- [Styling](#styling)
- [Building from source](#building-from-source)
- [Using it in the Trading example](#using-it-in-the-trading-example)
- [License](#license)

## Installation

```bash
npm install @drasi/react
# peer dependencies
npm install react react-dom clsx
```

`react`, `react-dom`, and `clsx` are **peer dependencies** so the library always
uses your application's single copy of React.

Import the stylesheet once (for the row/dialog animations):

```ts
import '@drasi/react/styles.css';
```

## Quick start

```tsx
import {
  DrasiProvider,
  QueryTable,
  useDrasiConnectionStatus,
  type QueryDefinition,
  type ReactionDefinition,
} from '@drasi/react';
import '@drasi/react/styles.css';

// 1. Describe the Continuous Queries to run on the Drasi Server.
const QUERIES: QueryDefinition[] = [
  {
    id: 'stocks-query',
    query: `MATCH (s:stocks)-[:HAS_PRICE]->(p:stock_prices)
            RETURN s.symbol AS symbol, p.price AS price`,
    sources: [
      { sourceId: 'postgres-stocks' },
      { sourceId: 'price-feed' },
    ],
  },
];

// 2. Describe the SSE Reaction that delivers those queries' result changes.
const REACTION: ReactionDefinition = {
  id: 'sse-stream',
  host: '0.0.0.0',
  port: 8281,
  ssePath: '/events',
};

function Stocks() {
  return (
    <QueryTable
      queryId="stocks-query"
      rowKey={(row) => row.symbol}
      columns={[
        { key: 'symbol', label: 'Symbol' },
        { key: 'price', label: 'Price', align: 'right' },
      ]}
      animateOnChange="price"
    />
  );
}

function ConnectionBadge() {
  const status = useDrasiConnectionStatus();
  return <span>{status.connected ? 'Live' : 'Connecting…'}</span>;
}

export default function App() {
  return (
    <DrasiProvider
      serverUrl="http://localhost:8280"
      queries={QUERIES}
      reaction={REACTION}
    >
      <ConnectionBadge />
      <Stocks />
    </DrasiProvider>
  );
}
```

When the provider mounts it:

1. checks the Drasi Server is healthy,
2. ensures every query in `queries` exists and is running,
3. seeds each query's current results from the REST API,
4. ensures the SSE `reaction` exists and is running, and
5. opens **one** SSE connection and fans updates out to each `useDrasiQuery`.

## Concepts

**Multiplexed connection.** A Drasi SSE Reaction can deliver the result changes
for many queries over a single connection. `drasi-react` opens that connection once
([`DrasiSSEClient`](#low-level-classes)) and routes each batch to the right
subscribers by query id, so adding more `QueryTable`s does not add more sockets.

**Content routing for aggregations.** Some Drasi result changes (for example from
aggregating queries) arrive without a query id. The library cannot know which
query they belong to, so you may supply a `routeUnidentified` callback that
inspects a row and decides which query id(s) it belongs to. This keeps all
application‑specific shape knowledge in your app.

**Generic accumulation.** `useDrasiQuery` folds each change batch into a `Map`
keyed by `getKey(row)`; rows flagged `_deleted` are removed. Optional `transform`
(normalize a row) and `postProcess` (sort/filter/slice the final array) options
let you adapt any data model without the library hard‑coding it.

## API reference

### `DrasiProvider`

Establishes the shared connection and makes it available to descendants. Render
it once near the root of your tree.

| Prop | Type | Description |
| --- | --- | --- |
| `serverUrl` | `string` | Base URL of the Drasi Server REST API. Default `http://localhost:8280`. |
| `queries` | `QueryDefinition[]` | Continuous Queries to ensure exist and run. **Required.** |
| `reaction` | `ReactionDefinition` | The SSE Reaction that delivers the queries' result changes. **Required.** |
| `routeUnidentified` | `(rows, deliver) => void` | Optional router for result-change payloads that arrive without a query id. Call `deliver(queryId, rows)` for each matching query. |
| `children` | `ReactNode` | Your application. |

`QueryDefinition`:

```ts
interface QueryDefinition {
  id: string;
  query: string;                 // Cypher (default) or other supported language
  sources: { sourceId: string; pipeline?: unknown[] }[];
  joins?: { id: string; keys: { label: string; property: string }[] }[];
  queryLanguage?: string;        // defaults to 'Cypher'
  [key: string]: any;            // extra fields are passed through to the server
}
```

`ReactionDefinition`:

```ts
interface ReactionDefinition {
  id?: string;                   // defaults to 'sse-stream'
  kind?: string;                 // defaults to 'sse'
  host?: string;                 // defaults to '0.0.0.0'
  port?: number;
  ssePath?: string;              // defaults to '/events'
  heartbeatIntervalMs?: number;
  endpoint?: string;             // override the computed public SSE URL
}
```

### `useDrasiQuery`

```ts
function useDrasiQuery<T = any>(
  queryId: string,
  options?: UseDrasiQueryOptions<T>,
): { data: T[] | null; loading: boolean; error: string | null; lastUpdate: Date | null };
```

Subscribes to a query over the shared connection and returns its accumulated
result set.

`UseDrasiQueryOptions<T>`:

| Option | Type | Description |
| --- | --- | --- |
| `getKey` | `(row) => string \| null` | Stable unique key used to accumulate adds/updates/deletes. Returning `null` skips the row. Defaults to `row.id ?? row.symbol ?? JSON.stringify(row)`. |
| `transform` | `(row) => T` | Normalize a raw row before it is stored (e.g. parse numeric strings). |
| `postProcess` | `(rows: T[]) => T[]` | Sort/filter/slice the accumulated rows before render. |

```tsx
const { data, loading } = useDrasiQuery('portfolio-query', {
  getKey: (row) => `portfolio-${row.id}`,
  transform: (row) => ({ ...row, quantity: Number(row.quantity) }),
  postProcess: (rows) => rows.sort((a, b) => b.currentValue - a.currentValue),
});
```

### `useDrasiConnectionStatus`

```ts
function useDrasiConnectionStatus(): ConnectionStatus;
// { connected: boolean; reconnecting?: boolean; error?: string }
```

### `useDrasiServerUiUrl`

```ts
function useDrasiServerUiUrl(): string | null;
```

Returns a deep link to the Drasi Server UI for the connected instance, or `null`
before the connection is established.

### `useDrasiQueryDefinition`

```ts
function useDrasiQueryDefinition(queryId: string):
  { config: Record<string, any> | null; loading: boolean };
```

Fetches a query's full configuration from the server (used, for example, to show
the live Cypher definition in the `QueryTable` code viewer).

### `QueryTable`

A sortable, animated table bound to a single query. It calls `useDrasiQuery`
internally, so you only describe how to render rows.

Key props (see `QueryTableProps<T>` for the full list):

| Prop | Type | Description |
| --- | --- | --- |
| `queryId` | `string` | Query to subscribe to. **Required.** |
| `columns` | `ColumnDef<T>[]` | Column definitions. **Required.** |
| `rowKey` | `(row: T) => string` | Unique React key per row. **Required.** |
| `queryOptions` | `UseDrasiQueryOptions<T>` | Forwarded to `useDrasiQuery` (key/transform/sort). |
| `title` | `string` | Card title. |
| `defaultSort` | `{ column: string; direction: 'asc' \| 'desc' }` | Initial sort. |
| `animateOnChange` | `keyof T` | Field whose changes trigger the row flash animation. |
| `actions` | `RowAction<T>[]` | Per‑row action buttons. |
| `headerActions` | `ReactNode` | Header slot (e.g. an “add” button). |
| `emptyMessage` | `string` | Shown when there are no rows. |
| `codeSnippet` | `string` | Consumer code shown in the “view code” dialog. |
| `className` / `tableClassName` / `headerClassName` / `rowClassName` | `string` | Style hooks for full theming. |

`ColumnDef<T>`:

```ts
interface ColumnDef<T> {
  key: keyof T | string;
  label: string;
  format?: (value: any, row: T) => React.ReactNode;
  sortable?: boolean;            // default true
  align?: 'left' | 'center' | 'right';
  className?: string | ((value: any, row: T) => string);
  headerClassName?: string;
  width?: string;
}
```

### Low‑level classes

If you are not using React, or you want full control, the underlying classes are
exported too:

- **`DrasiClient`** — orchestrates query/reaction lifecycle and exposes
  `initialize()`, `subscribe(queryId, cb)`, `getQueryResults(queryId)`,
  `onConnectionStatusChange(cb)`, `getServerUiUrl()`.
- **`DrasiSSEClient`** — the raw multiplexing SSE client used by `DrasiClient`.

```ts
const client = new DrasiClient({ serverUrl, queries, reaction, routeUnidentified });
await client.initialize();
const unsubscribe = client.subscribe('stocks-query', (result) => {
  console.log(result.data);
});
```

## Styling

The components use [Tailwind](https://tailwindcss.com/) utility classes for
layout and a small `styles.css` for the value‑change/dialog animations.

1. Import the animations once:

   ```ts
   import '@drasi/react/styles.css';
   ```

2. If you build your own CSS with Tailwind and consume this package **as source**
   (for example via a path alias in a monorepo), add it to your Tailwind
   `content` globs so the utility classes are generated:

   ```js
   // tailwind.config.js
   module.exports = {
     content: [
       './src/**/*.{js,ts,jsx,tsx}',
       './node_modules/@drasi/react/dist/**/*.js',
     ],
   };
   ```

Every visual element is also overridable through the `className`,
`tableClassName`, `headerClassName`, `rowClassName`, and per‑column `className`
props, so you can match any design system.

## Building from source

```bash
# from this directory (examples/trading/drasi-react)
npm install      # install dev dependencies (TypeScript + React types)
npm run build    # type-check and emit ESM + .d.ts to ./dist
```

Other scripts:

| Script | Description |
| --- | --- |
| `npm run typecheck` | Type-check without emitting. |
| `npm run clean` | Remove `dist/`. |

The build emits ES modules and TypeScript declarations to `dist/`. Both `dist/`
and `node_modules/` are git‑ignored.

## Using it in the Trading example

The Trading dashboard (`examples/trading/app`) consumes this package as
`@drasi/react`. For the demo it is wired up as source so no separate publish/build
step is required:

- **Vite** aliases `@drasi/react` to `../drasi-react/src` (`vite.config.ts`).
- **TypeScript** maps the same path (`tsconfig.json` `paths`).
- **Tailwind** includes `../drasi-react/src/**/*` in its `content` globs.

All trading‑specific behaviour (the query list, the SSE Reaction, the
content‑router for aggregation result changes, and per‑query key/transform/sort
options) lives in the app under `src/drasi/`, demonstrating how an application
supplies its domain knowledge to these otherwise‑generic components.

## License

Apache License 2.0. See the license headers in each source file and the
repository's top‑level `LICENSE`.
