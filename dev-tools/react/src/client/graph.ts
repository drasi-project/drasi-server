// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiError, type DrasiErrorDetails } from './errors';
import { isIdentifier, isRecord } from './resources';
import type {
  ElementIdentity, GraphResultChange, OutputDelta, ResultAdapter, ResultAdapterContext, ResultChange,
  ResultRow, RowKey,
} from './types';

/*
 * Standard graph changes from a native sink, from any producer (a source, a
 * transformer or a query). Each change states its real operation, its canonical
 * element identity and only the images the record carries. The row state that
 * applies them lives here, in the client; the server keeps none.
 */

/** Whether a change is a standard graph change keyed by canonical identity. */
export function isGraphChange(change: ResultChange): change is GraphResultChange {
  return change.kind.startsWith('graph-');
}

const identities = new WeakMap<object, ElementIdentity>();

function invalid(details: DrasiErrorDetails): never {
  throw new DrasiError('INVALID_PAYLOAD', details);
}

/** Unambiguous map key for a canonical identity. The pair is never hashed or truncated. */
export function elementKey(identity: ElementIdentity): string {
  return `${identity.namespace.length}:${identity.namespace}${identity.id}`;
}

/** A row that remembers the canonical identity of the element it was built from. */
export function graphRow(identity: ElementIdentity, properties: ResultRow): ResultRow {
  const row = { ...properties };
  identities.set(row, { namespace: identity.namespace, id: identity.id });
  return row;
}

/** The canonical identity of a row built by this module, if any. */
export function graphRowIdentity(row: Readonly<ResultRow>): ElementIdentity | undefined {
  return identities.get(row);
}

/**
 * Use as `getKey` for rows from a graph producer: the canonical identity itself.
 * Rows that were not built from a graph change have no identity and are rejected.
 */
export const graphRowKey: RowKey = row => {
  const identity = identities.get(row);
  if (!identity) throw new DrasiError('INVALID_ROW_KEY');
  return elementKey(identity);
};

function identityOf(value: unknown, details: DrasiErrorDetails): ElementIdentity {
  if (!isRecord(value) || typeof value.namespace !== 'string' || typeof value.id !== 'string' ||
      value.namespace.length === 0 || value.id.length === 0) {
    return invalid(details);
  }
  return { namespace: value.namespace, id: value.id };
}

function properties(value: unknown, details: DrasiErrorDetails): ResultRow {
  // A property value may be null; the image itself is an object, never null.
  if (!isRecord(value) || Array.isArray(value)) return invalid(details);
  return value;
}

/**
 * Validate one wire change. `before` and `after` are present only when the
 * record carries them: a present `null` image is malformed, an absent one is
 * unknown. A property whose value is `null` stays `null`.
 */
export function readGraphChange(value: unknown, details: DrasiErrorDetails = {}): GraphResultChange {
  if (!isRecord(value)) return invalid(details);
  const identity = identityOf(value.identity, details);
  const optional = (name: 'before' | 'after') =>
    name in value ? properties(value[name], details) : undefined;
  const known = ['operation', 'identity', 'element', 'before', 'after', 'removedProperties', 'effectiveFrom'];
  if (Object.keys(value).some(key => !known.includes(key))) return invalid(details);
  const removed = value.removedProperties;
  const before = optional('before');
  const after = optional('after');
  switch (value.operation) {
    case 'ADD':
      if (!after || removed !== undefined) return invalid(details);
      return { kind: 'graph-add', identity, after };
    case 'REPLACE':
      if (!after || removed !== undefined) return invalid(details);
      return { kind: 'graph-replace', identity, after, ...(before ? { before } : {}) };
    case 'PATCH': {
      if (!after || !Array.isArray(removed) || removed.some(name => typeof name !== 'string')) {
        return invalid(details);
      }
      const names = removed as string[];
      if (names.some(name => name in after)) return invalid(details);
      return { kind: 'graph-patch', identity, set: after, removed: names, ...(before ? { before } : {}) };
    }
    case 'DELETE':
      if (after || removed !== undefined) return invalid(details);
      return { kind: 'graph-delete', identity, ...(before ? { before } : {}) };
    default:
      return invalid(details);
  }
}

/**
 * Apply one change to identity-keyed row state.
 *
 * Add and replace state the complete element, so they need no base row and
 * repeating one is idempotent. A patch needs its base: without it the state has
 * diverged, which is an explicit error that makes the subscription resynchronize.
 * A deletion of an absent element is a no-op. A patch keeps an explicit `null`
 * apart from a removed property.
 */
export function applyGraphChange(
  rows: Map<string, ResultRow>,
  change: GraphResultChange,
  details: DrasiErrorDetails = {},
): void {
  const key = elementKey(change.identity);
  switch (change.kind) {
    case 'graph-add':
    case 'graph-replace':
      rows.set(key, graphRow(change.identity, change.after));
      return;
    case 'graph-patch': {
      const base = rows.get(key);
      if (!base) throw new DrasiError('MISSING_BASE_ROW', details);
      const next: ResultRow = { ...base, ...change.set };
      for (const name of change.removed) delete next[name];
      rows.set(key, graphRow(change.identity, next));
      return;
    }
    case 'graph-delete':
      rows.delete(key);
      return;
  }
}

/** Rows of a graph snapshot: an array of `ADD` changes, each with its canonical identity. */
export function readGraphSnapshot(value: unknown, details: DrasiErrorDetails = {}): ResultRow[] {
  if (!Array.isArray(value)) return invalid(details);
  const seen = new Set<string>();
  return value.map(item => {
    const change = readGraphChange(item, details);
    if (change.kind !== 'graph-add') return invalid(details);
    const key = elementKey(change.identity);
    if (seen.has(key)) return invalid(details);
    seen.add(key);
    return graphRow(change.identity, change.after);
  });
}

function timestamp(value: unknown, details: DrasiErrorDetails): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return invalid(details);
  return value;
}

/**
 * The native SSE sink's default frame:
 * `{output: {alias, component, port}, sequence, publishedAt, changes}`.
 * `output` is the real component output port that produced the data; its
 * `alias` is the sink's route name and the key a subscription uses. `publishedAt`
 * is the sink's own publication time in milliseconds, not an event time. A
 * heartbeat is `{type:'heartbeat', ts}`. Any other shape, including a frame that
 * still names a query or producer, is invalid.
 */
export const nativeGraphResultAdapter: ResultAdapter = (payload, context: ResultAdapterContext) => {
  if (!isRecord(payload)) return invalid(context);
  if (payload.type === 'heartbeat' && payload.output === undefined) {
    if (Object.keys(payload).some(key => key !== 'type' && key !== 'ts')) return invalid(context);
    timestamp(payload.ts, context);
    return [];
  }
  const output = payload.output;
  if (!isRecord(output) || !isIdentifier(output.alias) || !isIdentifier(output.component) ||
      !isIdentifier(output.port) || Object.keys(output).length !== 3) {
    return invalid(context);
  }
  const details: DrasiErrorDetails = {
    instanceId: context.instanceId, resourceKind: 'query', resourceId: output.alias,
  };
  const keys = Object.keys(payload);
  if (keys.some(key => !['output', 'sequence', 'publishedAt', 'changes'].includes(key)) ||
      !Array.isArray(payload.changes) || !Number.isSafeInteger(payload.sequence)) {
    return invalid(details);
  }
  const sourceTimestamp = timestamp(payload.publishedAt, details);
  const delta: OutputDelta = {
    kind: 'delta',
    outputId: output.alias,
    changes: payload.changes.map(change => readGraphChange(change, details)),
    receivedAt: context.receivedAt,
    sourceTimestamp,
  };
  return [delta];
};

/**
 * A native sink's snapshot response. A bare JSON array of `ADD` changes is the
 * only success. A missing read capability (501) is its own explicit error rather
 * than an empty snapshot; an alias outside the sink's bindings is incompatible.
 */
export async function readGraphSnapshotResponse(response: Response, details: DrasiErrorDetails): Promise<unknown> {
  const errorDetails = { ...details, status: response.status };
  if (!response.ok) {
    if (response.status === 401) throw new DrasiError('UNAUTHENTICATED', errorDetails);
    if (response.status === 403) throw new DrasiError('FORBIDDEN', errorDetails);
    if (response.status === 501) throw new DrasiError('UNSUPPORTED_CAPABILITY', errorDetails);
    throw new DrasiError(
      response.status >= 500 || response.status === 408 || response.status === 429
        ? 'SERVER_UNAVAILABLE' : 'INCOMPATIBLE_RESOURCE',
      errorDetails,
    );
  }
  try {
    return await response.json();
  } catch {
    throw new DrasiError('INVALID_PAYLOAD', errorDetails);
  }
}
