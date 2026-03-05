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

import React from 'react';
import { BaseDialog, DialogButton } from './BaseDialog';

/**
 * A key-value pair for the detail section.
 */
export interface DetailItem {
  label: string;
  value: React.ReactNode;
  /** Optional class for the value (e.g., for colored text) */
  valueClassName?: string;
}

/**
 * Props for the ConfirmDialog component.
 */
export interface ConfirmDialogProps {
  /** Whether the dialog is open */
  isOpen: boolean;
  /** Callback when the dialog should close (cancel action) */
  onClose: () => void;
  /** Callback when the user confirms the action */
  onConfirm: () => void;
  /** Dialog title (e.g., "Delete Position") */
  title: string;
  /** Optional details to display about the item being acted upon */
  details?: DetailItem[];
  /** Warning/info message (e.g., "This action cannot be undone.") */
  message?: string;
  /** Confirm button text (default: "Confirm") */
  confirmText?: string;
  /** Cancel button text (default: "Cancel") */
  cancelText?: string;
  /** Whether the confirm action is in progress */
  isLoading?: boolean;
  /** Loading text for confirm button (default: "Processing...") */
  loadingText?: string;
  /** Button variant for confirm button (default: "danger") */
  confirmVariant?: 'default' | 'primary' | 'danger';
}

/**
 * ConfirmDialog - A specialized dialog for confirming destructive or important actions.
 *
 * Features:
 * - Optional detail section showing key-value pairs about the affected item
 * - Warning message
 * - Confirm/Cancel buttons with loading state
 * - Styled for destructive actions by default (red title and confirm button)
 *
 * @example
 * ```tsx
 * <ConfirmDialog
 *   isOpen={showDelete}
 *   onClose={() => setShowDelete(false)}
 *   onConfirm={handleDelete}
 *   title="Delete Position"
 *   details={[
 *     { label: 'Stock', value: 'AAPL - Apple Inc.' },
 *     { label: 'Quantity', value: '100 shares' },
 *     { label: 'Value', value: '$15,000', valueClassName: 'font-bold' },
 *   ]}
 *   message="This action cannot be undone."
 *   confirmText="Delete"
 *   isLoading={isDeleting}
 *   loadingText="Deleting..."
 * />
 * ```
 */
export const ConfirmDialog: React.FC<ConfirmDialogProps> = ({
  isOpen,
  onClose,
  onConfirm,
  title,
  details,
  message,
  confirmText = 'Confirm',
  cancelText = 'Cancel',
  isLoading = false,
  loadingText = 'Processing...',
  confirmVariant = 'danger',
}) => {
  return (
    <BaseDialog
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      titleClassName={confirmVariant === 'danger' ? 'text-red-400' : undefined}
      footer={
        <>
          <DialogButton onClick={onClose} disabled={isLoading}>
            {cancelText}
          </DialogButton>
          <DialogButton
            onClick={onConfirm}
            disabled={isLoading}
            variant={confirmVariant}
          >
            {isLoading ? loadingText : confirmText}
          </DialogButton>
        </>
      }
    >
      {/* Detail section */}
      {details && details.length > 0 && (
        <div className="bg-trading-bg rounded p-4 mb-4 space-y-2">
          {details.map((detail, index) => (
            <div key={index} className="flex justify-between">
              <span className="text-gray-400">{detail.label}:</span>
              <span className={detail.valueClassName}>{detail.value}</span>
            </div>
          ))}
        </div>
      )}

      {/* Warning message */}
      {message && (
        <p className="text-gray-400 text-sm">{message}</p>
      )}
    </BaseDialog>
  );
};
