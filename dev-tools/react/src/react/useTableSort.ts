// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useCallback, useState } from 'react';

/** A row property name, including computed or non-visible fields. */
export interface SortConfig {
  column: string;
  direction: 'asc' | 'desc';
}

export interface UseTableSortOptions {
  /** Defined (including null) means controlled. Null explicitly clears sorting. */
  sort?: SortConfig | null;
  /** Read once on mount for uncontrolled sorting; ignored while controlled. */
  defaultSort?: SortConfig | null;
  /** One notification per setSort/toggleSort call; never called for prop changes. */
  onSortChange?: (sort: SortConfig | null) => void;
}

export interface UseTableSortResult {
  sort: SortConfig | null;
  /** Request a sort, or null to restore input order. Controlled owners must apply it. */
  setSort: (sort: SortConfig | null) => void;
  /** A new column starts ascending; the active column toggles ascending/descending. */
  toggleSort: (column: string) => void;
}

/** Shared by standalone tables and app-owned presentations of the same rows. */
export function useTableSort({
  sort: controlledSort,
  defaultSort = null,
  onSortChange,
}: UseTableSortOptions = {}): UseTableSortResult {
  const [uncontrolledSort, setUncontrolledSort] = useState(defaultSort);
  const controlled = controlledSort !== undefined;
  const sort = controlled ? controlledSort : uncontrolledSort;
  const setSort = useCallback((next: SortConfig | null) => {
    if (!controlled) setUncontrolledSort(next);
    onSortChange?.(next);
  }, [controlled, onSortChange]);
  const toggleSort = useCallback((column: string) => {
    setSort({
      column,
      direction: sort?.column === column && sort.direction === 'asc' ? 'desc' : 'asc',
    });
  }, [sort, setSort]);
  return { sort, setSort, toggleSort };
}
