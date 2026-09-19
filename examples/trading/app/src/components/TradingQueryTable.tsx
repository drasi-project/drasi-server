// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import clsx from 'clsx';
import {
  DataTable, Modal, queryTableState, CodeIcon, ExpandIcon, CollapseIcon, type DataTableProps,
} from '@drasi/react/components';
import {
  useDrasiQuery, useDrasiClient, useTableSort, useRowAnimation, useReducedMotion, type UseDrasiQueryOptions,
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
  const expandButton = useRef<HTMLButtonElement>(null);
  const reducedMotion = useReducedMotion();
  const [expanded, setExpanded] = useState(false);
  const [expandRect, setExpandRect] = useState<DOMRect | null>(null);
  const [animating, setAnimating] = useState(false);
  const frames = useRef<number[]>([]);
  const collapseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closing = useRef(false);
  const getValue = useCallback((row: T) => {
    const value = presentation.animateOnChange === undefined ? undefined : row[presentation.animateOnChange];
    return typeof value === 'number' || typeof value === 'string' ? value : undefined;
  }, [presentation.animateOnChange]);
  const { animations } = useRowAnimation({
    rowKey: presentation.rowKey,
    getValue,
    data: presentation.animateOnChange === undefined ? undefined : query.data,
  });

  const cancelTransition = useCallback(() => {
    frames.current.forEach(frame => cancelAnimationFrame(frame));
    frames.current = [];
    if (collapseTimer.current) {
      clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
  }, []);

  const finishCollapse = useCallback(() => {
    cancelTransition();
    closing.current = false;
    setAnimating(false);
    setExpanded(false);
    setExpandRect(null);
  }, [cancelTransition]);

  const handleExpand = useCallback(() => {
    if (!containerRef.current) return;
    cancelTransition();
    closing.current = false;
    setExpandRect(containerRef.current.getBoundingClientRect());
    setExpanded(true);
    setAnimating(reducedMotion);
    if (!reducedMotion) {
      frames.current.push(requestAnimationFrame(() => {
        frames.current.push(requestAnimationFrame(() => {
          frames.current = [];
          setAnimating(true);
        }));
      }));
    }
  }, [cancelTransition, reducedMotion]);

  const handleCollapse = useCallback(() => {
    if (closing.current) return;
    closing.current = true;
    cancelTransition();
    if (reducedMotion) {
      finishCollapse();
      return;
    }
    setAnimating(false);
    collapseTimer.current = setTimeout(finishCollapse, 350);
  }, [cancelTransition, finishCollapse, reducedMotion]);

  useEffect(() => cancelTransition, [cancelTransition]);

  useEffect(() => {
    if (!query.error || !expanded) return;
    finishCollapse();
  }, [query.error, expanded, finishCollapse]);

  useEffect(() => {
    if (!expanded || !reducedMotion) return;
    cancelTransition();
    if (closing.current) finishCollapse();
    else setAnimating(true);
  }, [expanded, reducedMotion, cancelTransition, finishCollapse]);

  const expandedStyle = useMemo((): DataTableProps<T, DrasiError>['style'] => {
    if (!expandRect) return undefined;
    return {
      position: 'fixed',
      top: animating ? 32 : expandRect.top,
      left: animating ? 32 : expandRect.left,
      width: animating ? 'calc(100vw - 64px)' : expandRect.width,
      height: animating ? 'calc(100vh - 64px)' : expandRect.height,
      transition: reducedMotion ? 'none' : 'all 0.35s cubic-bezier(0.4, 0, 0.2, 1)',
      zIndex: 60,
    };
  }, [expandRect, animating, reducedMotion]);

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
      style={isExpanded ? { height: '100%' } : presentation.style}
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
            <button type="button" onClick={() => setShowCodeViewer(true)} className="drasi-icon-button" title="View code" aria-label="View code" aria-haspopup="dialog">
              <CodeIcon className="drasi-icon drasi-icon--medium" />
            </button>
          )}
          <button
            type="button"
            ref={isExpanded ? undefined : expandButton}
            onClick={isExpanded ? handleCollapse : handleExpand}
            className="drasi-icon-button"
            title={isExpanded ? 'Collapse table' : 'Expand table'}
            aria-label={isExpanded ? 'Collapse table' : 'Expand table'}
            aria-haspopup={isExpanded ? undefined : 'dialog'}
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
      {expanded && (
        <Modal
          open
          title={presentation.title || queryId}
          onClose={handleCollapse}
          returnFocusRef={expandButton}
          themeRef={containerRef}
          className="trading-expanded-dialog"
          style={expandedStyle}
          overlayClassName="drasi-query-table__backdrop"
          overlayStyle={{
            backgroundColor: animating ? 'rgba(0,0,0,0.7)' : 'rgba(0,0,0,0)',
            transition: reducedMotion ? 'none' : 'background-color 0.35s ease',
          }}
        >
          {renderTable(true, animating)}
        </Modal>
      )}
    </>
  );
}
