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

/**
 * Provider-free presentation and a convenient live-query composition.
 * Styling is an explicit import; tutorial and overlay composition belong to apps.
 */

export { DataTable } from './DataTable';
export type {
  DataTableProps, DataTableState, DataTableRenderContext,
  DataTableErrorContext, DataTableHeaderContext,
} from './DataTable';
export { QueryTable, queryTableState } from './QueryTable';
export type { QueryTableProps, QueryTableRenderContext, QueryTableErrorContext } from './QueryTable';
export type { ColumnDef, RowAction, SortConfig } from './types';
export { CodeIcon, ExpandIcon, CollapseIcon } from './icons';
export { Modal } from './Modal';
export type { ModalProps } from './Modal';
export type { TableHeight } from './sizing';
