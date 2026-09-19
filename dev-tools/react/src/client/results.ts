// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiError, asDrasiError, type DrasiErrorDetails } from './errors';
import { isIdentifier, isRecord, readRows } from './resources';
import type {
  LegacyResultAdapterOptions, QueryDelta, ResultAdapter, ResultAdapterContext, ResultChange, ResultRow,
} from './types';

function invalid(details: DrasiErrorDetails): never {
  throw new DrasiError('INVALID_PAYLOAD', details);
}

function row(value: unknown, details: DrasiErrorDetails): ResultRow {
  return readRows([value], details)[0];
}

function timestamp(value: unknown, details: DrasiErrorDetails): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return invalid(details);
  return value;
}

function identity(payload: ResultRow, legacy: boolean, context: ResultAdapterContext): string | undefined {
  const id = payload.queryId ?? (legacy ? payload.query_id : undefined);
  if ((payload.queryId !== undefined && !isIdentifier(payload.queryId)) ||
      (payload.query_id !== undefined && (!legacy || !isIdentifier(payload.query_id))) ||
      (payload.queryId !== undefined && payload.query_id !== undefined && payload.queryId !== payload.query_id)) {
    return invalid(context);
  }
  return isIdentifier(id) ? id : undefined;
}

function detailsFor(queryId: string | undefined, context: ResultAdapterContext): DrasiErrorDetails {
  return queryId === undefined ? context
    : { instanceId: context.instanceId, resourceKind: 'query', resourceId: queryId };
}

function officialChange(value: unknown, details: DrasiErrorDetails): ResultChange | null {
  if (!isRecord(value)) return invalid(details);
  // u64 signatures lose precision in JSON.parse and are absent from REST rows.
  // Validate only their wire type; never expose or use them as identity.
  if (value.row_signature !== undefined &&
      (typeof value.row_signature !== 'number' || !Number.isInteger(value.row_signature) || value.row_signature < 0)) {
    return invalid(details);
  }
  switch (value.type) {
    case 'ADD': return { kind: 'upsert', after: row(value.data, details) };
    case 'DELETE': return { kind: 'delete', before: row(value.data, details) };
    case 'UPDATE':
      row(value.data, details);
      if (value.grouping_keys !== undefined &&
          (!Array.isArray(value.grouping_keys) || !value.grouping_keys.every(key => typeof key === 'string'))) {
        return invalid(details);
      }
      return { kind: 'update', before: row(value.before, details), after: row(value.after, details) };
    case 'aggregation':
      return value.before === null
        ? { kind: 'upsert', after: row(value.after, details) }
        : { kind: 'update', before: row(value.before, details), after: row(value.after, details) };
    case 'noop': return null;
    default: return invalid(details);
  }
}

/**
 * Default SSE 0.3.4 untemplated wire format: queryId/results/timestamp and
 * ADD, DELETE, UPDATE, aggregation, noop. No field-shape or timestamp routing.
 * Custom plugin templates are not this protocol.
 */
export const sse034ResultAdapter: ResultAdapter = (payload, context) => {
  if (!isRecord(payload)) return invalid(context);
  const queryId = identity(payload, false, context);
  if (payload.type === 'heartbeat' && queryId === undefined) {
    if (Object.keys(payload).some(key => key !== 'type' && key !== 'ts')) return invalid(context);
    timestamp(payload.ts, context);
    return [];
  }
  const details = detailsFor(queryId, context);
  if (queryId === undefined || payload.type !== undefined || !Array.isArray(payload.results) ||
      ['data', 'addedResults', 'updatedResults', 'deletedResults'].some(key => payload[key] !== undefined)) {
    return invalid(details);
  }
  const sourceTimestamp = timestamp(payload.timestamp, details);
  const changes = payload.results.map(value => officialChange(value, details))
    .filter((change): change is ResultChange => change !== null);
  return [{ kind: 'delta', queryId, changes, receivedAt: context.receivedAt, sourceTimestamp }];
};

function legacyRow(value: unknown, details: DrasiErrorDetails): ResultChange {
  const data = row(value, details);
  if (data._deleted !== undefined && typeof data._deleted !== 'boolean') return invalid(details);
  const { _deleted, ...clean } = data;
  return _deleted === true ? { kind: 'delete', before: clean } : { kind: 'upsert', after: clean };
}

function legacyChange(value: unknown, details: DrasiErrorDetails): ResultChange | null {
  if (!isRecord(value)) return invalid(details);
  if (value.op !== undefined) {
    if (value.op === 'd' || (value.op === 'u' && value.after == null)) {
      return { kind: 'delete', before: row(value.before, details) };
    }
    if (value.op === 'c' || value.op === 'r' || value.op === 'u') {
      const after = row(value.after, details);
      return value.op === 'u' && value.before != null
        ? { kind: 'update', before: row(value.before, details), after }
        : { kind: 'upsert', after };
    }
    return invalid(details);
  }
  switch (value.type) {
    case 'DELETE': case 'delete':
      return { kind: 'delete', before: row(value.before ?? value.data, details) };
    case 'ADD': case 'add':
      return { kind: 'upsert', after: row(value.data, details) };
    case 'UPDATE': case 'update': case 'aggregation': {
      const after = row(value.after, details);
      return value.before == null ? { kind: 'upsert', after }
        : { kind: 'update', before: row(value.before, details), after };
    }
    case 'noop': return null;
    case undefined: return legacyRow(value.data === undefined ? value : value.data, details);
    default: return invalid(details);
  }
}

function legacyBatch(payload: ResultRow, details: DrasiErrorDetails): ResultChange[] {
  const changes: ResultChange[] = [];
  for (const kind of ['addedResults', 'updatedResults', 'deletedResults'] as const) {
    const values = payload[kind];
    if (values === undefined) continue;
    if (!Array.isArray(values)) return invalid(details);
    for (const value of values) {
      const item = row(value, details);
      if (kind === 'deletedResults') {
        changes.push({ kind: 'delete', before: row(item.before === undefined ? item : item.before, details) });
      } else {
        const after = row(item.after === undefined ? item : item.after, details);
        changes.push(kind === 'updatedResults' && item.before != null
          ? { kind: 'update', before: row(item.before, details), after }
          : { kind: 'upsert', after });
      }
    }
  }
  return changes;
}

/**
 * Explicit migration adapter, not an alternative official protocol. Accepts
 * query_id, op c/r/u/d, lowercase operations, keyed data/_deleted, and
 * addedResults/updatedResults/deletedResults. Unidentified routing is app-owned.
 * The legacy row callback cannot preserve key-changing before/after updates:
 * these are delivered to it as an explicit delete followed by an upsert.
 */
export function createLegacyResultAdapter(options: LegacyResultAdapterOptions = {}): ResultAdapter {
  return (payload, context) => {
    if (!isRecord(payload)) return invalid(context);
    const queryId = identity(payload, true, context);
    const details = detailsFor(queryId, context);
    if (payload.type === 'heartbeat' && queryId === undefined) {
      if (Object.keys(payload).some(key => key !== 'type' && key !== 'ts')) return invalid(details);
      if (payload.ts !== undefined) timestamp(payload.ts, details);
      return [];
    }
    let sourceTimestamp: number | undefined;
    if (payload.timestamp !== undefined) {
      sourceTimestamp = typeof payload.timestamp === 'string'
        ? timestamp(Date.parse(payload.timestamp), details) : timestamp(payload.timestamp, details);
    }
    const batch = ['addedResults', 'updatedResults', 'deletedResults'].some(key => payload[key] !== undefined);
    let changes: ResultChange[];
    if (batch) {
      if (payload.results !== undefined || payload.data !== undefined) return invalid(details);
      changes = legacyBatch(payload, details);
    } else if (payload.results !== undefined) {
      if (!Array.isArray(payload.results) || payload.data !== undefined) return invalid(details);
      changes = payload.results.map(value => legacyChange(value, details))
        .filter((change): change is ResultChange => change !== null);
    } else if (queryId !== undefined && payload.data !== undefined) {
      if (payload.type !== undefined && (typeof payload.type !== 'string' ||
          !['add', 'ADD', 'update', 'UPDATE', 'delete', 'DELETE'].includes(payload.type))) {
        return invalid(details);
      }
      changes = (Array.isArray(payload.data) ? payload.data : [payload.data]).map(value =>
        payload.type === 'delete' || payload.type === 'DELETE'
          ? { kind: 'delete', before: row(value, details) } : legacyRow(value, details));
    } else if (queryId === undefined && Object.keys(payload).length > 0) {
      changes = [legacyRow(payload, details)];
    } else return invalid(details);
    if (queryId !== undefined) {
      return [{ kind: 'delta', queryId, changes, receivedAt: context.receivedAt, sourceTimestamp }];
    }
    if (!changes.length) return [];
    if (!options.routeUnidentified) throw new DrasiError('UNROUTABLE_RESULT', details);
    const rows = changes.flatMap(change => change.kind === 'upsert' ? [change.after]
      : change.kind === 'delete' ? [{ ...change.before, _deleted: true }]
      : [{ ...change.before, _deleted: true }, change.after]);
    const routed: QueryDelta[] = [];
    const expectedRows = [...rows];
    const delivered = new Set<ResultRow>();
    try {
      options.routeUnidentified(rows, (id, selected) => {
        if (!isIdentifier(id) || !Array.isArray(selected) || selected.some(item => !expectedRows.includes(item))) {
          throw new DrasiError('UNROUTABLE_RESULT', details);
        }
        selected.forEach(item => delivered.add(item));
        routed.push({
          kind: 'delta', queryId: id, changes: selected.map(item => legacyRow(item, details)),
          receivedAt: context.receivedAt, sourceTimestamp,
        });
      });
    } catch (error) {
      throw asDrasiError(error, details, 'UNROUTABLE_RESULT');
    }
    if (expectedRows.some(item => !delivered.has(item))) throw new DrasiError('UNROUTABLE_RESULT', details);
    return routed;
  };
}

/** Validate even custom adapters: a TypeScript declaration is not a runtime boundary. */
export function readAdaptedResults(value: unknown, context: ResultAdapterContext): readonly QueryDelta[] {
  if (!Array.isArray(value)) return invalid(context);
  return value.map(item => {
    if (!isRecord(item) || item.kind !== 'delta' || !isIdentifier(item.queryId) || !Array.isArray(item.changes)) {
      return invalid(context);
    }
    const details = detailsFor(item.queryId, context);
    const changes: ResultChange[] = item.changes.map(change => {
      if (!isRecord(change)) return invalid(details);
      switch (change.kind) {
        case 'upsert': return { kind: 'upsert', after: row(change.after, details) };
        case 'delete': return { kind: 'delete', before: row(change.before, details) };
        case 'update': return { kind: 'update', before: row(change.before, details), after: row(change.after, details) };
        default: return invalid(details);
      }
    });
    return {
      kind: 'delta', queryId: item.queryId, changes, receivedAt: timestamp(item.receivedAt, details),
      ...(item.sourceTimestamp === undefined ? {} : { sourceTimestamp: timestamp(item.sourceTimestamp, details) }),
    };
  });
}
