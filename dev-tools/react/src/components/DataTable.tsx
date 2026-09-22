// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React, { useCallback, useMemo, useState } from 'react';
import clsx from 'clsx';
import { useTableSort, type UseTableSortOptions, type SortConfig } from '../react/useTableSort';
import { useRowAnimation, type AnimationDirection } from '../react/useRowAnimation';
import { useReducedMotion } from '../react/useReducedMotion';
import type { ColumnDef, RowAction } from './types';
import { tableHeight, type TableHeight } from './sizing';

const textOrder = new Intl.Collator('en-US', { sensitivity: 'variant', numeric: false });

/** @internal Numbers, then fixed-English text representations, then nullish values. */
export function compareTableValues(a: unknown, b: unknown): number {
  if (a == null) return b == null ? 0 : 1;
  if (b == null) return -1;
  if (typeof a === 'number') {
    if (typeof b !== 'number') return -1;
    if (Number.isNaN(a)) return Number.isNaN(b) ? 0 : 1;
    if (Number.isNaN(b)) return -1;
    // Subtraction would produce NaN for equal infinities.
    return a < b ? -1 : a > b ? 1 : 0;
  }
  if (typeof b === 'number') return 1;
  return textOrder.compare(String(a), String(b));
}

/** Presentation state only: no transport, provider or query identity is required. */
export interface DataTableState<E extends Error = Error> {
  loading?: boolean;
  error?: E | null;
  stale?: boolean;
  /** The owner chooses the recovery scope. Omit to render no retry button. */
  retry?: () => void;
  /** Defaults to "Retry". */
  retryLabel?: string;
  /** Defaults to "Showing last known data." */
  staleMessage?: string;
}

export interface DataTableRenderContext<T extends object, E extends Error = Error> {
  /** Sorted view of the supplied rows; null means no baseline is available. */
  rows: readonly T[] | null;
  state: DataTableState<E>;
  sort: SortConfig | null;
  setSort: (sort: SortConfig | null) => void;
}

export interface DataTableErrorContext<T extends object, E extends Error = Error>
  extends DataTableRenderContext<T, E> {
  error: E;
}

export interface DataTableHeaderContext<T extends object, E extends Error = Error>
  extends DataTableRenderContext<T, E> {
  defaultRender: () => React.ReactNode;
}

export interface DataTableProps<T extends object = Record<string, unknown>, E extends Error = Error>
  extends UseTableSortOptions {
  /** Null means no accepted baseline; [] is an accepted empty view. Never mutated. */
  rows: readonly T[] | null;
  columns: readonly ColumnDef<T>[];
  /** Stable identity for React rows and animation, not an array position. */
  rowKey: (row: T) => string;
  state?: DataTableState<E>;
  title?: string;
  /** Names the table and its keyboard-scrollable viewport; defaults to title. Supply when no visible title exists. */
  ariaLabel?: string;
  className?: string;
  style?: React.CSSProperties;
  /** The card element, for app-owned layout/overlay composition. */
  containerRef?: React.Ref<HTMLDivElement>;
  /** Scroll-container class, retained for compatibility. */
  tableClassName?: string;
  /** Thead class. ColumnDef.headerClassName styles each heading. */
  headerClassName?: string;
  titleClassName?: string;
  rowClassName?: string | ((row: T, index: number) => string);
  /** Pixels, an explicit CSS length, or var(--token). Overrides style.height; default token is 400px. */
  height?: TableHeight;
  actions?: readonly RowAction<T>[];
  actionsWidth?: string;
  /** Alongside the title. */
  headerActions?: React.ReactNode;
  /** Right-hand header controls; no built-in tutorial or fullscreen actions. */
  headerControls?: React.ReactNode;
  /** Below the title/header, above the table. */
  headerSlot?: React.ReactNode;
  headerSlotClassName?: string;
  renderHeader?: (context: DataTableHeaderContext<T, E>) => React.ReactNode;
  animateOnChange?: keyof T;
  /** Optional shared animation state; takes precedence over animateOnChange. */
  rowAnimations?: ReadonlyMap<string, AnimationDirection>;
  /** Changed tokens restart controlled decoration without changing row identity. Share useRowAnimation.revisions. */
  rowAnimationRevisions?: ReadonlyMap<string, number>;
  renderRow?: (
    row: T,
    columns: readonly ColumnDef<T>[],
    animation: AnimationDirection,
    defaultRender: () => React.ReactNode,
  ) => React.ReactNode;
  /** Defaults to "No data available"; renderEmpty takes precedence. */
  emptyMessage?: string;
  /** Slots may return null to intentionally suppress their default. */
  renderLoading?: (context: DataTableRenderContext<T, E>) => React.ReactNode;
  renderEmpty?: (context: DataTableRenderContext<T, E>) => React.ReactNode;
  renderError?: (context: DataTableErrorContext<T, E>) => React.ReactNode;
  renderStale?: (context: DataTableRenderContext<T, E>) => React.ReactNode;
}

const SortIndicator = ({ direction }: { direction: 'asc' | 'desc' | null }) => {
  if (!direction) {
    return (
      <svg aria-hidden="true" focusable="false" className="drasi-sort-indicator drasi-sort-indicator--inactive" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
      </svg>
    );
  }
  return (
    <svg aria-hidden="true" focusable="false" className="drasi-sort-indicator drasi-sort-indicator--active" fill="currentColor" viewBox="0 0 20 20">
      <path d={direction === 'asc' ? 'M10 5l5 7H5l5-7z' : 'M10 15l-5-7h10l-5 7z'} />
    </svg>
  );
};

function AnimatedRow({ animation, revision, className, children }: {
  animation: AnimationDirection;
  revision?: number;
  className: string;
  children: React.ReactNode;
}) {
  const [cycle, setCycle] = useState({ animation, revision, alternate: false });
  // Compare rendered tokens rather than their parity: batched updates may skip revisions.
  if (cycle.animation !== animation || !Object.is(cycle.revision, revision)) {
    setCycle({ animation, revision, alternate: animation !== null && cycle.animation !== null && !cycle.alternate });
  }
  return <tr className={clsx(className, animation && cycle.alternate && 'drasi-row--repeat')}>{children}</tr>;
}

/**
 * Provider-free presentation with local or controlled sorting and optional
 * animation. Error > stale > loading notices; supplied rows are always retained.
 * Without rows, error/loading use a state card. Empty content fills one table
 * cell. No reads, subscriptions, tutorial modules or CSS imports occur here.
 */
export function DataTable<T extends object = Record<string, unknown>, E extends Error = Error>({
  rows,
  columns,
  rowKey,
  state = {},
  title,
  ariaLabel,
  className,
  style,
  containerRef,
  tableClassName,
  headerClassName,
  titleClassName,
  rowClassName,
  height,
  sort: controlledSort,
  defaultSort,
  onSortChange,
  actions,
  actionsWidth,
  headerActions,
  headerControls,
  headerSlot,
  headerSlotClassName,
  renderHeader,
  animateOnChange,
  rowAnimations,
  rowAnimationRevisions,
  renderRow,
  emptyMessage = 'No data available',
  renderLoading,
  renderEmpty,
  renderError,
  renderStale,
}: DataTableProps<T, E>): React.ReactElement {
  const reducedMotion = useReducedMotion();
  const cardStyle = height === undefined ? style : { ...style, height: tableHeight(height) };
  const { sort, setSort, toggleSort } = useTableSort({ sort: controlledSort, defaultSort, onSortChange });
  const sortedRows = useMemo(() => {
    if (rows === null || !sort) return rows;
    return [...rows].sort((a, b) => {
      const aVal: unknown = Reflect.get(a, sort.column);
      const bVal: unknown = Reflect.get(b, sort.column);
      const comparison = compareTableValues(aVal, bVal);
      return sort.direction === 'asc' ? comparison : -comparison;
    });
  }, [rows, sort]);
  const getValue = useCallback((row: T) => {
    const value = animateOnChange === undefined ? undefined : row[animateOnChange];
    return typeof value === 'number' || typeof value === 'string' ? value : undefined;
  }, [animateOnChange]);
  const tracked = useRowAnimation({
    rowKey, getValue,
    data: rowAnimations === undefined && animateOnChange !== undefined ? rows : undefined,
  });
  const animations = rowAnimations ?? tracked.animations;
  const revisions = rowAnimations === undefined ? tracked.revisions : rowAnimationRevisions;
  const context: DataTableRenderContext<T, E> = { rows: sortedRows, state, sort, setSort };
  let notice: React.ReactNode = null;
  if (state.error) {
    notice = renderError ? renderError({ ...context, error: state.error }) : (
      <div className="drasi-query-table__error" role="alert">
        <span>Error: {state.error.message}</span>
        {state.stale && <span> Showing last known data.</span>}{' '}
        {state.retry && <button type="button" onClick={state.retry}>{state.retryLabel ?? 'Retry'}</button>}
      </div>
    );
  } else if (state.stale) {
    notice = renderStale ? renderStale(context) : (
      <div role="status">{state.staleMessage ?? 'Showing last known data.'}</div>
    );
  } else if (state.loading) {
    notice = renderLoading ? renderLoading(context) : (
      <div className="drasi-loading" role="status" aria-label="Loading">
        <div className="drasi-spinner drasi-spinner--large" />
      </div>
    );
  }

  if (rows === null && (state.error || state.loading)) {
    return (
      <div ref={containerRef} style={cardStyle} className={clsx('drasi-query-table drasi-query-table--state', className)}>
        {title && <h2 className="drasi-query-table__state-title">{title}</h2>}
        {notice}
      </div>
    );
  }

  const defaultHeader = () => (title || headerActions || headerControls) ? (
    <div className="drasi-query-table__header">
      <div className="drasi-query-table__header-main">
        {title && <h2 className={clsx('drasi-query-table__title', titleClassName)}>{title}</h2>}
        {headerActions}
      </div>
      <div className="drasi-query-table__header-controls">{headerControls}</div>
    </div>
  ) : null;

  const defaultRow = (row: T, index: number, animation: AnimationDirection, revision: number | undefined) => (
    <AnimatedRow animation={animation} revision={revision} className={clsx(
      'drasi-query-table__row',
      animation === 'up' && 'drasi-row--up',
      animation === 'down' && 'drasi-row--down',
      animation === 'change' && 'drasi-row--change',
      typeof rowClassName === 'function' ? rowClassName(row, index) : rowClassName,
    )}>
      {columns.map(column => {
        const value: unknown = Reflect.get(row, column.key);
        return (
          <td key={String(column.key)} className={clsx(
            'drasi-query-table__cell',
            column.align === 'left' && 'drasi-align--left',
            column.align === 'right' && 'drasi-align--right',
            column.align === 'center' && 'drasi-align--center',
            typeof column.className === 'function' ? column.className(value, row) : column.className,
          )}>
            {column.format ? column.format(value, row) : value == null ? '-' : String(value)}
          </td>
        );
      })}
      {!!actions?.length && (
        <td className="drasi-query-table__cell">
          <div className="drasi-query-table__actions">
            {actions.map((action, index) => {
              const disabled = action.disabled?.(row) ?? false;
              const loading = action.loading?.(row) ?? false;
              return (
                <button
                  type="button"
                  key={index}
                  onClick={() => !disabled && !loading && action.onClick(row)}
                  disabled={disabled || loading}
                  className={clsx(
                    'drasi-action-button', action.className,
                    !disabled && !loading && action.hoverClassName,
                    (disabled || loading) && 'drasi-action-button--disabled',
                  )}
                  title={action.label}
                  aria-label={action.label}
                  aria-busy={loading || undefined}
                >
                  {loading ? <div className="drasi-spinner drasi-spinner--small" /> : action.icon}
                </button>
              );
            })}
          </div>
        </td>
      )}
    </AnimatedRow>
  );

  return (
    <div ref={containerRef} style={cardStyle} className={clsx('drasi-query-table', className)}>
      {notice}
      {renderHeader ? renderHeader({ ...context, defaultRender: defaultHeader }) : defaultHeader()}
      {headerSlot && <div className={clsx('drasi-query-table__header-slot', headerSlotClassName)}>{headerSlot}</div>}
      <div
        className={clsx('drasi-query-table__scroll', tableClassName)}
        role="region"
        aria-label={`${ariaLabel ?? title ?? 'Data'} table viewport`}
        tabIndex={0}
      >
        <table className="drasi-query-table__table" aria-label={ariaLabel ?? title}>
          <thead className={clsx('drasi-query-table__thead', headerClassName)}>
            <tr className="drasi-query-table__header-row">
              {columns.map(column => {
                const sortable = column.sortable !== false;
                const direction = sort?.column === String(column.key) ? sort.direction : null;
                return (
                  <th
                    key={String(column.key)}
                    scope="col"
                    style={column.width ? { width: column.width } : undefined}
                    className={clsx(
                      'drasi-query-table__heading',
                      column.align === 'right' && 'drasi-align--right',
                      column.align === 'center' && 'drasi-align--center',
                      column.align !== 'right' && column.align !== 'center' && 'drasi-align--left',
                      column.headerClassName,
                      sortable && 'drasi-query-table__heading--sortable',
                    )}
                    aria-sort={direction === null ? undefined : direction === 'asc' ? 'ascending' : 'descending'}
                  >
                    {sortable ? (
                      <button type="button" className="drasi-query-table__sort-button" onClick={() => toggleSort(String(column.key))}>
                        <span className={clsx(
                          'drasi-query-table__heading-content',
                          column.align === 'right' && 'drasi-query-table__heading-content--right',
                        )}>
                          {column.label}
                          <SortIndicator direction={direction} />
                        </span>
                      </button>
                    ) : <span className={clsx(
                      'drasi-query-table__heading-content',
                      column.align === 'right' && 'drasi-query-table__heading-content--right',
                    )}>{column.label}</span>}
                  </th>
                );
              })}
              {!!actions?.length && <th scope="col" className="drasi-query-table__actions-heading" style={actionsWidth ? { width: actionsWidth } : undefined}>
                <span className="drasi-visually-hidden">Actions</span>
              </th>}
            </tr>
          </thead>
          <tbody>
            {sortedRows?.map((row, index) => {
              const key = rowKey(row);
              const animation = reducedMotion ? null : animations.get(key) ?? null;
              const revision = revisions?.get(key);
              return (
                <React.Fragment key={key}>
                  {renderRow ? renderRow(row, columns, animation, () => defaultRow(row, index, animation, revision))
                    : defaultRow(row, index, animation, revision)}
                </React.Fragment>
              );
            })}
            {!sortedRows?.length && (
              <tr>
                <td colSpan={columns.length + (actions?.length ? 1 : 0)} className="drasi-query-table__empty">
                  {renderEmpty ? renderEmpty(context) : emptyMessage}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
