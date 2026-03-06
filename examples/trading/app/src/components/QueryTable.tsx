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
import { useQuery, useQueryDefinition, useDrasiUiUrl } from '@/hooks/useDrasi';
import { useRowAnimation, AnimationDirection } from '@/hooks/useRowAnimation';
import { CodeViewerDialog, CodeIcon, ExpandIcon, CollapseIcon } from './shared';
import clsx from 'clsx';

/**
 * Column definition for QueryTable
 */
export interface ColumnDef<T> {
  /** Property key on the data object, or a custom string for computed columns */
  key: keyof T | string;
  /** Column header label */
  label: string;
  /** Custom formatter/renderer for cell content */
  format?: (value: any, row: T) => React.ReactNode;
  /** Whether this column is sortable (default: true) */
  sortable?: boolean;
  /** Text alignment (default: 'left') */
  align?: 'left' | 'center' | 'right';
  /** Additional CSS classes for cells */
  className?: string | ((value: any, row: T) => string);
  /** Additional CSS classes for header */
  headerClassName?: string;
  /** Width hint (e.g., 'w-20', 'w-32') */
  width?: string;
}

/**
 * Row action definition (edit, delete, etc.)
 */
export interface RowAction<T> {
  /** Icon element to display */
  icon: React.ReactNode;
  /** Accessibility label */
  label: string;
  /** Click handler */
  onClick: (row: T) => void;
  /** Additional CSS classes */
  className?: string;
  /** Hover CSS classes */
  hoverClassName?: string;
  /** Whether action is disabled for this row */
  disabled?: (row: T) => boolean;
  /** Whether action is loading for this row */
  loading?: (row: T) => boolean;
}

/**
 * Sort configuration
 */
export interface SortConfig {
  column: string;
  direction: 'asc' | 'desc';
}

/**
 * Props for QueryTable component
 */
export interface QueryTableProps<T> {
  /** Drasi query ID to subscribe to */
  queryId: string;
  /** Column definitions */
  columns: ColumnDef<T>[];
  /** Function to extract unique key for each row */
  rowKey: (row: T) => string;
  
  // Optional props
  /** Card title */
  title?: string;
  /** Container className */
  className?: string;
  /** Table className */
  tableClassName?: string;
  /** Header row className */
  headerClassName?: string;
  /** Body row className (static or per-row function) */
  rowClassName?: string | ((row: T, index: number) => string);
  /** Fixed height for the card (default: 'h-[400px]') */
  height?: string;
  
  // Sorting
  /** Initial sort configuration */
  defaultSort?: SortConfig;
  /** Callback when sort changes */
  onSortChange?: (sort: SortConfig) => void;
  
  // Actions
  /** Row actions (edit, delete, etc.) */
  actions?: RowAction<T>[];
  /** Actions column width */
  actionsWidth?: string;
  /** Header actions slot (e.g., add button) */
  headerActions?: React.ReactNode;
  
  // Animation
  /** Field to track for row change animations */
  animateOnChange?: keyof T;
  
  // Custom rendering
  /** Custom row renderer (receives default render function) */
  renderRow?: (
    row: T,
    columns: ColumnDef<T>[],
    animation: AnimationDirection,
    defaultRender: () => React.ReactNode
  ) => React.ReactNode;
  /** Message to show when table is empty */
  emptyMessage?: string;
  
  // Slots
  /** Content to render between header and table */
  headerSlot?: React.ReactNode;
  
  // Code viewer (for presentations)
  /** React code snippet to display in code viewer dialog */
  codeSnippet?: string;
}

/**
 * Sort indicator icon component
 */
const SortIndicator: React.FC<{ direction: 'asc' | 'desc' | null; active: boolean }> = ({ direction, active }) => {
  if (!active) {
    return (
      <svg className="w-3 h-3 ml-1 opacity-30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
      </svg>
    );
  }
  
  if (direction === 'asc') {
    return (
      <svg className="w-3 h-3 ml-1 text-trading-blue" fill="currentColor" viewBox="0 0 20 20">
        <path d="M10 5l5 7H5l5-7z"/>
      </svg>
    );
  }
  
  return (
    <svg className="w-3 h-3 ml-1 text-trading-blue" fill="currentColor" viewBox="0 0 20 20">
      <path d="M10 15l-5-7h10l-5 7z"/>
    </svg>
  );
};

/**
 * Loading spinner component
 */
const LoadingSpinner: React.FC = () => (
  <div className="flex items-center justify-center flex-1">
    <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-trading-blue"></div>
  </div>
);

/**
 * Small inline loading spinner for actions
 */
const ActionSpinner: React.FC = () => (
  <div className="w-4 h-4 animate-spin rounded-full border-2 border-gray-500 border-t-transparent"></div>
);

/**
 * Format a query config object into a readable, YAML-like string for display.
 */
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

  // Query — display as a nicely indented block
  if (config.query) {
    const q = typeof config.query === 'string' ? config.query.trim() : JSON.stringify(config.query);
    lines.push('');
    lines.push('query: |');
    for (const line of q.split('\n')) {
      lines.push(`  ${line}`);
    }
  }

  // Sources
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

  // Joins
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
        // Fallback for unknown join shapes
        const joinStr = JSON.stringify(join, null, 2);
        for (const line of joinStr.split('\n')) {
          lines.push(`  ${line}`);
        }
      }
    }
  }

  // Optional fields
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
 * QueryTable - A reusable, sortable table component for Drasi query results.
 * 
 * Features:
 * - Subscribes to a Drasi query and displays results as a table
 * - Sortable columns with click-to-toggle asc/desc
 * - Optional row actions (edit, delete, etc.)
 * - Row animations on value changes
 * - Customizable styling and rendering
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
  title,
  className,
  tableClassName,
  headerClassName,
  rowClassName,
  height = 'h-[400px]',
  defaultSort,
  onSortChange,
  actions,
  actionsWidth = 'w-20',
  headerActions,
  animateOnChange,
  renderRow,
  emptyMessage = 'No data available',
  headerSlot,
  codeSnippet,
}: QueryTableProps<T>): React.ReactElement {
  const { data, loading, error } = useQuery<T>(queryId);
  const [sort, setSort] = useState<SortConfig | undefined>(defaultSort);
  const [showCodeViewer, setShowCodeViewer] = useState(false);
  const drasiUiUrl = useDrasiUiUrl();

  // Expand/collapse state
  const containerRef = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [expandRect, setExpandRect] = useState<DOMRect | null>(null);
  const [animating, setAnimating] = useState(false);

  const handleExpand = useCallback(() => {
    if (containerRef.current) {
      setExpandRect(containerRef.current.getBoundingClientRect());
      setExpanded(true);
      // Start at original rect, then animate to fullscreen on next frame
      requestAnimationFrame(() => {
        requestAnimationFrame(() => setAnimating(true));
      });
      document.body.style.overflow = 'hidden';
    }
  }, []);

  const handleCollapse = useCallback(() => {
    setAnimating(false);
    // Wait for the transition to finish before unmounting
    setTimeout(() => {
      setExpanded(false);
      setExpandRect(null);
      document.body.style.overflow = '';
    }, 350);
  }, []);

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
  const { config: queryConfig, loading: configLoading } = useQueryDefinition(queryId);
  const displayConfig = useMemo(() => {
    if (configLoading) return 'Loading query definition...';
    if (!queryConfig) return 'Query not found';
    return formatQueryConfig(queryConfig);
  }, [queryConfig, configLoading]);

  // Animation hook
  const { animations, updateData } = useRowAnimation<T>({
    rowKey,
    getValue: animateOnChange 
      ? (row) => {
          const val = row[animateOnChange];
          // Support both numeric and string values for animation
          if (typeof val === 'number' || typeof val === 'string') {
            return val;
          }
          return undefined;
        }
      : () => undefined,
  });

  // Update animation tracking when data changes
  useEffect(() => {
    if (data && animateOnChange) {
      updateData(data);
    }
  }, [data, animateOnChange, updateData]);

  // Handle column header click for sorting
  const handleHeaderClick = useCallback((column: ColumnDef<T>) => {
    if (column.sortable === false) return;
    
    const columnKey = String(column.key);
    
    setSort((prev) => {
      let newSort: SortConfig;
      
      if (prev?.column === columnKey) {
        // Toggle direction
        newSort = {
          column: columnKey,
          direction: prev.direction === 'asc' ? 'desc' : 'asc',
        };
      } else {
        // New column, default to ascending
        newSort = { column: columnKey, direction: 'asc' };
      }
      
      onSortChange?.(newSort);
      return newSort;
    });
  }, [onSortChange]);

  // Sort data
  const sortedData = useMemo(() => {
    if (!data || !sort) return data;
    
    return [...data].sort((a, b) => {
      const aVal = a[sort.column as keyof T];
      const bVal = b[sort.column as keyof T];
      
      // Handle null/undefined
      if (aVal == null && bVal == null) return 0;
      if (aVal == null) return sort.direction === 'asc' ? 1 : -1;
      if (bVal == null) return sort.direction === 'asc' ? -1 : 1;
      
      // Compare values
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
  const renderDefaultRow = (row: T, index: number, animation: AnimationDirection): React.ReactNode => {
    const key = rowKey(row);
    
    return (
      <tr
        key={key}
        className={clsx(
          "border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors",
          animation === 'up' && 'price-up',
          animation === 'down' && 'price-down',
          animation === 'change' && 'status-change',
          getRowClassName(row, index)
        )}
      >
        {columns.map((column) => (
          <td
            key={String(column.key)}
            className={clsx(
              "py-3 px-2",
              column.align === 'right' && 'text-right',
              column.align === 'center' && 'text-center',
              getCellClassName(row, column)
            )}
          >
            {renderCell(row, column)}
          </td>
        ))}
        {actions && actions.length > 0 && (
          <td className="py-3 px-2">
            <div className="flex gap-1 justify-end">
              {actions.map((action, actionIndex) => {
                const isDisabled = action.disabled?.(row) ?? false;
                const isLoading = action.loading?.(row) ?? false;
                
                return (
                  <button
                    key={actionIndex}
                    onClick={() => !isDisabled && !isLoading && action.onClick(row)}
                    disabled={isDisabled || isLoading}
                    className={clsx(
                      "p-1 rounded transition-colors",
                      action.className || "text-gray-500",
                      !isDisabled && !isLoading && (action.hoverClassName || "hover:bg-trading-border/50 hover:text-trading-blue"),
                      (isDisabled || isLoading) && "opacity-50 cursor-not-allowed"
                    )}
                    title={action.label}
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

  // Compute expanded portal styles for FLIP animation
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
      <div className={clsx("bg-trading-card rounded-lg p-6 border border-trading-border flex flex-col", height, className)}>
        {title && <h2 className="text-xl font-bold mb-4">{title}</h2>}
        <LoadingSpinner />
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className={clsx("bg-trading-card rounded-lg p-6 border border-trading-border", height, className)}>
        {title && <h2 className="text-xl font-bold mb-4">{title}</h2>}
        <div className="text-trading-red">Error: {error}</div>
      </div>
    );
  }

  // Expand button shown in normal view
  const expandButton = (
    <button
      onClick={handleExpand}
      className="p-1.5 rounded hover:bg-trading-border/50 transition-colors text-gray-500 hover:text-trading-blue"
      title="Expand table"
    >
      <ExpandIcon className="w-5 h-5" />
    </button>
  );

  // Collapse button shown in expanded view
  const collapseButton = (
    <button
      onClick={handleCollapse}
      className="p-1.5 rounded hover:bg-trading-border/50 transition-colors text-gray-500 hover:text-trading-blue"
      title="Collapse table"
    >
      <CollapseIcon className="w-5 h-5" />
    </button>
  );

  // Renders the full table card content (shared between normal and expanded views)
  const renderTableCard = (isExpanded: boolean, isAnimating: boolean) => (
    <>
      {/* Header */}
      {(title || headerActions || codeSnippet) && (
        <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
          <div className="flex items-center gap-3">
            {title && <h2 className={clsx("font-bold transition-all duration-300", isAnimating ? "text-5xl" : "text-xl")}>{title}</h2>}
            {headerActions}
          </div>
          <div className="flex items-center gap-1">
            {codeSnippet && (
              <button
                onClick={() => setShowCodeViewer(true)}
                className="p-1.5 rounded hover:bg-trading-border/50 transition-colors text-gray-500 hover:text-trading-blue"
                title="View code"
              >
                <CodeIcon className="w-5 h-5" />
              </button>
            )}
            {isExpanded ? collapseButton : expandButton}
          </div>
        </div>
      )}

      {/* Header slot (e.g., summary stats) */}
      {headerSlot && (
        <div className={clsx("px-6 pb-4 flex-shrink-0", isAnimating && "text-3xl expanded-table-text")}>
          {headerSlot}
        </div>
      )}

      {/* Table */}
      <div className={clsx("overflow-y-auto overflow-x-hidden flex-1 px-6 pb-6", isAnimating && "text-3xl expanded-table-text", tableClassName)}>
        <table className="w-full">
          <thead className={clsx("sticky top-0 bg-trading-card z-10", headerClassName)}>
            <tr className="border-b border-trading-border">
              {columns.map((column) => {
                const isSortable = column.sortable !== false;
                const isActive = sort?.column === String(column.key);
                
                return (
                  <th
                    key={String(column.key)}
                    className={clsx(
                      "py-2 px-2 font-medium text-gray-400",
                      isAnimating ? "text-2xl" : "text-sm",
                      column.align === 'right' && 'text-right',
                      column.align === 'center' && 'text-center',
                      column.align !== 'right' && column.align !== 'center' && 'text-left',
                      column.width,
                      column.headerClassName,
                      isSortable && "cursor-pointer hover:text-gray-200 select-none"
                    )}
                    onClick={() => isSortable && handleHeaderClick(column)}
                  >
                    <span className={clsx(
                      "inline-flex items-center",
                      column.align === 'right' && 'justify-end w-full'
                    )}>
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
                <th className={actionsWidth}></th>
              )}
            </tr>
          </thead>
          <tbody>
            {sortedData?.map((row, index) => {
              const key = rowKey(row);
              const animation = animations.get(key) ?? null;
              
              if (renderRow) {
                return renderRow(
                  row,
                  columns,
                  animation,
                  () => renderDefaultRow(row, index, animation)
                );
              }
              
              return renderDefaultRow(row, index, animation);
            })}
            {(!sortedData || sortedData.length === 0) && (
              <tr>
                <td 
                  colSpan={columns.length + (actions?.length ? 1 : 0)} 
                  className="py-8 text-center text-gray-500"
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
          "bg-trading-card rounded-lg border border-trading-border flex flex-col",
          height,
          className,
          expanded && "invisible"
        )}
      >
        {renderTableCard(false, false)}
      </div>

      {/* Expanded portal overlay */}
      {expanded && createPortal(
        <>
          {/* Backdrop */}
          <div
            className="fixed inset-0 z-50"
            style={{
              backgroundColor: animating ? 'rgba(0,0,0,0.7)' : 'rgba(0,0,0,0)',
              transition: 'background-color 0.35s ease',
            }}
            onClick={handleCollapse}
          />
          {/* Expanded card */}
          <div
            className="bg-trading-card rounded-lg border border-trading-border flex flex-col"
            style={expandedStyle}
          >
            {renderTableCard(true, animating)}
          </div>
        </>,
        document.body
      )}
    </>
  );
}
