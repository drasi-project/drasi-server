// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiError } from '../client/errors';
import type { QueryConfig, QueryErrorScope, QueryStatus, ResultRow, RowKey } from '../client/types';

/** Identity precedes projection. A generic alone never validates a wire row. */
export interface UseDrasiQueryOptions<T extends object = ResultRow> {
  /**
   * Required stable RAW-row identity, including sparse deletes and both sides
   * of updates. Return a nonempty string; invalid keys are errors, not skips.
   */
  getKey: RowKey;
  /**
   * Validate/project current raw rows, never deletes. Return null to hide this
   * identity; its raw state is retained so later updates/option changes can
   * reveal it again. Use row => row for untransformed object rows.
   */
  transform: (row: Readonly<ResultRow>) => T | null;
  /** Pure derived sorting/filtering. Does not remove rows from the raw result set. */
  postProcess?: (rows: T[]) => T[];
}

/** Best-effort data and explicit recovery state, not an atomic server snapshot. */
export interface UseDrasiQueryResult<T extends object = ResultRow> {
  data: T[] | null;
  status: QueryStatus;
  stale: boolean;
  loading: boolean;
  error: DrasiError | null;
  errorScope: QueryErrorScope | null;
  lastUpdate: Date | null;
  /** Query-local REST refresh; use useDrasiClient().retry for a shared connection failure. */
  retry: () => void;
}

export interface UseDrasiQueryDefinitionResult {
  config: QueryConfig | null;
  loading: boolean;
  error: DrasiError | null;
}
