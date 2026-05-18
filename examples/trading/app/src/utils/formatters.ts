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
 * Shared formatting utilities for the trading UI.
 */

export interface CurrencyFormatOptions {
  currency?: string;
  minimumFractionDigits?: number;
  maximumFractionDigits?: number;
}

/**
 * Format a number as currency (USD by default).
 */
export function formatCurrency(
  value: number | null | undefined,
  options: CurrencyFormatOptions = {}
): string {
  if (value == null || isNaN(value)) return '-';
  
  const {
    currency = 'USD',
    minimumFractionDigits = 2,
    maximumFractionDigits = 2,
  } = options;

  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits,
    maximumFractionDigits,
  }).format(value);
}

/**
 * Format a number as a percentage with optional sign prefix.
 */
export function formatPercent(
  value: number | null | undefined,
  showSign: boolean = true
): string {
  if (value == null || isNaN(value)) return '-';
  
  const formatted = Math.abs(value).toFixed(2);
  if (showSign) {
    return `${value >= 0 ? '+' : '-'}${formatted}%`;
  }
  return `${formatted}%`;
}

/**
 * Format large numbers with K/M/B suffixes for volume display.
 */
export function formatVolume(value: number | null | undefined): string {
  if (value == null || isNaN(value)) return '-';
  
  if (value >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(2)}B`;
  }
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(2)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(2)}K`;
  }
  return value.toString();
}

/**
 * Format large numbers compactly with K/M/B suffixes (1 decimal place).
 */
export function formatCompactNumber(value: number | null | undefined): string {
  if (value == null || isNaN(value)) return '-';
  
  if (value >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(1)}B`;
  }
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return value.toLocaleString();
}

/**
 * Format a price value (alias for formatCurrency with common defaults).
 */
export function formatPrice(value: number | null | undefined): string {
  return formatCurrency(value);
}
