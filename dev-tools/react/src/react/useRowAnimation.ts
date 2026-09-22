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
import { useReducedMotion } from './useReducedMotion';

export type AnimationDirection = 'up' | 'down' | 'change' | null;

export interface UseRowAnimationOptions<T> {
  /** Function to extract the unique key for each row. */
  rowKey: (row: T) => string;
  /** Function to extract the value to track for changes. */
  getValue: (row: T) => number | string | undefined;
  /** Decoration-state lifetime after the latest change, in milliseconds (default: 500). */
  animationDuration?: number;
  /** Optionally track supplied rows automatically; null/undefined retains the previous baseline. */
  data?: readonly T[] | null;
}

export interface UseRowAnimationResult<T> {
  /** Map of row keys to their current animation state. */
  animations: Map<string, AnimationDirection>;
  /** Per-row restart tokens, cleared on expiry/removal. Share with DataTable.rowAnimationRevisions. */
  revisions: ReadonlyMap<string, number>;
  /** Update tracked data (call when the data changes). */
  updateData: (data: readonly T[]) => void;
}

/**
 * Track value changes across rows and trigger CSS animations.
 *
 * For numeric values it emits an 'up' or 'down' direction; for string values it
 * emits a neutral 'change'. Revisions advance even for repeated directions;
 * they identify decoration updates, never row identity. `QueryTable` maps these values to the
 * `drasi-row--up`, `drasi-row--down`, and `drasi-row--change` classes shipped in
 * `@drasi/react/styles.css`. Reduced motion cancels active timers/animations,
 * while continuing to track the current baseline for subsequent updates.
 */
export function useRowAnimation<T>(
  options: UseRowAnimationOptions<T>,
): UseRowAnimationResult<T> {
  const { rowKey, getValue, animationDuration = 500, data } = options;
  const reducedMotion = useReducedMotion();

  const [state, setState] = useState(() => ({
    animations: new Map<string, AnimationDirection>(),
    revisions: new Map<string, number>(),
  }));
  const prevValuesRef = useRef<Map<string, number | string>>(new Map());
  const timeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  // Cleanup timeouts on unmount
  useEffect(() => {
    return () => {
      timeoutsRef.current.forEach((timeout) => clearTimeout(timeout));
      timeoutsRef.current.clear();
    };
  }, []);

  useEffect(() => {
    if (!reducedMotion) return;
    timeoutsRef.current.forEach(clearTimeout);
    timeoutsRef.current.clear();
    setState(previous => previous.animations.size === 0 ? previous
      : { animations: new Map(), revisions: new Map() });
  }, [reducedMotion]);

  const updateData = useCallback(
    (data: readonly T[]) => {
      const newAnimations = new Map<string, AnimationDirection>();
      const prevValues = prevValuesRef.current;
      const nextValues = new Map<string, number | string>();
      const currentKeys = new Set<string>();

      if (!data || data.length === 0) {
        timeoutsRef.current.forEach((timeout) => clearTimeout(timeout));
        timeoutsRef.current.clear();
        prevValuesRef.current.clear();
        setState(prev => prev.animations.size === 0 ? prev
          : { animations: new Map(), revisions: new Map() });
        return;
      }

      data.forEach((row) => {
        const key = rowKey(row);
        currentKeys.add(key);
        const currentValue = getValue(row);
        const prevValue = prevValues.get(key);

        if (currentValue !== undefined) {
          nextValues.set(key, currentValue);
        }

        if (
          !reducedMotion &&
          currentValue !== undefined &&
          prevValue !== undefined &&
          currentValue !== prevValue
        ) {
          let direction: AnimationDirection;
          if (typeof currentValue === 'number' && typeof prevValue === 'number') {
            direction = currentValue > prevValue ? 'up' : 'down';
          } else {
            direction = 'change';
          }

          newAnimations.set(key, direction);

          const existingTimeout = timeoutsRef.current.get(key);
          if (existingTimeout) {
            clearTimeout(existingTimeout);
          }

          const timeout = setTimeout(() => {
            setState((prev) => {
              if (!prev.animations.has(key)) return prev;
              const animations = new Map(prev.animations);
              const revisions = new Map(prev.revisions);
              animations.delete(key);
              revisions.delete(key);
              return { animations, revisions };
            });
            timeoutsRef.current.delete(key);
          }, animationDuration);

          timeoutsRef.current.set(key, timeout);
        }
      });

      timeoutsRef.current.forEach((timeout, key) => {
        if (!currentKeys.has(key)) {
          clearTimeout(timeout);
          timeoutsRef.current.delete(key);
        }
      });

      setState((prev) => {
        const animations = new Map(
          Array.from(prev.animations).filter(([key]) => currentKeys.has(key)),
        );
        if (newAnimations.size === 0 && animations.size === prev.animations.size) return prev;
        const revisions = new Map(
          Array.from(prev.revisions).filter(([key]) => currentKeys.has(key)),
        );
        newAnimations.forEach((value, key) => {
          animations.set(key, value);
          revisions.set(key, (prev.revisions.get(key) ?? -1) + 1);
        });
        return { animations, revisions };
      });

      prevValuesRef.current = nextValues;
    },
    [rowKey, getValue, animationDuration, reducedMotion],
  );

  useEffect(() => {
    if (data != null) updateData(data);
  }, [data, updateData]);

  return { ...state, updateData };
}
