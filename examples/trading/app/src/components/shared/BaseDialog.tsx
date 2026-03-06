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

import React, { useEffect, useCallback } from 'react';
import clsx from 'clsx';

/**
 * Props for the BaseDialog component.
 */
export interface BaseDialogProps {
  /** Whether the dialog is open */
  isOpen: boolean;
  /** Callback when the dialog should close */
  onClose: () => void;
  /** Dialog title */
  title: string;
  /** Title color class (e.g., 'text-red-400' for destructive dialogs) */
  titleClassName?: string;
  /** Dialog content */
  children: React.ReactNode;
  /** Footer content (typically action buttons) */
  footer?: React.ReactNode;
  /** Custom width class (default: 'w-96') */
  width?: string;
  /** Whether clicking the overlay closes the dialog (default: true) */
  closeOnOverlayClick?: boolean;
  /** Whether pressing Escape closes the dialog (default: true) */
  closeOnEscape?: boolean;
}

/**
 * BaseDialog - A reusable modal dialog component.
 *
 * Provides a consistent modal overlay with:
 * - Centered content with semi-transparent backdrop
 * - Title with optional color styling
 * - Content area (children)
 * - Footer area for action buttons
 * - Escape key and overlay click handling
 *
 * @example
 * ```tsx
 * <BaseDialog
 *   isOpen={showDialog}
 *   onClose={() => setShowDialog(false)}
 *   title="Confirm Action"
 *   footer={
 *     <>
 *       <Button onClick={onCancel}>Cancel</Button>
 *       <Button onClick={onConfirm}>Confirm</Button>
 *     </>
 *   }
 * >
 *   <p>Are you sure you want to proceed?</p>
 * </BaseDialog>
 * ```
 */
export const BaseDialog: React.FC<BaseDialogProps> = ({
  isOpen,
  onClose,
  title,
  titleClassName,
  children,
  footer,
  width = 'w-96',
  closeOnOverlayClick = true,
  closeOnEscape = true,
}) => {
  // Handle escape key
  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (closeOnEscape && event.key === 'Escape') {
        onClose();
      }
    },
    [closeOnEscape, onClose]
  );

  useEffect(() => {
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      // Prevent body scroll when dialog is open
      document.body.style.overflow = 'hidden';
    }

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = '';
    };
  }, [isOpen, handleKeyDown]);

  // Handle overlay click
  const handleOverlayClick = (event: React.MouseEvent) => {
    if (closeOnOverlayClick && event.target === event.currentTarget) {
      onClose();
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-[100]"
      onClick={handleOverlayClick}
    >
      <div
        className={clsx(
          'bg-trading-card border border-trading-border rounded-lg p-6 max-w-[90vw]',
          width
        )}
      >
        {/* Title */}
        <h3 className={clsx('text-lg font-bold mb-4', titleClassName)}>
          {title}
        </h3>

        {/* Content */}
        <div>{children}</div>

        {/* Footer */}
        {footer && (
          <div className="flex gap-3 justify-end mt-4">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
};

/**
 * Standard button styles for dialog actions.
 */
export const DialogButton: React.FC<{
  onClick: () => void;
  disabled?: boolean;
  variant?: 'default' | 'primary' | 'danger';
  children: React.ReactNode;
}> = ({ onClick, disabled, variant = 'default', children }) => {
  const baseClasses = 'px-4 py-2 rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed';
  
  const variantClasses = {
    default: 'border border-trading-border hover:bg-trading-border/30',
    primary: 'bg-trading-blue hover:bg-trading-blue/80',
    danger: 'bg-red-600 hover:bg-red-700',
  };

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={clsx(baseClasses, variantClasses[variant])}
    >
      {children}
    </button>
  );
};
