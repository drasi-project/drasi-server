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

import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import clsx from 'clsx';

export interface CodeViewerDialogProps {
  /** Whether the dialog is open */
  isOpen: boolean;
  /** Callback when the dialog should close */
  onClose: () => void;
  /** Title for the dialog (e.g., component name) */
  title: string;
  /** React component code snippet */
  reactCode: string;
  /** Cypher query string */
  cypherQuery: string;
  /** URL to open the Drasi Server UI for this query's instance */
  drasiUiUrl?: string | null;
}

type TabId = 'react' | 'cypher';

/**
 * CodeViewerDialog - A large dialog for displaying code snippets during presentations.
 * 
 * Features:
 * - Two tabs: React Code and Cypher Query
 * - Large, presentation-friendly monospace text
 * - Copy-to-clipboard functionality
 * - Dark theme matching the trading UI
 * - Rendered via portal to prevent jumping when underlying data changes
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

  // Memoize the code content to prevent re-renders from changing it while dialog is open
  const memoizedReactCode = useMemo(() => reactCode, [isOpen ? null : reactCode]);
  const memoizedCypherQuery = useMemo(() => cypherQuery, [isOpen ? null : cypherQuery]);

  // Handle escape key
  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    },
    [onClose]
  );

  useEffect(() => {
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      document.body.style.overflow = 'hidden';
    }

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = '';
    };
  }, [isOpen, handleKeyDown]);

  // Reset state when dialog opens
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
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const handleOverlayClick = (event: React.MouseEvent) => {
    if (event.target === event.currentTarget) {
      onClose();
    }
  };

  if (!isOpen) return null;

  const currentCode = activeTab === 'react' ? memoizedReactCode : memoizedCypherQuery;

  // Use portal to render outside the component tree, preventing layout jumps
  return createPortal(
    <div
      className="fixed inset-0 bg-black/70 flex items-center justify-center z-[100] p-4 animate-fade-in"
      onClick={handleOverlayClick}
    >
      <div className="bg-[#1e2433] border border-trading-border/50 rounded-lg w-[90vw] h-[85vh] max-w-6xl flex flex-col shadow-2xl animate-slide-up">
        {/* Header */}
        <div className="flex justify-between items-center p-4 border-b border-trading-border/50 flex-shrink-0">
          <div className="flex items-center gap-3">
            <h2 className="text-xl font-bold text-white">{title}</h2>
            {drasiUiUrl && (
              <a
                href={drasiUiUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 px-3 py-1 text-sm rounded bg-trading-border/30 hover:bg-trading-border/50 text-trading-blue hover:text-blue-300 transition-colors"
                title="Open in Drasi Server UI"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
                Open in Drasi UI
              </a>
            )}
          </div>
          <button
            onClick={onClose}
            className="p-2 rounded hover:bg-trading-border/50 transition-colors text-gray-400 hover:text-white"
            title="Close"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-trading-border flex-shrink-0">
          <button
            onClick={() => setActiveTab('cypher')}
            className={clsx(
              "px-6 py-3 text-sm font-medium transition-colors",
              activeTab === 'cypher'
                ? "text-trading-blue border-b-2 border-trading-blue bg-trading-border/20"
                : "text-gray-400 hover:text-gray-200 hover:bg-trading-border/10"
            )}
          >
            Query Definition
          </button>
          <button
            onClick={() => setActiveTab('react')}
            className={clsx(
              "px-6 py-3 text-sm font-medium transition-colors",
              activeTab === 'react'
                ? "text-trading-blue border-b-2 border-trading-blue bg-trading-border/20"
                : "text-gray-400 hover:text-gray-200 hover:bg-trading-border/10"
            )}
          >
            React Code
          </button>
          
          {/* Copy button */}
          <div className="ml-auto flex items-center pr-4">
            <button
              onClick={handleCopy}
              className={clsx(
                "px-3 py-1.5 text-sm rounded transition-colors flex items-center gap-2",
                copied
                  ? "bg-trading-green/20 text-trading-green"
                  : "bg-trading-border/30 hover:bg-trading-border/50 text-gray-300"
              )}
            >
              {copied ? (
                <>
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                  Copied!
                </>
              ) : (
                <>
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                  </svg>
                  Copy
                </>
              )}
            </button>
          </div>
        </div>

        {/* Code content */}
        <div className="flex-1 overflow-auto p-6 bg-[#0d1117]">
          <pre className="text-2xl leading-relaxed font-mono whitespace-pre-wrap text-gray-200">
            <code>{currentCode}</code>
          </pre>
        </div>
      </div>
    </div>,
    document.body
  );
};
