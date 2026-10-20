// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { DrasiClient, DrasiError } from '@drasi/react/client';
import { ensureTradingResources } from '../src/drasi/ensureTradingResources';
import { DRASI_SERVER_URL, TRADING_QUERY_IDS, TRADING_STREAM } from '../src/drasi/config';

const client = new DrasiClient({
  serverUrl: DRASI_SERVER_URL, instanceId: 'trading-server',
  queryIds: TRADING_QUERY_IDS, reaction: TRADING_STREAM, fetch,
});
await ensureTradingResources({ client, serverUrl: DRASI_SERVER_URL, fetch },
  new DrasiError('QUERY_NOT_FOUND', {
    instanceId: 'trading-server', resourceKind: 'query', resourceId: TRADING_QUERY_IDS[0],
  }), new AbortController().signal);
console.log(JSON.stringify({ ready: true, queries: TRADING_QUERY_IDS }));
