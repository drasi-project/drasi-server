// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { DrasiError, type QueryStatus } from '@drasi/react/client';
import type { UseDrasiQueryResult } from '@drasi/react/react';
import type { Reading } from './readings';

export const simulatedStates: readonly QueryStatus[] = [
  'initial-loading', 'empty', 'live', 'reconnecting', 'resynchronizing',
  'stale-last-good-data', 'terminal-error',
];
export const simulatedRows: Reading[] = [
  { key: 'sim-probe-102', room: 'South', celsius: 5 },
  { key: 'sim-probe-101', room: 'North', celsius: 3 },
];

export function simulatedQuery(status: QueryStatus, rows: Reading[], retry: () => void): UseDrasiQueryResult<Reading> {
  const stale = ['reconnecting', 'resynchronizing', 'stale-last-good-data', 'terminal-error'].includes(status);
  return {
    data: status === 'initial-loading' ? null : status === 'empty' ? [] : rows,
    status,
    stale,
    loading: ['initial-loading', 'reconnecting', 'resynchronizing'].includes(status),
    error: status === 'terminal-error' ? new DrasiError('INVALID_PAYLOAD') :
      status === 'stale-last-good-data' ? new DrasiError('SERVER_UNAVAILABLE') : null,
    errorScope: status === 'terminal-error' || status === 'stale-last-good-data' ? 'query' : null,
    lastUpdate: status === 'initial-loading' ? null : new Date('2026-01-15T12:00:00Z'),
    retry,
  };
}
