// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { createPortal } from 'react-dom';
import clsx from 'clsx';
import {
  useDrasiQuery,
  useDrasiQueryDefinition,
  useDrasiServerUiUrl,
} from '../react/DrasiContext';
import { useRowAnimation, AnimationDirection } from '../react/useRowAnimation';
import { CodeViewerDialog } from './CodeViewerDialog';
import { CodeIcon, ExpandIcon, CollapseIcon } from './icons';
import {
  ColumnDef,
  RowAction,
  SortConfig,
  UseDrasiQueryOptions,
} from '../types';

export type { ColumnDef, RowAction, SortConfig } from '../types';

/** Props for the {@link QueryTable} component. */
export interface QueryTableProps<T> {
  /** Drasi query id to subscribe to over the shared connection. */
  queryId: string;
  /** Column definitions. */
  columns: ColumnDef<T>[];
  /** Function to extract a unique key for each row. */
  rowKey: (row: T) => string;

  /**
   * Options that control how the query's result batches are folded into rows
   * (key extraction, normalization, sort/filter). See {@link UseDrasiQueryOptions}.
   */
  queryOptions?: UseDrasiQueryOptions<T>;

  // Optional props
  /** Card title. */
  title?: string;
  /** Container className. */
  className?: string;
  /** Table className. */
  tableClassName?: string;
  /** Header row className. */
  headerClassName?: string;
  /** Body row className (static or per-row function). */
  rowClassName?: string | ((row: T, index: number) => string);
  /** Additional height class. The package default is 400px. */
  height?: string;

  // Sorting
  /** Initial sort configuration. */
  defaultSort?: SortConfig;
  /** Callback when sort changes. */
  onSortChange?: (sort: SortConfig) => void;

  // Actions
  /** Row actions (edit, delete, etc.). */
  actions?: RowAction<T>[];
  /** CSS width for the actions column (for example, `3rem`). */
  actionsWidth?: string;
  /** Header actions slot (e.g., add button). */
  headerActions?: React.ReactNode;

  // Animation
  /** Field to track for row change animations. */
  animateOnChange?: keyof T;

  // Custom rendering
  /** Custom row renderer (receives a default render function). */
  renderRow?: (
    row: T,
    columns: ColumnDef<T>[],
    animation: AnimationDirection,
    defaultRender: () => React.ReactNode,
  ) => React.ReactNode;
  /** Message to show when the table is empty. */
  emptyMessage?: string;

  // Slots
  /** Content to render between the header and the table. */
  headerSlot?: React.ReactNode;

  // Code viewer
  /** Consumer code snippet to display in the code viewer dialog. */
  codeSnippet?: string;
}

/** Sort indicator icon component. */
const SortIndicator: React.FC<{ direction: 'asc' | 'desc' | null; active: boolean }> = ({
  direction,
  active,
}) => {
  if (!active) {
    return (
      <svg
        className="drasi-sort-indicator drasi-sort-indicator--inactive"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
        />
      </svg>
    );
  }

  if (direction === 'asc') {
    return (
      <svg
        className="drasi-sort-indicator drasi-sort-indicator--active"
        fill="currentColor"
        viewBox="0 0 20 20"
      >
        <path d="M10 5l5 7H5l5-7z" />
      </svg>
    );
  }

  return (
    <svg
      className="drasi-sort-indicator drasi-sort-indicator--active"
      fill="currentColor"
      viewBox="0 0 20 20"
    >
      <path d="M10 15l-5-7h10l-5 7z" />
    </svg>
  );
};

/** Loading spinner component. */
const LoadingSpinner: React.FC = () => (
  <div className="drasi-loading">
    <div className="drasi-spinner drasi-spinner--large" />
  </div>
);

/** Small inline loading spinner for actions. */
const ActionSpinner: React.FC = () => (
  <div className="drasi-spinner drasi-spinner--small" />
);

/** Format a query config object into a readable, YAML-like string. */
function formatQueryConfig(config: Record<string, any>): string {
  const lines: string[] = [];

  const addField = (label: string, value: any) => {
    if (value === undefined || value === null) return;
    if (typeof value === 'string') {
      lines.push(`${label}: ${value}`);
    } else if (typeof value === 'boolean' || typeof value === 'number') {
      lines.push(`${label}: ${value}`);
    } else {
      lines.push(`${label}: ${JSON.stringify(value)}`);
    }
  };

  addField('id', config.id);
  addField('queryLanguage', config.queryLanguage);
  addField('autoStart', config.autoStart);

  if (config.query) {
    const q = typeof config.query === 'string' ? config.query.trim() : JSON.stringify(config.query);
    lines.push('');
    lines.push('query: |');
    for (const line of q.split('\n')) {
      lines.push(`  ${line}`);
    }
  }

  if (config.sources && config.sources.length > 0) {
    lines.push('');
    lines.push('sources:');
    for (const src of config.sources) {
      const sourceId = typeof src.sourceId === 'string' ? src.sourceId : JSON.stringify(src.sourceId);
      lines.push(`  - sourceId: ${sourceId}`);
      if (src.pipeline?.length) {
        lines.push(`    pipeline: [${src.pipeline.join(', ')}]`);
      }
      if (src.nodes?.length) {
        lines.push(`    nodes: [${src.nodes.join(', ')}]`);
      }
      if (src.relations?.length) {
        lines.push(`    relations: [${src.relations.join(', ')}]`);
      }
    }
  }

  if (config.joins) {
    lines.push('');
    lines.push('joins:');
    const joins = Array.isArray(config.joins) ? config.joins : [config.joins];
    for (const join of joins) {
      if (join.id) {
        lines.push(`  - id: ${join.id}`);
        if (join.keys && Array.isArray(join.keys)) {
          lines.push('    keys:');
          for (const key of join.keys) {
            lines.push(`      - label: ${key.label}, property: ${key.property}`);
          }
        }
      } else {
        const joinStr = JSON.stringify(join, null, 2);
        for (const line of joinStr.split('\n')) {
          lines.push(`  ${line}`);
        }
      }
    }
  }

  addField('enableBootstrap', config.enableBootstrap);
  addField('bootstrapBufferSize', config.bootstrapBufferSize);
  if (config.middleware?.length) {
    lines.push(`middleware: [${config.middleware.join(', ')}]`);
  }
  addField('priorityQueueCapacity', config.priorityQueueCapacity);
  addField('dispatchBufferCapacity', config.dispatchBufferCapacity);
  addField('dispatchMode', config.dispatchMode);
  if (config.storageBackend) {
    lines.push('');
    lines.push(`storageBackend: ${JSON.stringify(config.storageBackend, null, 2)}`);
  }

  return lines.join('\n');
}

/**
 * QueryTable — a reusable, sortable table bound to a Drasi continuous query.
 *
 * Features:
 * - Subscribes to a query over the shared {@link DrasiProvider} connection and
 *   renders the live result set as a table.
 * - Sortable columns, optional row actions, value-change animations, expand to
 *   full screen, and a code viewer showing the query definition.
 * - Fully data-model agnostic: callers supply `columns`, `rowKey`, and optional
 *   `queryOptions` (key/transform/sort).
 *
 * @example
 * ```tsx
 * <QueryTable<Stock>
 *   queryId="watchlist-query"
 *   columns={[
 *     { key: 'symbol', label: 'Symbol' },
 *     { key: 'price', label: 'Price', format: formatCurrency, align: 'right' },
 *   ]}
 *   rowKey={(row) => row.symbol}
 *   defaultSort={{ column: 'symbol', direction: 'asc' }}
 *   animateOnChange="price"
 * />
 * ```
 */
export function QueryTable<T extends Record<string, any>>({
  queryId,
  columns,
  rowKey,
  queryOptions,
  title,
  className,
  tableClassName,
  headerClassName,
  rowClassName,
  height,
  defaultSort,
  onSortChange,
  actions,
  actionsWidth,
  headerActions,
  animateOnChange,
  renderRow,
  emptyMessage = 'No data available',
  headerSlot,
  codeSnippet,
}: QueryTableProps<T>): React.ReactElement {
  const effectiveQueryOptions = useMemo<UseDrasiQueryOptions<T>>(
    () => ({
      ...queryOptions,
      getKey: queryOptions?.getKey ?? ((row: any) => rowKey(row as T)),
    }),
    [queryOptions, rowKey],
  );
  const { data, loading, error } = useDrasiQuery<T>(
    queryId,
    effectiveQueryOptions,
  );
  const [sort, setSort] = useState<SortConfig | undefined>(defaultSort);
  const [showCodeViewer, setShowCodeViewer] = useState(false);
  const drasiUiUrl = useDrasiServerUiUrl();

  // Expand/collapse state
  const containerRef = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [expandRect, setExpandRect] = useState<DOMRect | null>(null);
  const [animating, setAnimating] = useState(false);
  const animationFramesRef = useRef<number[]>([]);
  const collapseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previousBodyOverflowRef = useRef<string | null>(null);

  const handleExpand = useCallback(() => {
    if (containerRef.current) {
      if (collapseTimerRef.current) {
        clearTimeout(collapseTimerRef.current);
        collapseTimerRef.current = null;
      }
      setExpandRect(containerRef.current.getBoundingClientRect());
      setExpanded(true);
      const firstFrame = requestAnimationFrame(() => {
        const secondFrame = requestAnimationFrame(() => setAnimating(true));
        animationFramesRef.current.push(secondFrame);
      });
      animationFramesRef.current.push(firstFrame);
      previousBodyOverflowRef.current = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
    }
  }, []);

  const handleCollapse = useCallback(() => {
    animationFramesRef.current.forEach(cancelAnimationFrame);
    animationFramesRef.current = [];
    setAnimating(false);
    if (collapseTimerRef.current) clearTimeout(collapseTimerRef.current);
    collapseTimerRef.current = setTimeout(() => {
      setExpanded(false);
      setExpandRect(null);
      document.body.style.overflow =
        previousBodyOverflowRef.current ?? '';
      previousBodyOverflowRef.current = null;
      collapseTimerRef.current = null;
    }, 350);
  }, []);

  useEffect(
    () => () => {
      animationFramesRef.current.forEach(cancelAnimationFrame);
      if (collapseTimerRef.current) {
        clearTimeout(collapseTimerRef.current);
      }
      if (previousBodyOverflowRef.current !== null) {
        document.body.style.overflow = previousBodyOverflowRef.current;
      }
    },
    [],
  );

  useEffect(() => {
    if (!error || !expanded) return;

    animationFramesRef.current.forEach(cancelAnimationFrame);
    animationFramesRef.current = [];
    if (collapseTimerRef.current) {
      clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
    setAnimating(false);
    setExpanded(false);
    setExpandRect(null);
    document.body.style.overflow = previousBodyOverflowRef.current ?? '';
    previousBodyOverflowRef.current = null;
  }, [error, expanded]);

  // Escape key to collapse
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') handleCollapse();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [expanded, handleCollapse]);

  // Fetch the full query config from the Drasi Server
  const {
    config: queryConfig,
    loading: configLoading,
    error: configError,
  } = useDrasiQueryDefinition(queryId);
  const displayConfig = useMemo(() => {
    if (configLoading) return 'Loading query definition...';
    if (configError) return `Unable to load query definition: ${configError}`;
    if (!queryConfig) return 'Query not found';
    return formatQueryConfig(queryConfig);
  }, [queryConfig, configLoading, configError]);

  // Animation hook
  const getAnimatedValue = useCallback(
    (row: T) => {
      if (!animateOnChange) return undefined;
      const value = row[animateOnChange];
      return typeof value === 'number' || typeof value === 'string'
        ? value
        : undefined;
    },
    [animateOnChange],
  );
  const { animations, updateData } = useRowAnimation<T>({
    rowKey,
    getValue: getAnimatedValue,
  });

  // Update animation tracking when data changes
  useEffect(() => {
    if (data && animateOnChange) {
      updateData(data);
    }
  }, [data, animateOnChange, updateData]);

  // Handle column header click for sorting
  const handleHeaderClick = useCallback(
    (column: ColumnDef<T>) => {
      if (column.sortable === false) return;

      const columnKey = String(column.key);

      setSort((prev) => {
        let newSort: SortConfig;
        if (prev?.column === columnKey) {
          newSort = {
            column: columnKey,
            direction: prev.direction === 'asc' ? 'desc' : 'asc',
          };
        } else {
          newSort = { column: columnKey, direction: 'asc' };
        }
        onSortChange?.(newSort);
        return newSort;
      });
    },
    [onSortChange],
  );

  // Sort data
  const sortedData = useMemo(() => {
    if (!data || !sort) return data;

    return [...data].sort((a, b) => {
      const aVal = a[sort.column as keyof T];
      const bVal = b[sort.column as keyof T];

      if (aVal == null && bVal == null) return 0;
      if (aVal == null) return sort.direction === 'asc' ? 1 : -1;
      if (bVal == null) return sort.direction === 'asc' ? -1 : 1;

      let comparison = 0;
      if (typeof aVal === 'number' && typeof bVal === 'number') {
        comparison = aVal - bVal;
      } else if (typeof aVal === 'string' && typeof bVal === 'string') {
        comparison = aVal.localeCompare(bVal);
      } else {
        comparison = String(aVal).localeCompare(String(bVal));
      }

      return sort.direction === 'asc' ? comparison : -comparison;
    });
  }, [data, sort]);

  // Get cell value
  const getCellValue = (row: T, column: ColumnDef<T>): any => {
    const key = column.key as keyof T;
    return row[key];
  };

  // Render cell content
  const renderCell = (row: T, column: ColumnDef<T>): React.ReactNode => {
    const value = getCellValue(row, column);
    if (column.format) {
      return column.format(value, row);
    }
    if (value == null) return '-';
    return String(value);
  };

  // Get cell className
  const getCellClassName = (row: T, column: ColumnDef<T>): string => {
    const value = getCellValue(row, column);
    if (typeof column.className === 'function') {
      return column.className(value, row);
    }
    return column.className || '';
  };

  // Get row className
  const getRowClassName = (row: T, index: number): string => {
    if (typeof rowClassName === 'function') {
      return rowClassName(row, index);
    }
    return rowClassName || '';
  };

  // Render default row
  const renderDefaultRow = (
    row: T,
    index: number,
    animation: AnimationDirection,
  ): React.ReactNode => {
    const key = rowKey(row);

    return (
      <tr
        key={key}
        className={clsx(
          'drasi-query-table__row',
          animation === 'up' && 'drasi-row--up',
          animation === 'down' && 'drasi-row--down',
          animation === 'change' && 'drasi-row--change',
          getRowClassName(row, index),
        )}
      >
        {columns.map((column) => (
          <td
            key={String(column.key)}
            className={clsx(
              'drasi-query-table__cell',
              column.align === 'right' && 'drasi-align--right',
              column.align === 'center' && 'drasi-align--center',
              getCellClassName(row, column),
            )}
          >
            {renderCell(row, column)}
          </td>
        ))}
        {actions && actions.length > 0 && (
          <td className="drasi-query-table__cell">
            <div className="drasi-query-table__actions">
              {actions.map((action, actionIndex) => {
                const isDisabled = action.disabled?.(row) ?? false;
                const isLoading = action.loading?.(row) ?? false;

                return (
                  <button
                    type="button"
                    key={actionIndex}
                    onClick={() => !isDisabled && !isLoading && action.onClick(row)}
                    disabled={isDisabled || isLoading}
                    className={clsx(
                      'drasi-action-button',
                      action.className,
                      !isDisabled &&
                        !isLoading &&
                        action.hoverClassName,
                      (isDisabled || isLoading) &&
                        'drasi-action-button--disabled',
                    )}
                    title={action.label}
                    aria-label={action.label}
                  >
                    {isLoading ? <ActionSpinner /> : action.icon}
                  </button>
                );
              })}
            </div>
          </td>
        )}
      </tr>
    );
  };

  // Compute expanded portal styles for the FLIP animation
  const expandedStyle = useMemo((): React.CSSProperties | undefined => {
    if (!expandRect) return undefined;
    if (animating) {
      const pad = 32;
      return {
        position: 'fixed',
        top: pad,
        left: pad,
        width: `calc(100vw - ${pad * 2}px)`,
        height: `calc(100vh - ${pad * 2}px)`,
        transition: 'all 0.35s cubic-bezier(0.4, 0, 0.2, 1)',
        zIndex: 60,
      };
    }
    return {
      position: 'fixed',
      top: expandRect.top,
      left: expandRect.left,
      width: expandRect.width,
      height: expandRect.height,
      transition: 'all 0.35s cubic-bezier(0.4, 0, 0.2, 1)',
      zIndex: 60,
    };
  }, [expandRect, animating]);

  // Loading state
  if (loading && !data) {
    return (
      <div
        className={clsx(
          'drasi-query-table drasi-query-table--state',
          height,
          className,
        )}
      >
        {title && <h2 className="drasi-query-table__state-title">{title}</h2>}
        <LoadingSpinner />
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div
        className={clsx(
          'drasi-query-table drasi-query-table--state',
          height,
          className,
        )}
      >
        {title && <h2 className="drasi-query-table__state-title">{title}</h2>}
        <div className="drasi-query-table__error">Error: {error}</div>
      </div>
    );
  }

  // Expand button shown in normal view
  const expandButton = (
    <button
      type="button"
      onClick={handleExpand}
      className="drasi-icon-button"
      title="Expand table"
      aria-label="Expand table"
    >
      <ExpandIcon className="drasi-icon drasi-icon--medium" />
    </button>
  );

  // Collapse button shown in expanded view
  const collapseButton = (
    <button
      type="button"
      onClick={handleCollapse}
      className="drasi-icon-button"
      title="Collapse table"
      aria-label="Collapse table"
    >
      <CollapseIcon className="drasi-icon drasi-icon--medium" />
    </button>
  );

  // Renders the full table card content (shared between normal and expanded views)
  const renderTableCard = (isExpanded: boolean, isAnimating: boolean) => (
    <>
      {/* Header */}
      {(title || headerActions || codeSnippet) && (
        <div className="drasi-query-table__header">
          <div className="drasi-query-table__header-main">
            {title && (
              <h2
                className={clsx(
                  'drasi-query-table__title',
                  isAnimating && 'drasi-query-table__title--expanded',
                )}
              >
                {title}
              </h2>
            )}
            {headerActions}
          </div>
          <div className="drasi-query-table__header-controls">
            {codeSnippet && (
              <button
                type="button"
                onClick={() => setShowCodeViewer(true)}
                className="drasi-icon-button"
                title="View code"
                aria-label="View code"
              >
                <CodeIcon className="drasi-icon drasi-icon--medium" />
              </button>
            )}
            {isExpanded ? collapseButton : expandButton}
          </div>
        </div>
      )}

      {/* Header slot (e.g., summary stats) */}
      {headerSlot && (
        <div
          className={clsx(
            'drasi-query-table__header-slot',
            isAnimating && 'drasi-expanded-text',
          )}
        >
          {headerSlot}
        </div>
      )}

      {/* Table */}
      <div
        className={clsx(
          'drasi-query-table__scroll',
          isAnimating && 'drasi-expanded-text',
          tableClassName,
        )}
      >
        <table className="drasi-query-table__table">
          <thead
            className={clsx(
              'drasi-query-table__thead',
              headerClassName,
            )}
          >
            <tr className="drasi-query-table__header-row">
              {columns.map((column) => {
                const isSortable = column.sortable !== false;
                const isActive = sort?.column === String(column.key);

                return (
                  <th
                    key={String(column.key)}
                    style={column.width ? { width: column.width } : undefined}
                    className={clsx(
                      'drasi-query-table__heading',
                      isAnimating &&
                        'drasi-query-table__heading--expanded',
                      column.align === 'right' && 'drasi-align--right',
                      column.align === 'center' && 'drasi-align--center',
                      column.align !== 'right' &&
                        column.align !== 'center' &&
                        'drasi-align--left',
                      column.headerClassName,
                      isSortable && 'drasi-query-table__heading--sortable',
                    )}
                    onClick={() => isSortable && handleHeaderClick(column)}
                    onKeyDown={(event) => {
                      if (
                        isSortable &&
                        (event.key === 'Enter' || event.key === ' ')
                      ) {
                        event.preventDefault();
                        handleHeaderClick(column);
                      }
                    }}
                    tabIndex={isSortable ? 0 : undefined}
                    aria-sort={
                      isActive
                        ? sort!.direction === 'asc'
                          ? 'ascending'
                          : 'descending'
                        : undefined
                    }
                  >
                    <span
                      className={clsx(
                        'drasi-query-table__heading-content',
                        column.align === 'right' &&
                          'drasi-query-table__heading-content--right',
                      )}
                    >
                      {column.label}
                      {isSortable && (
                        <SortIndicator
                          direction={isActive ? sort!.direction : null}
                          active={isActive}
                        />
                      )}
                    </span>
                  </th>
                );
              })}
              {actions && actions.length > 0 && (
                <th
                  className="drasi-query-table__actions-heading"
                  style={actionsWidth ? { width: actionsWidth } : undefined}
                />
              )}
            </tr>
          </thead>
          <tbody>
            {sortedData?.map((row, index) => {
              const key = rowKey(row);
              const animation = animations.get(key) ?? null;

              if (renderRow) {
                return renderRow(row, columns, animation, () =>
                  renderDefaultRow(row, index, animation),
                );
              }

              return renderDefaultRow(row, index, animation);
            })}
            {(!sortedData || sortedData.length === 0) && (
              <tr>
                <td
                  colSpan={columns.length + (actions?.length ? 1 : 0)}
                  className="drasi-query-table__empty"
                >
                  {emptyMessage}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </>
  );

  return (
    <>
      {/* Code Viewer Dialog - rendered at top level for proper z-index */}
      {codeSnippet && (
        <CodeViewerDialog
          isOpen={showCodeViewer}
          onClose={() => setShowCodeViewer(false)}
          title={title || queryId}
          reactCode={codeSnippet}
          cypherQuery={displayConfig}
          drasiUiUrl={drasiUiUrl}
        />
      )}

      {/* Normal in-place card */}
      <div
        ref={containerRef}
        className={clsx(
          'drasi-query-table',
          height,
          className,
          expanded && 'drasi-query-table--hidden',
        )}
      >
        {renderTableCard(false, false)}
      </div>

      {/* Expanded portal overlay */}
      {expanded &&
        createPortal(
          <>
            {/* Backdrop */}
            <div
              className="drasi-query-table__backdrop"
              style={{
                backgroundColor: animating ? 'rgba(0,0,0,0.7)' : 'rgba(0,0,0,0)',
                transition: 'background-color 0.35s ease',
              }}
              onClick={handleCollapse}
            />
            {/* Expanded card */}
            <div
              className="drasi-query-table drasi-query-table--expanded"
              style={expandedStyle}
            >
              {renderTableCard(true, animating)}
            </div>
          </>,
          document.body,
        )}
    </>
  );
}
