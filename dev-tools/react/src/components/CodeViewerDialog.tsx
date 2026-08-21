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

import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
  useId,
} from 'react';
import { createPortal } from 'react-dom';
import clsx from 'clsx';

export interface CodeViewerDialogProps {
  /** Whether the dialog is open. */
  isOpen: boolean;
  /** Callback when the dialog should close. */
  onClose: () => void;
  /** Title for the dialog (e.g., component name). */
  title: string;
  /** Consumer code snippet (for example, the React usage). */
  reactCode: string;
  /** The query definition / source code string. */
  cypherQuery: string;
  /** Optional URL to open the query in the Drasi Server UI. */
  drasiUiUrl?: string | null;
}

type TabId = 'react' | 'cypher';

/**
 * CodeViewerDialog displays the query definition alongside the consumer code
 * for a {@link QueryTable}. It is presentation-friendly (large monospace text)
 * and rendered via a portal so it never shifts when the underlying data changes.
 */
export const CodeViewerDialog: React.FC<CodeViewerDialogProps> = ({
  isOpen,
  onClose,
  title,
  reactCode,
  cypherQuery,
  drasiUiUrl,
}) => {
  const [activeTab, setActiveTab] = useState<TabId>('cypher');
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previousBodyOverflowRef = useRef<string | null>(null);
  const titleId = useId();

  // Freeze the code content while the dialog is open.
  const memoizedReactCode = useMemo(() => reactCode, [isOpen ? null : reactCode]);
  const memoizedCypherQuery = useMemo(() => cypherQuery, [isOpen ? null : cypherQuery]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    },
    [onClose],
  );

  useEffect(() => {
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      previousBodyOverflowRef.current = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
    }
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      if (previousBodyOverflowRef.current !== null) {
        document.body.style.overflow = previousBodyOverflowRef.current;
        previousBodyOverflowRef.current = null;
      }
    };
  }, [isOpen, handleKeyDown]);

  useEffect(() => {
    if (isOpen) {
      setActiveTab('cypher');
      setCopied(false);
    }
  }, [isOpen]);

  const handleCopy = async () => {
    const textToCopy = activeTab === 'react' ? memoizedReactCode : memoizedCypherQuery;
    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopied(true);
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => {
        setCopied(false);
        copyTimerRef.current = null;
      }, 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  useEffect(
    () => () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    },
    [],
  );

  const handleOverlayClick = (event: React.MouseEvent) => {
    if (event.target === event.currentTarget) {
      onClose();
    }
  };

  if (!isOpen) return null;

  const currentCode = activeTab === 'react' ? memoizedReactCode : memoizedCypherQuery;

  return createPortal(
    <div
      className="drasi-code-dialog__overlay"
      onClick={handleOverlayClick}
    >
      <div
        className="drasi-code-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        {/* Header */}
        <div className="drasi-code-dialog__header">
          <div className="drasi-code-dialog__title-group">
            <h2 id={titleId} className="drasi-code-dialog__title">
              {title}
            </h2>
            {drasiUiUrl && (
              <a
                href={drasiUiUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="drasi-code-dialog__external-link"
                title="Open in Drasi Server UI"
              >
                <svg
                  className="drasi-icon"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                  />
                </svg>
                Open in Drasi UI
              </a>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="drasi-icon-button"
            title="Close"
            aria-label="Close"
          >
            <svg
              className="drasi-icon drasi-icon--medium"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>

        {/* Tabs */}
        <div className="drasi-code-dialog__tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'cypher'}
            onClick={() => setActiveTab('cypher')}
            className={clsx(
              'drasi-code-dialog__tab',
              activeTab === 'cypher'
                ? 'drasi-code-dialog__tab--active'
                : 'drasi-code-dialog__tab--inactive',
            )}
          >
            Query Definition
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'react'}
            onClick={() => setActiveTab('react')}
            className={clsx(
              'drasi-code-dialog__tab',
              activeTab === 'react'
                ? 'drasi-code-dialog__tab--active'
                : 'drasi-code-dialog__tab--inactive',
            )}
          >
            React Code
          </button>

          {/* Copy button */}
          <div className="drasi-code-dialog__copy-container">
            <button
              type="button"
              onClick={handleCopy}
              className={clsx(
                'drasi-code-dialog__copy-button',
                copied
                  ? 'drasi-code-dialog__copy-button--copied'
                  : 'drasi-code-dialog__copy-button--idle',
              )}
            >
              {copied ? (
                <>
                  <svg
                    className="drasi-icon"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M5 13l4 4L19 7"
                    />
                  </svg>
                  Copied!
                </>
              ) : (
                <>
                  <svg
                    className="drasi-icon"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                    />
                  </svg>
                  Copy
                </>
              )}
            </button>
          </div>
        </div>

        {/* Code content */}
        <div className="drasi-code-dialog__content">
          <pre className="drasi-code-dialog__code">
            <code>{currentCode}</code>
          </pre>
        </div>
      </div>
    </div>,
    document.body,
  );
};
