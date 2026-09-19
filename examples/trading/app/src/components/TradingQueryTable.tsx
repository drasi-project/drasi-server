// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import clsx from 'clsx';
import {
  DataTable, queryTableState, CodeIcon, ExpandIcon, CollapseIcon, type DataTableProps,
} from '@drasi/react/components';
import {
  useDrasiQuery, useDrasiClient, useTableSort, useRowAnimation, type UseDrasiQueryOptions,
} from '@drasi/react/react';
import type { DrasiError } from '@drasi/react/client';
import { QueryInspector } from './QueryInspector';

export interface TradingQueryTableProps<T extends object> extends Omit<
  DataTableProps<T, DrasiError>, 'rows' | 'state' | 'rowAnimations' | 'containerRef'
> {
  queryId: string;
  queryOptions: UseDrasiQueryOptions<T>;
  codeSnippet?: string;
}

/**
 * One query/animation/sort owner, two presentations during the existing FLIP
 * transition. Trading owns tutorial content and overlays; DataTable owns rows.
 */
export function TradingQueryTable<T extends object>({
  queryId, queryOptions, codeSnippet, sort, defaultSort, onSortChange, ...presentation
}: TradingQueryTableProps<T>) {
  const query = useDrasiQuery(queryId, queryOptions);
  const { retry } = useDrasiClient();
  const sorting = useTableSort({ sort, defaultSort, onSortChange });
  const [showCodeViewer, setShowCodeViewer] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [expandRect, setExpandRect] = useState<DOMRect | null>(null);
  const [animating, setAnimating] = useState(false);
  const frames = useRef<number[]>([]);
  const collapseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previousBodyOverflow = useRef<string | null>(null);
  const getValue = useCallback((row: T) => {
    const value = presentation.animateOnChange === undefined ? undefined : row[presentation.animateOnChange];
    return typeof value === 'number' || typeof value === 'string' ? value : undefined;
  }, [presentation.animateOnChange]);
  const { animations } = useRowAnimation({
    rowKey: presentation.rowKey,
    getValue,
    data: presentation.animateOnChange === undefined ? undefined : query.data,
  });

  const handleExpand = useCallback(() => {
    if (!containerRef.current) return;
    if (collapseTimer.current) {
      clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
    setExpandRect(containerRef.current.getBoundingClientRect());
    setExpanded(true);
    frames.current.push(requestAnimationFrame(() => {
      frames.current.push(requestAnimationFrame(() => setAnimating(true)));
    }));
    previousBodyOverflow.current = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
  }, []);

  const handleCollapse = useCallback(() => {
    frames.current.forEach(cancelAnimationFrame);
    frames.current = [];
    setAnimating(false);
    if (collapseTimer.current) clearTimeout(collapseTimer.current);
    collapseTimer.current = setTimeout(() => {
      setExpanded(false);
      setExpandRect(null);
      document.body.style.overflow = previousBodyOverflow.current ?? '';
      previousBodyOverflow.current = null;
      collapseTimer.current = null;
    }, 350);
  }, []);

  useEffect(() => () => {
    frames.current.forEach(cancelAnimationFrame);
    if (collapseTimer.current) clearTimeout(collapseTimer.current);
    if (previousBodyOverflow.current !== null) document.body.style.overflow = previousBodyOverflow.current;
  }, []);

  useEffect(() => {
    if (!query.error || !expanded) return;
    frames.current.forEach(cancelAnimationFrame);
    frames.current = [];
    if (collapseTimer.current) {
      clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
    setAnimating(false);
    setExpanded(false);
    setExpandRect(null);
    document.body.style.overflow = previousBodyOverflow.current ?? '';
    previousBodyOverflow.current = null;
  }, [query.error, expanded]);

  useEffect(() => {
    if (!expanded) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') handleCollapse();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [expanded, handleCollapse]);

  const expandedStyle = useMemo((): DataTableProps<T, DrasiError>['style'] => {
    if (!expandRect) return undefined;
    return {
      position: 'fixed',
      top: animating ? 32 : expandRect.top,
      left: animating ? 32 : expandRect.left,
      width: animating ? 'calc(100vw - 64px)' : expandRect.width,
      height: animating ? 'calc(100vh - 64px)' : expandRect.height,
      transition: 'all 0.35s cubic-bezier(0.4, 0, 0.2, 1)',
      zIndex: 60,
    };
  }, [expandRect, animating]);

  const renderTable = (isExpanded: boolean, isAnimating: boolean) => (
    <DataTable<T, DrasiError>
      {...presentation}
      rows={query.data}
      state={queryTableState(query, retry)}
      sort={sorting.sort}
      onSortChange={sorting.setSort}
      rowAnimations={animations}
      containerRef={isExpanded ? undefined : containerRef}
      className={isExpanded ? 'drasi-query-table--expanded'
        : clsx(presentation.className, expanded && 'drasi-query-table--hidden')}
      style={isExpanded ? expandedStyle : presentation.style}
      height={isExpanded ? undefined : presentation.height}
      titleClassName={clsx(presentation.titleClassName, isAnimating && 'drasi-query-table__title--expanded')}
      headerSlotClassName={clsx(presentation.headerSlotClassName, isAnimating && 'drasi-expanded-text')}
      tableClassName={clsx(isAnimating && 'drasi-expanded-text', presentation.tableClassName)}
      columns={isAnimating ? presentation.columns.map(column => ({
        ...column, headerClassName: clsx('drasi-query-table__heading--expanded', column.headerClassName),
      })) : presentation.columns}
      headerControls={(presentation.title || presentation.headerActions || presentation.headerControls || codeSnippet) && (
        <>
          {presentation.headerControls}
          {codeSnippet && (
            <button type="button" onClick={() => setShowCodeViewer(true)} className="drasi-icon-button" title="View code" aria-label="View code">
              <CodeIcon className="drasi-icon drasi-icon--medium" />
            </button>
          )}
          <button
            type="button"
            onClick={isExpanded ? handleCollapse : handleExpand}
            className="drasi-icon-button"
            title={isExpanded ? 'Collapse table' : 'Expand table'}
            aria-label={isExpanded ? 'Collapse table' : 'Expand table'}
          >
            {isExpanded ? <CollapseIcon className="drasi-icon drasi-icon--medium" /> : <ExpandIcon className="drasi-icon drasi-icon--medium" />}
          </button>
        </>
      )}
    />
  );
  return (
    <>
      {codeSnippet && showCodeViewer && (
        <QueryInspector
          queryId={queryId}
          title={presentation.title || queryId}
          codeSnippet={codeSnippet}
          onClose={() => setShowCodeViewer(false)}
        />
      )}
      {renderTable(false, false)}
      {expanded && createPortal(
        <>
          <div
            className="drasi-query-table__backdrop"
            style={{
              backgroundColor: animating ? 'rgba(0,0,0,0.7)' : 'rgba(0,0,0,0)',
              transition: 'background-color 0.35s ease',
            }}
            onClick={handleCollapse}
          />
          {renderTable(true, animating)}
        </>,
        document.body,
      )}
    </>
  );
}
