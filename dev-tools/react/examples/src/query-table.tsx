// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { DrasiProvider, type UseDrasiQueryOptions, type UseDrasiQueryResult } from '@drasi/react/react';
import { QueryTable } from '@drasi/react/components';
import '@drasi/react/styles.css';
import './layout.css';
import { Connection } from './Connection';
import { Shell } from './Shell';
import { columns, rowKey } from './columns';
import { connectionOptions, readingOptions, rooms, type Reading } from './readings';

function QueryNotice({ title, query }: { title: string; query: UseDrasiQueryResult<Reading> }) {
  return <p role="status">
    {title} query: {query.status}{query.stale ? ' (last-good data)' : ''}
  </p>;
}

function RoomQueryTable({ queryId, title, options = readingOptions }: {
  queryId: string; title: string; options?: UseDrasiQueryOptions<Reading>;
}) {
  return <section aria-label={title}>
    <QueryTable<Reading>
      queryId={queryId} queryOptions={options}
      title={title} columns={columns} rowKey={rowKey} height="18rem"
      animateOnChange="celsius"
      renderLoading={({ query }) => <QueryNotice title={title} query={query} />}
      renderEmpty={({ query }) => <>
        <QueryNotice title={title} query={query} />
        No readings in this view.
      </>}
      renderStale={({ query }) => <QueryNotice title={title} query={query} />}
      renderError={({ error, query, retryConnection }) => <div role="alert">
        <QueryNotice title={title} query={query} />
        <p>{error.code}: {error.message}</p>
        <button type="button" onClick={query.errorScope === 'connection' ? retryConnection : query.retry}>
          {query.errorScope === 'connection' ? 'Retry connection' : 'Retry query'}
        </button>
      </div>}
    />
  </section>;
}

const rejectedProjection: UseDrasiQueryOptions<Reading> = {
  ...readingOptions,
  transform: () => { throw new Error('Deliberate client-only projection failure'); },
};

function QueryTableExample() {
  const [rejectNorth, setRejectNorth] = useState(false);
  return <DrasiProvider {...connectionOptions(window.location.origin)}>
    <Shell title="Cold storage: QueryTable">
      <p>Two real pre-created queries share one connection. Each QueryTable owns its
        query hook; slots expose state and scoped retry without a second subscription.</p>
      <Connection />
      <div className="example-grid">
        <RoomQueryTable {...rooms[0]} options={rejectNorth ? rejectedProjection : readingOptions} />
        <RoomQueryTable {...rooms[1]} />
      </div>
      <label>
        <input type="checkbox" checked={rejectNorth} onChange={event => setRejectNorth(event.target.checked)} />
        Reject North projection (client-only demo)
      </label>
      <p>This deliberate client error does not alter server rows or the stream.
        Retry query repeats only North's read; it cannot fix a bad projection.
        Uncheck to restore the validating projection. South stays live.</p>
    </Shell>
  </DrasiProvider>;
}

createRoot(document.getElementById('root')!).render(<QueryTableExample />);
