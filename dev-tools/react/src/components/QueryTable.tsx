// Copyright 2025 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React from 'react';
import { useDrasiQuery, useDrasiClient } from '../react/DrasiContext';
import type { UseDrasiQueryOptions, UseDrasiQueryResult } from '../react/types';
import type { DrasiError } from '../client/errors';
import type { ResultRow } from '../client/types';
import {
  DataTable,
  type DataTableProps,
  type DataTableState,
  type DataTableRenderContext,
} from './DataTable';

export type { ColumnDef, RowAction, SortConfig } from './types';

export interface QueryTableRenderContext<T extends object> extends DataTableRenderContext<T, DrasiError> {
  /** Full P4 state, including status, errorScope, lastUpdate and query-local retry. */
  query: UseDrasiQueryResult<T>;
  retryConnection: () => void;
}

export interface QueryTableErrorContext<T extends object> extends QueryTableRenderContext<T> {
  error: DrasiError;
}

export interface QueryTableProps<T extends object = ResultRow> extends Omit<
  DataTableProps<T, DrasiError>,
  'rows' | 'state' | 'renderLoading' | 'renderEmpty' | 'renderError' | 'renderStale'
> {
  queryId: string;
  /** Required raw identity and validating transform, separate from transformed rowKey. */
  queryOptions: UseDrasiQueryOptions<T>;
  renderLoading?: (context: QueryTableRenderContext<T>) => React.ReactNode;
  renderEmpty?: (context: QueryTableRenderContext<T>) => React.ReactNode;
  renderError?: (context: QueryTableErrorContext<T>) => React.ReactNode;
  renderStale?: (context: QueryTableRenderContext<T>) => React.ReactNode;
}

/** Map query state to presentation, choosing local versus shared retry explicitly. */
export function queryTableState<T extends object>(
  query: UseDrasiQueryResult<T>,
  retryConnection: () => void,
): DataTableState<DrasiError> {
  const shared = query.errorScope === 'connection';
  return {
    loading: query.loading,
    error: query.error,
    stale: query.stale,
    retry: shared ? retryConnection : query.retry,
    retryLabel: shared ? 'Retry connection' : 'Retry query',
    staleMessage: `${query.status === 'reconnecting' ? 'Reconnecting' : 'Resynchronizing'}. Showing last known data.`,
  };
}

/** Convenient live table. Tutorial/inspection/fullscreen composition belongs to the app. */
export function QueryTable<T extends object = ResultRow>({
  queryId, queryOptions, renderLoading, renderEmpty, renderError, renderStale, ...presentation
}: QueryTableProps<T>): React.ReactElement {
  const query = useDrasiQuery(queryId, queryOptions);
  const { retry: retryConnection } = useDrasiClient();
  return (
    <DataTable<T, DrasiError>
      {...presentation}
      rows={query.data}
      state={queryTableState(query, retryConnection)}
      renderLoading={renderLoading && (context => renderLoading({ ...context, query, retryConnection }))}
      renderEmpty={renderEmpty && (context => renderEmpty({ ...context, query, retryConnection }))}
      renderError={renderError && (context => renderError({ ...context, query, retryConnection }))}
      renderStale={renderStale && (context => renderStale({ ...context, query, retryConnection }))}
    />
  );
}
