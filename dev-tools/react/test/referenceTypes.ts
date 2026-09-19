// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { createLegacyResultAdapter, sse034ResultAdapter } from '../src';
import type {
  DrasiClientOptions, DrasiError, DrasiProviderProps, ReactionReference,
  QueryDelta, QuerySnapshot, QuerySubscriptionState, ResultAdapter, ResultChange,
  RowKey, UseDrasiQueryOptions,
} from '../src';

const reaction: ReactionReference = { id: 'events', endpoint: 'https://events.example/changes' };
const options: DrasiClientOptions = {
  serverUrl: 'https://drasi.example', instanceId: 'analytics', queryIds: ['readings'], reaction,
};
export const provider: DrasiProviderProps = { ...options, children: null };
export const explicitAdapter: DrasiClientOptions = {
  ...options, resultAdapter: sse034ResultAdapter, reconciliation: { maxPendingChanges: 10000 },
};
export const legacyAdapter: ResultAdapter = createLegacyResultAdapter({
  routeUnidentified: (rows, deliver) => deliver('readings', rows),
});
export const rowKey: RowKey = raw => {
  if (typeof raw.id !== 'string' || !raw.id) throw new Error('Missing reading identity');
  return raw.id;
};
export const rawOptions: UseDrasiQueryOptions = { getKey: rowKey, transform: raw => raw };

// @ts-expect-error Instance identity must be explicitly resolved by the consumer.
export const missingInstance: DrasiClientOptions = { serverUrl: options.serverUrl, queryIds: ['readings'], reaction };
// @ts-expect-error Connection options do not accept deployment definitions.
export const deployment: DrasiClientOptions = { ...options, queries: [{ id: 'readings', query: 'MATCH (n) RETURN n', sources: [] }] };
// @ts-expect-error A server bind port is not a browser endpoint reference.
export const bindSettings: ReactionReference = { id: 'events', port: 8081 };
// @ts-expect-error Legacy routing must be explicitly selected through resultAdapter.
export const topLevelRouting: DrasiClientOptions = { ...options, routeUnidentified() {} };
// @ts-expect-error Raw reads require getKey, not just an identity transform.
export const missingKey: UseDrasiQueryOptions = { transform: raw => raw };
// @ts-expect-error Raw reads require an explicit transform as well as a stable key.
export const missingTransform: UseDrasiQueryOptions = { getKey: rowKey };
// @ts-expect-error Unknown raw fields are not automatically string identities.
export const uncheckedKey: RowKey = raw => raw.id;
// @ts-expect-error Null is not a silently skipped identity.
export const nullKey: RowKey = () => null;
// @ts-expect-error Numeric identity needs an explicit domain string representation.
export const numberKey: RowKey = () => 1;

export const snapshot: QuerySnapshot = { kind: 'snapshot', queryId: 'readings', rows: [], receivedAt: 1 };
export const change: ResultChange = { kind: 'update', before: { id: 'old' }, after: { id: 'new' } };
export const delta: QueryDelta = { kind: 'delta', queryId: 'readings', changes: [change], receivedAt: 2 };
// @ts-expect-error A normalized update requires both sides.
export const missingBefore: ResultChange = { kind: 'update', after: { id: 'new' } };
// @ts-expect-error Deltas cannot masquerade as full snapshots.
export const deltaSnapshot: QuerySnapshot = delta;
export const state: QuerySubscriptionState = {
  status: 'resynchronizing', stale: false, error: null, errorScope: null,
};
// @ts-expect-error Only hooks can determine whether a projected view is empty.
export const emptyState: QuerySubscriptionState = { ...state, status: 'empty' };

export function handleError(error: DrasiError): string {
  const retryable: boolean = error.retryable;
  return error.code === 'QUERY_NOT_FOUND' && !retryable ? error.resourceId ?? '' : error.message;
}
