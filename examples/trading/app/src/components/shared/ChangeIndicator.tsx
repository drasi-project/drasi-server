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
import { ArrowUpIcon, ArrowDownIcon } from './Icons';
import { formatPercent } from '@/utils/formatters';

/**
 * Props for the ChangeIndicator component.
 */
export interface ChangeIndicatorProps {
  /** The numeric value to display (percentage change) */
  value: number | null | undefined;
  /** Custom formatter function (defaults to formatPercent) */
  format?: (value: number) => string;
  /** Whether to show the directional arrow (default: true) */
  showArrow?: boolean;
  /** Custom class for the arrow icon */
  arrowClassName?: string;
}

/**
 * ChangeIndicator - Displays a percentage change value with a directional arrow.
 *
 * Used throughout the trading UI to show price changes, P/L percentages, etc.
 * Handles null/undefined values gracefully by showing a dash.
 *
 * @example
 * ```tsx
 * // Basic usage
 * <ChangeIndicator value={2.5} />  // Shows "▲ +2.50%"
 * <ChangeIndicator value={-1.2} /> // Shows "▼ -1.20%"
 *
 * // Without arrow
 * <ChangeIndicator value={2.5} showArrow={false} /> // Shows "+2.50%"
 *
 * // Custom formatting
 * <ChangeIndicator value={1000} format={(v) => `${v.toFixed(0)} pts`} />
 * ```
 */
export const ChangeIndicator: React.FC<ChangeIndicatorProps> = ({
  value,
  format = formatPercent,
  showArrow = true,
  arrowClassName = 'w-3 h-3',
}) => {
  if (value == null) return <>-</>;

  return (
    <span className="inline-flex items-center gap-1">
      {showArrow && (
        value >= 0 ? (
          <ArrowUpIcon className={arrowClassName} />
        ) : (
          <ArrowDownIcon className={arrowClassName} />
        )
      )}
      {format(value)}
    </span>
  );
};
