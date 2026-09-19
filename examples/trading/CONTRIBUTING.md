# Contributing to the Trading Demo

This document contains suggestions for improving the Trading Demo and exercises to help you learn Drasi by contributing.

Whether you're learning Drasi or looking to contribute to the project, these ideas range from beginner-friendly enhancements to more advanced features.

Before changing a Trading component or its `@drasi/react` integration, run the
checks in [Testing and behavior baseline](TESTING.md). The versioned inventory
distinguishes intended behavior, compatibility assertions and known baseline
bugs; do not update visual expectations to hide an unexplained regression.

## Table composition boundaries

Trading is the only example consumer in P5 ([#164 Part A](https://github.com/drasi-project/drasi-server/issues/164)).
Keep domain definitions, transformations, filtering, default sorts and tutorial
snippets in this app. Do not change the eleven Cypher queries or the known
market-mover default ordering as part of a presentation refactor.

| Need | Owner |
| --- | --- |
| Query rows, status, last-good data and query-local retry | `useDrasiQuery` from `@drasi/react/react`; raw `getKey` and validating `transform` stay explicit. |
| A live table without tutorials or overlays | `QueryTable` from `@drasi/react/components`; it composes the hook and `DataTable`, not the Trading wrapper. |
| Provider-free presentation for supplied rows | `DataTable` from `@drasi/react/components`; columns, transformed `rowKey`, state slots, typed cells/actions/rows/header and controlled/uncontrolled sort. |
| Trading fullscreen, tutorial code and server-UI links | App-owned `TradingQueryTable.tsx`, `QueryInspector.tsx` and `CodeViewerDialog.tsx`. The wrapper reuses `DataTable`, not a second table implementation. |

`TradingQueryTable` shares one headless `useTableSort` controller and one
animation map across its normal/fullscreen presentations. `sort={null}` clears
sorting; omitting `sort` uses a mount-time `defaultSort`. Never call an external
`onSortChange` inside a state updater. The callback receives a sort or `null`
exactly once per action, including under StrictMode.

Use the package's `queryTableState(query, retryConnection)` adapter when
composing headless query state with `DataTable`. A query fault refreshes only
that query; a shared failure delegates to `useDrasiClient().retry`. Retain and
label last-good rows instead of disguising an error as an empty result.

The inspector is mounted only while its code action is open. Its extra
definition GET is distinct from required client resource-validation GETs.
Closing cancels the read; asynchronous results update the open viewer and its
copy text. A failed definition read can retry without resubscribing a table;
shared connection failures still use the connection owner's retry. Keep
displayed tutorial snippets aligned with the app-owned wrapper.

Import client, hooks and presentation through their independent entrypoints,
and opt into `@drasi/react/styles.css` explicitly. P5 preserves existing CSS,
markup, animations and all five original visual PNG images. P6 owns the
remaining focus, shared overlay/scroll-lock, theming/portal, reduced-motion and
height contracts; do not migrate unrelated forms or claim those checks here.
No standalone example app or Storybook site is needed before #165.

## Learning Exercises

These exercises are designed to help you understand Drasi by making small, focused changes to the trading demo.

### Beginner

#### Exercise 1: Add a New Stock to the Watchlist

**Goal**: Understand how queries filter data.

1. Open `app/src/services/queries.ts`
2. Find the `watchlist-query` definition
3. Add `'AMZN'` to the `WHERE s.symbol IN [...]` clause
4. Restart the app and observe the new stock appear

**What you'll learn**: How Cypher WHERE clauses filter continuous query results.

#### Exercise 2: Change the High Volume Threshold

**Goal**: Understand query conditions and result set changes.

1. Find `high-volume-query` in `queries.ts`
2. Change `sp.volume > 10000000` to `sp.volume > 5000000`
3. Watch how more stocks now qualify for the "High Volume" panel

**What you'll learn**: How changing query conditions affects which rows enter/exit result sets.

#### Exercise 3: Add a New Field to the Ticker

**Goal**: Understand query RETURN clauses.

1. Find `price-ticker-query` in `queries.ts`
2. Add `sp.volume AS volume` to the RETURN clause
3. Update `StockTicker.tsx` to display the volume

**What you'll learn**: How to extend query results with additional fields.

### Intermediate

#### Exercise 4: Create a Sector Filter Query

**Goal**: Build a new query from scratch.

Create a query that shows only Technology stocks:

Add a new query definition to `app/src/services/queries.ts` and include it in
the exported `ALL_QUERIES` array. `app/src/drasi/config.ts` projects the app-owned
deployment bodies and query IDs; `TradingProvider` passes only references to the
connect-only package and delegates eligible setup to `ensureTradingResources`.
Use a fresh disposable instance for query-definition exercises, or explicitly
apply the intended changes with server tooling. Restarting the app never
overwrites existing definitions or rewrites reaction membership; conflicts are
surfaced rather than silently reconciled.

```typescript
export const TECH_STOCKS_QUERY: QueryDefinition = {
  id: 'tech-stocks-query',
  description: 'Technology sector stocks',
  query: `
    MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WHERE s.sector = 'Technology'
    RETURN s.symbol AS symbol,
           s.name AS name,
           sp.price AS price,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [HAS_PRICE]
};
```

Then create a new UI panel to display it.

**What you'll learn**: The full cycle of creating queries and connecting them to UI components.

#### Exercise 5: Implement a Price Alert

**Goal**: Understand ADD and DELETE events in continuous queries.

Create a query that returns stocks crossing a price threshold:

```typescript
// Stocks that just crossed above $200
MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
WHERE sp.price > 200 AND sp.previous_close <= 200
RETURN s.symbol, sp.price
```

**What you'll learn**: How rows enter (ADD) and exit (DELETE) query result sets based on conditions.

#### Exercise 6: Add Database Write Buttons

**Goal**: Understand PostgreSQL CDC and how database changes trigger query updates.

Add "Buy" and "Sell" buttons to the Portfolio panel:

1. Create an API endpoint or direct database connection
2. INSERT into the `portfolio` table when buying
3. DELETE from the `portfolio` table when selling
4. Watch the portfolio-query update automatically via CDC

**What you'll learn**: How PostgreSQL logical replication captures changes and feeds them to Drasi.

### Advanced

#### Exercise 7: Create a Query Inspector Panel

**Goal**: Deep understanding of SSE events and query result changes.

Build a developer panel that shows:

- Raw SSE events as they arrive
- Event types (ADD/UPDATE/DELETE) with visual indicators
- Which queries are firing and when
- Latency from price change to UI update

**What you'll learn**: The internal mechanics of how Drasi delivers change notifications.

#### Exercise 8: Implement Query Parameters

**Goal**: Understand dynamic query modification.

Allow users to change query parameters at runtime:

1. Add a form to set the watchlist symbols
2. Recreate the query with new parameters
3. Handle the transition smoothly in the UI

**What you'll learn**: Query lifecycle management and dynamic query creation.

#### Exercise 9: Add a Second Reaction Type

**Goal**: Understand Drasi's reaction system.

Add a webhook reaction alongside SSE:

1. Create an HTTP reaction that POSTs to a local endpoint
2. Build a simple server to receive the webhooks
3. Compare the data format between SSE and HTTP reactions

**What you'll learn**: How different reaction types serve different use cases.

## Feature Contributions

These are more substantial features that would improve the demo for everyone.

### High Priority

#### 1. Buy/Sell Functionality

**Impact**: Demonstrates full CDC capability with user-initiated database writes.

**Implementation**:

- Add buy/sell buttons to stock rows
- Create backend endpoint to modify `portfolio` table
- Show toast notifications when transactions complete
- Watch portfolio update via CDC (not manual refresh)

**Files to modify**:

- `app/src/components/StockList.tsx` - Add action buttons
- `database/init.sql` - Add transaction history table (optional)
- New: `app/src/services/TradingService.ts` - API calls

#### 2. Result Event Debug Panel

**Impact**: Educational tool showing Drasi internals.

**Implementation**:

- Toggle-able panel showing raw SSE events
- Color-coded event types (green=ADD, yellow=UPDATE, red=DELETE)
- Event counter per query
- Timestamp and latency display

**Files to modify**:

- New: `app/src/components/ResultDebugPanel.tsx` (the existing `QueryInspector.tsx` is the lazy tutorial definition viewer)
- `dev-tools/react/src/client/DrasiSSEClient.ts` - Add event hooks
- `app/src/App.tsx` - Add toggle button

#### 3. Custom Screener Builder

**Impact**: Lets users experiment with Cypher without editing code.

**Implementation**:

- Form with dropdowns for common conditions
- "Create Query" button that calls the REST API
- Dynamic panel showing custom query results
- "Delete Query" to clean up

**Files to modify**:

- New: `app/src/components/ScreenerBuilder.tsx`
- `app/src/services/queries.ts` - Add the query definition and call the REST API to create it dynamically

### Medium Priority

#### 4. Historical Spark Lines

**Impact**: Visual context for price movements.

**Implementation**:

- Store last N price updates per symbol in React state
- Render mini line charts in stock rows
- Use a lightweight chart library (recharts is already installed)

**Files to modify**:

- `app/src/drasi/queryOptions.ts` - Track price history via a query's `transform`/`postProcess`
- `app/src/components/StockList.tsx` - Add sparkline column

#### 5. Connection Status Improvements

**Impact**: Better UX during network issues.

**Implementation**:

- Toast notifications on disconnect/reconnect
- Retry counter display
- "Reconnecting in X seconds" message
- Manual reconnect button

**Files to modify**:

- `dev-tools/react/src/client/DrasiSSEClient.ts` - Expose more status info
- `app/src/App.tsx` - Add notification system

#### 6. Mobile-Responsive Layout

**Impact**: Demo works on phones/tablets.

**Implementation**:

- Responsive grid adjustments
- Collapsible panels
- Touch-friendly interactions
- Bottom navigation for mobile

**Files to modify**:

- `app/src/App.tsx` - Responsive layout
- `app/tailwind.config.js` - Breakpoints
- Various component files

### Lower Priority / Exploratory

#### 7. Temporal Query Demo

**Impact**: Shows Drasi's time-window capabilities.

**Implementation**:

- Query: "Stocks up 5% in last hour but now declining"
- Requires understanding Drasi's temporal features
- New panel showing time-based alerts

#### 8. Multi-User Portfolio

**Impact**: Shows query parameterization.

**Implementation**:

- Add user selector dropdown
- Filter portfolio-query by user_id
- Show different portfolios for different users

#### 9. Performance Metrics Dashboard

**Impact**: Demonstrates Drasi's efficiency.

**Implementation**:

- Count events from sources
- Count query evaluations
- Count events sent to clients
- Show the reduction ratio

#### 10. Alternative Data Source

**Impact**: Shows Drasi's multi-source flexibility.

**Ideas**:

- Add a gRPC source for news headlines
- Add a mock WebSocket source for order book data
- Join news sentiment with price data

## Documentation Contributions

### Improve Inline Code Comments

Add JSDoc comments explaining key concepts:

```typescript
/**
 * Synthetic joins define relationships that don't exist in the database.
 * This HAS_PRICE join connects stocks (PostgreSQL) to prices (HTTP source)
 * by matching on the 'symbol' property in both node types.
 *
 * When Drasi sees a stock with symbol='AAPL' and a price with symbol='AAPL',
 * it creates a virtual HAS_PRICE edge between them.
 */
const hasPrice: QueryJoin = {
  id: 'HAS_PRICE',
  keys: [
    { label: 'stocks', property: 'symbol' },
    { label: 'stock_prices', property: 'symbol' }
  ]
};
```