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
  useRef,
} from 'react';
import { Modal } from '@drasi/react/components';
import * as Tabs from '@radix-ui/react-tabs';
import clsx from 'clsx';
import './CodeViewerDialog.css';

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
  /** App-owned asynchronous query inspection feedback. */
  statusSlot?: React.ReactNode;
}

type TabId = 'react' | 'cypher';

/**
 * Trading's tutorial presentation with shared modal behavior and live definition content.
 */
export const CodeViewerDialog: React.FC<CodeViewerDialogProps> = ({
  isOpen,
  onClose,
  title,
  reactCode,
  cypherQuery,
  drasiUiUrl,
  statusSlot,
}) => {
  const [activeTab, setActiveTab] = useState<TabId>('cypher');
  const [copied, setCopied] = useState(false);
  const [copyFailed, setCopyFailed] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const copyAttemptRef = useRef(0);

  useEffect(() => {
    if (isOpen) {
      setActiveTab('cypher');
      setCopied(false);
      setCopyFailed(false);
    }

    return () => {
      // Clipboard writes cannot be aborted; invalidate feedback from a closed viewer.
      copyAttemptRef.current += 1;
      if (copyTimerRef.current !== null) {
        clearTimeout(copyTimerRef.current);
        copyTimerRef.current = null;
      }
    };
  }, [isOpen]);

  const handleCopy = async () => {
    const textToCopy = activeTab === 'react' ? reactCode : cypherQuery;
    const attempt = ++copyAttemptRef.current;
    try {
      await navigator.clipboard.writeText(textToCopy);
      if (attempt !== copyAttemptRef.current) return;

      setCopied(true);
      setCopyFailed(false);
      if (copyTimerRef.current !== null) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => {
        setCopied(false);
        copyTimerRef.current = null;
      }, 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
      if (attempt !== copyAttemptRef.current) return;

      setCopied(false);
      setCopyFailed(true);
      if (copyTimerRef.current !== null) {
        clearTimeout(copyTimerRef.current);
        copyTimerRef.current = null;
      }
    }
  };

  return (
    <Modal
      open={isOpen}
      title={title}
      onClose={onClose}
      className="drasi-code-dialog"
      overlayClassName="drasi-code-dialog__overlay"
    >
      <Tabs.Root
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as TabId)}
        activationMode="automatic"
        className="drasi-code-dialog__tab-root"
      >
        {/* Header */}
        <div className="drasi-code-dialog__header">
          <div className="drasi-code-dialog__title-group">
            <h2 className="drasi-code-dialog__title">
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
                  aria-hidden="true"
                  focusable="false"
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
              aria-hidden="true"
              focusable="false"
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
        <div className="drasi-code-dialog__tabs">
          <Tabs.List
            className="drasi-code-dialog__tablist"
            aria-label="Code examples"
          >
            <Tabs.Trigger
              value="cypher"
              className={clsx(
                'drasi-code-dialog__tab',
                activeTab === 'cypher'
                  ? 'drasi-code-dialog__tab--active'
                  : 'drasi-code-dialog__tab--inactive',
              )}
            >
              Query Definition
            </Tabs.Trigger>
            <Tabs.Trigger
              value="react"
              className={clsx(
                'drasi-code-dialog__tab',
                activeTab === 'react'
                  ? 'drasi-code-dialog__tab--active'
                  : 'drasi-code-dialog__tab--inactive',
              )}
            >
              React Code
            </Tabs.Trigger>
          </Tabs.List>

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
                    aria-hidden="true"
                    focusable="false"
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
                    aria-hidden="true"
                    focusable="false"
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
            <span role="status" className="drasi-code-dialog__status">
              {copyFailed
                ? 'Unable to copy code. Select and copy the text instead.'
                : copied ? 'Code copied to clipboard.' : ''}
            </span>
          </div>
        </div>

        {/* Code content */}
        <Tabs.Content
          value="cypher"
          tabIndex={0}
          className="drasi-code-dialog__content"
        >
          {statusSlot}
          <pre className="drasi-code-dialog__code">
            <code>{cypherQuery}</code>
          </pre>
        </Tabs.Content>
        <Tabs.Content
          value="react"
          tabIndex={0}
          className="drasi-code-dialog__content"
        >
          {statusSlot}
          <pre className="drasi-code-dialog__code">
            <code>{reactCode}</code>
          </pre>
        </Tabs.Content>
      </Tabs.Root>
    </Modal>
  );
};
