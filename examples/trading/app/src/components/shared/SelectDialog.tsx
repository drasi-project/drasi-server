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

import React, { useState, useEffect } from 'react';
import { BaseDialog, DialogButton } from './BaseDialog';

/**
 * An option in the select dropdown.
 */
export interface SelectOption {
  value: string;
  label: string;
}

/**
 * Props for the SelectDialog component.
 */
export interface SelectDialogProps {
  /** Whether the dialog is open */
  isOpen: boolean;
  /** Callback when the dialog should close (cancel action) */
  onClose: () => void;
  /** Callback when the user confirms with a selection */
  onConfirm: (value: string) => void;
  /** Dialog title (e.g., "Add to Watchlist") */
  title: string;
  /** Label for the select dropdown */
  selectLabel?: string;
  /** Available options */
  options: SelectOption[];
  /** Message to show when no options are available */
  emptyMessage?: string;
  /** Confirm button text (default: "Add") */
  confirmText?: string;
  /** Cancel button text (default: "Cancel") */
  cancelText?: string;
  /** Whether the confirm action is in progress */
  isLoading?: boolean;
  /** Loading text for confirm button */
  loadingText?: string;
}

/**
 * SelectDialog - A dialog for selecting from a dropdown and confirming.
 *
 * Used for simple "select and confirm" flows like adding items to a list.
 *
 * @example
 * ```tsx
 * <SelectDialog
 *   isOpen={showAddDialog}
 *   onClose={() => setShowAddDialog(false)}
 *   onConfirm={handleAdd}
 *   title="Add to Watchlist"
 *   selectLabel="Select Stock"
 *   options={stocks.map(s => ({ value: s.symbol, label: `${s.symbol} - ${s.name}` }))}
 *   emptyMessage="No stocks available to add."
 *   confirmText="Add"
 *   isLoading={isAdding}
 *   loadingText="Adding..."
 * />
 * ```
 */
export const SelectDialog: React.FC<SelectDialogProps> = ({
  isOpen,
  onClose,
  onConfirm,
  title,
  selectLabel = 'Select',
  options,
  emptyMessage = 'No options available.',
  confirmText = 'Add',
  cancelText = 'Cancel',
  isLoading = false,
  loadingText = 'Adding...',
}) => {
  const [selectedValue, setSelectedValue] = useState('');

  // Reset selection when dialog opens or options change
  useEffect(() => {
    if (isOpen && options.length > 0) {
      setSelectedValue(options[0].value);
    } else if (!isOpen) {
      setSelectedValue('');
    }
  }, [isOpen, options]);

  const handleConfirm = () => {
    if (selectedValue) {
      onConfirm(selectedValue);
    }
  };

  const hasOptions = options.length > 0;

  return (
    <BaseDialog
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      footer={
        <>
          <DialogButton onClick={onClose} disabled={isLoading}>
            {cancelText}
          </DialogButton>
          <DialogButton
            onClick={handleConfirm}
            disabled={isLoading || !hasOptions || !selectedValue}
            variant="primary"
          >
            {isLoading ? loadingText : confirmText}
          </DialogButton>
        </>
      }
    >
      {!hasOptions ? (
        <p className="text-gray-400">{emptyMessage}</p>
      ) : (
        <>
          <label className="block text-sm text-gray-400 mb-2">{selectLabel}</label>
          <select
            value={selectedValue}
            onChange={(e) => setSelectedValue(e.target.value)}
            className="w-full bg-trading-bg border border-trading-border rounded p-2 text-white"
          >
            {options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </>
      )}
    </BaseDialog>
  );
};
