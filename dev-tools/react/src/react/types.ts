// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiError } from '../client/errors';
import type { QueryConfig, ResultRow } from '../client/types';

/** Existing accumulation options. A generic alone does not validate an application's row schema. */
export interface UseDrasiQueryOptions<T = ResultRow> {
  /**
   * Applied AFTER transformation, including to legacy delete rows.
   * Returning null skips a row. Legacy default: id, then symbol, then serialized row.
   */
  getKey?: (row: T) => string | null;
  /** Narrow/validate the raw object's unknown fields here to establish your application's T. */
  transform?: (row: ResultRow) => T;
  postProcess?: (rows: T[]) => T[];
}

/** Current hook result. Transport connectivity alone is not snapshot synchronization. */
export interface UseDrasiQueryResult<T = ResultRow> {
  data: T[] | null;
  loading: boolean;
  error: DrasiError | null;
  lastUpdate: Date | null;
}

export interface UseDrasiQueryDefinitionResult {
  config: QueryConfig | null;
  loading: boolean;
  error: DrasiError | null;
}
