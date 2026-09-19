// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiError, type DrasiErrorDetails } from './errors';
import type { QueryResult, ResultRow, RowKey } from './types';

function keyFor(row: ResultRow, getKey: RowKey, details: DrasiErrorDetails): string {
  try {
    const key = getKey(row);
    if (typeof key === 'string' && key.length > 0) return key;
  } catch (error) {
    if (error instanceof DrasiError) throw error;
  }
  throw new DrasiError('INVALID_ROW_KEY', details);
}

/**
 * Transactional raw-row accumulation. Duplicate upserts are idempotent by key;
 * equal-valued rows with different domain keys remain independent. Deletes
 * never run a transform. An update removes its before key before inserting after.
 */
export function accumulateResult(
  rows: readonly ResultRow[],
  result: QueryResult,
  getKey: RowKey,
  details: DrasiErrorDetails = {},
): ResultRow[] {
  const next = new Map<string, ResultRow>();
  for (const row of result.kind === 'snapshot' ? result.rows : rows) {
    next.set(keyFor(row, getKey, details), row);
  }
  if (result.kind === 'delta') {
    for (const change of result.changes) {
      if (change.kind === 'delete') {
        next.delete(keyFor(change.before, getKey, details));
      } else {
        const afterKey = keyFor(change.after, getKey, details);
        if (change.kind === 'update') {
          const beforeKey = keyFor(change.before, getKey, details);
          if (beforeKey !== afterKey) next.delete(beforeKey);
        }
        next.set(afterKey, change.after);
      }
    }
  }
  return [...next.values()];
}
