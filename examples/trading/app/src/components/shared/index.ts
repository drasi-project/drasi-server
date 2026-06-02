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
 * Shared components for the trading UI.
 *
 * This module exports reusable UI components that are used across
 * multiple trading screens to ensure consistency and reduce code duplication.
 */

// Icons
export {
  ArrowUpIcon,
  ArrowDownIcon,
  EditIcon,
  DeleteIcon,
  RemoveIcon,
  AddIcon,
  CloseIcon,
  CodeIcon,
  ExpandIcon,
  CollapseIcon,
} from './Icons';

// Change indicators
export { ChangeIndicator } from './ChangeIndicator';
export type { ChangeIndicatorProps } from './ChangeIndicator';

// Dialogs
export { BaseDialog, DialogButton } from './BaseDialog';
export type { BaseDialogProps } from './BaseDialog';

export { ConfirmDialog } from './ConfirmDialog';
export type { ConfirmDialogProps, DetailItem } from './ConfirmDialog';

export { SelectDialog } from './SelectDialog';
export type { SelectDialogProps, SelectOption } from './SelectDialog';

export { CodeViewerDialog } from './CodeViewerDialog';
export type { CodeViewerDialogProps } from './CodeViewerDialog';
