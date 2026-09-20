// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { createRoot } from 'react-dom/client';
import { DrasiProvider, useDrasiClient, useDrasiQuery } from '@drasi/react/react';
import { Connection } from './Connection';
import { Shell } from './Shell';
import { connectionOptions, readingOptions, rooms } from './readings';

function RoomCards({ queryId, title }: { queryId: string; title: string }) {
  const query = useDrasiQuery(queryId, readingOptions);
  const { retry: retryConnection } = useDrasiClient();
  return <section aria-label={title}>
    <h2>{title}</h2>
    <p role="status">{title} query: {query.status}{query.stale ? ' (last-good data)' : ''}</p>
    {query.error && <div role="alert">
      <p>{query.error.code}: {query.error.message}</p>
      <button type="button" onClick={query.errorScope === 'connection' ? retryConnection : query.retry}>
        {query.errorScope === 'connection' ? 'Retry connection' : 'Retry query'}
      </button>
    </div>}
    {query.data?.length === 0 && <p>No readings in this view.</p>}
    <ul>{query.data?.map(row => <li key={row.key}>
      <h3>{row.key}</h3>
      <dl>
        <dt>Room</dt><dd>{row.room}</dd>
        <dt>Temperature (C)</dt><dd>{row.celsius.toFixed(1)}</dd>
      </dl>
    </li>)}</ul>
    <button type="button" disabled={query.loading} onClick={query.retry}>Refresh {title.toLowerCase()}</button>
  </section>;
}

createRoot(document.getElementById('root')!).render(
  <DrasiProvider {...connectionOptions(window.location.origin)}>
    <Shell title="Cold storage: hooks only">
      <p>Custom semantic HTML, no package presentation controls or stylesheet. These are the same real pre-created resources.</p>
      <Connection />
      {rooms.map(room => <RoomCards key={room.queryId} {...room} />)}
    </Shell>
  </DrasiProvider>,
);
