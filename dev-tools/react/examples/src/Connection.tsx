// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { useDrasiClient, useDrasiConnectionStatus, type UseDrasiQueryResult } from '@drasi/react/react';

export function QueryNotice({ title, query }: {
  title: string; query: Pick<UseDrasiQueryResult, 'status' | 'stale'>;
}) {
  return <p role="status">
    {title} query: {query.status}{query.stale ? ' (last-good data)' : ''}
  </p>;
}

export function Connection() {
  const connection = useDrasiConnectionStatus();
  const { error, retry } = useDrasiClient();
  return <section aria-label="Connection">
    <p role="status">Connection: {connection.connected ? 'stream open' :
      connection.reconnecting ? 'reconnecting' : error ? 'failed' : 'connecting'}</p>
    {error && <div role="alert">
      <p>{error.code}: {error.message}</p>
      <p>Start the example server with its cold-chain configuration. The browser never creates or starts resources.</p>
      <button type="button" onClick={retry}>Retry connection</button>
    </div>}
    <p>An open stream is not a synchronized query. Each view reports its own state below.</p>
  </section>;
}
