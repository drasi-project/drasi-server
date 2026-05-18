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

import { useState, useEffect, useRef, useCallback } from 'react';

export type AnimationDirection = 'up' | 'down' | 'change' | null;

export interface UseRowAnimationOptions<T> {
  /** Function to extract the unique key for each row */
  rowKey: (row: T) => string;
  /** Function to extract the value to track for changes */
  getValue: (row: T) => number | string | undefined;
  /** Duration of animation in milliseconds (default: 500) */
  animationDuration?: number;
}

export interface UseRowAnimationResult<T> {
  /** Map of row keys to their current animation state */
  animations: Map<string, AnimationDirection>;
  /** Function to update tracked data (call when data changes) */
  updateData: (data: T[]) => void;
}

/**
 * Hook for tracking value changes across rows and triggering CSS animations.
 * 
 * For numeric values: triggers 'up' or 'down' animation based on direction
 * For string values: triggers 'change' animation (neutral, non-directional)
 * 
 * Usage:
 * ```tsx
 * const { animations, updateData } = useRowAnimation<Stock>({
 *   rowKey: (row) => row.symbol,
 *   getValue: (row) => row.price,
 * });
 * 
 * useEffect(() => {
 *   if (data) updateData(data);
 * }, [data, updateData]);
 * 
 * // In render:
 * const animation = animations.get(row.symbol);
 * <tr className={clsx(
 *   animation === 'up' && 'price-up',
 *   animation === 'down' && 'price-down',
 *   animation === 'change' && 'status-change'
 * )}>
 * ```
 */
export function useRowAnimation<T>(
  options: UseRowAnimationOptions<T>
): UseRowAnimationResult<T> {
  const { rowKey, getValue, animationDuration = 500 } = options;
  
  const [animations, setAnimations] = useState<Map<string, AnimationDirection>>(new Map());
  const prevValuesRef = useRef<Map<string, number | string>>(new Map());
  const timeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  // Cleanup timeouts on unmount
  useEffect(() => {
    return () => {
      timeoutsRef.current.forEach((timeout) => clearTimeout(timeout));
    };
  }, []);

  const updateData = useCallback((data: T[]) => {
    if (!data || data.length === 0) return;

    const newAnimations = new Map<string, AnimationDirection>();
    const prevValues = prevValuesRef.current;

    data.forEach((row) => {
      const key = rowKey(row);
      const currentValue = getValue(row);
      const prevValue = prevValues.get(key);

      if (currentValue !== undefined && prevValue !== undefined && currentValue !== prevValue) {
        let direction: AnimationDirection;
        
        // For numeric values, determine up/down direction
        if (typeof currentValue === 'number' && typeof prevValue === 'number') {
          direction = currentValue > prevValue ? 'up' : 'down';
        } else {
          // For string values (like status), use neutral 'change' animation
          direction = 'change';
        }
        
        newAnimations.set(key, direction);

        // Clear any existing timeout for this key
        const existingTimeout = timeoutsRef.current.get(key);
        if (existingTimeout) {
          clearTimeout(existingTimeout);
        }

        // Set timeout to clear animation
        const timeout = setTimeout(() => {
          setAnimations((prev) => {
            const updated = new Map(prev);
            updated.set(key, null);
            return updated;
          });
          timeoutsRef.current.delete(key);
        }, animationDuration);

        timeoutsRef.current.set(key, timeout);
      }
    });

    // Update animations if there are any changes
    if (newAnimations.size > 0) {
      setAnimations((prev) => {
        const updated = new Map(prev);
        newAnimations.forEach((value, key) => {
          updated.set(key, value);
        });
        return updated;
      });
    }

    // Update previous values reference
    prevValuesRef.current = new Map(
      data.map((row) => {
        const val = getValue(row);
        return [rowKey(row), val ?? (typeof val === 'number' ? 0 : '')];
      })
    );
  }, [rowKey, getValue, animationDuration]);

  return { animations, updateData };
}
