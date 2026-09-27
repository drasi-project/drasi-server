// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { createRoot } from 'react-dom/client';
import { DrasiProvider, useDrasiClient, useDrasiQuery } from '@drasi/react/react';
import { DataTable, queryTableState } from '@drasi/react/components';
import type { DrasiError } from '@drasi/react/client';
import '@drasi/react/styles.css';
import './layout.css';
import { Connection } from './Connection';
import { Shell } from './Shell';
import { columns, rowKey } from './columns';
import { connectionOptions, readingOptions, rooms, type Reading } from './readings';

function RoomTable({ queryId, title }: { queryId: string; title: string }) {
  const query = useDrasiQuery(queryId, readingOptions);
  const { retry } = useDrasiClient();
  return <section aria-label={title}>
    <p role="status">{title} query: {query.status}{query.stale ? ' (last-good data)' : ''}</p>
    <DataTable<Reading, DrasiError>
      title={title} rows={query.data} columns={columns} rowKey={rowKey}
      state={queryTableState(query, retry)} height="18rem"
      animateOnChange="celsius"
      renderError={({ error, state }) => <div role="alert">
        <p>{error.code}: {error.message}</p>
        {state.stale && <p>Showing last known readings.</p>}
        <button type="button" onClick={state.retry}>{state.retryLabel}</button>
      </div>}
    />
  </section>;
}

createRoot(document.getElementById('root')!).render(
  <DrasiProvider {...connectionOptions(window.location.origin)}>
    <Shell title="Cold storage: live table">
      <p>Real, pre-created queries in instance <code>cold-chain</code>. Two same-shaped views share one stream.</p>
      <Connection />
      <div className="example-grid">
        {rooms.map(room => <RoomTable key={room.queryId} {...room} />)}
      </div>
    </Shell>
  </DrasiProvider>,
);
