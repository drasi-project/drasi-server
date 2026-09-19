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

import type { ReactNode } from 'react';
export type { SortConfig } from '../react/useTableSort';

/** Column definition shared by DataTable and QueryTable. */
export interface ColumnDef<T extends object = Record<string, unknown>> {
  /** Property key on the data object, or a custom string for computed columns. */
  key: keyof T | string;
  /** Column header label. */
  label: string;
  /**
   * Render a cell. The raw value is unknown because custom column keys may not
   * exist on the row. Use the typed row for property-specific or computed cells:
   * `format: (_value, row) => row.price.toFixed(2)`.
   */
  format?: (value: unknown, row: T) => ReactNode;
  /** Whether this column is sortable (default: true). */
  sortable?: boolean;
  /** Text alignment (default: 'left'). */
  align?: 'left' | 'center' | 'right';
  /** Additional CSS classes; callbacks receive the raw value and typed row. */
  className?: string | ((value: unknown, row: T) => string);
  /** Additional CSS classes for the header cell. */
  headerClassName?: string;
  /** CSS width for the header cell (for example, `5rem` or `120px`). */
  width?: string;
}

/** Row action definition shared by DataTable and QueryTable. */
export interface RowAction<T = Record<string, unknown>> {
  /** Icon element to display. */
  icon: ReactNode;
  /** Accessibility label. */
  label: string;
  /** Click handler. */
  onClick: (row: T) => void;
  /** Additional CSS classes. */
  className?: string;
  /** Hover CSS classes. */
  hoverClassName?: string;
  /** Whether the action is disabled for this row. */
  disabled?: (row: T) => boolean;
  /** Whether the action is loading for this row. */
  loading?: (row: T) => boolean;
}
