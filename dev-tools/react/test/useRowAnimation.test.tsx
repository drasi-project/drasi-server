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

import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useRowAnimation } from '../src/react/useRowAnimation';

interface Row {
  id: string;
  value?: number;
}

const rowKey = (row: Row) => row.id;
const getValue = (row: Row) => row.value;

afterEach(() => {
  vi.useRealTimers();
});

describe('useRowAnimation', () => {
  it('does not animate the first defined value after undefined', () => {
    const { result } = renderHook(() =>
      useRowAnimation({ rowKey, getValue }),
    );

    act(() => result.current.updateData([{ id: 'A' }]));
    act(() => result.current.updateData([{ id: 'A', value: 10 }]));

    expect(result.current.animations.size).toBe(0);
  });

  it('preserves animation state identity when data has not changed', () => {
    const { result } = renderHook(() =>
      useRowAnimation({ rowKey, getValue }),
    );

    act(() => result.current.updateData([{ id: 'A', value: 10 }]));
    const animations = result.current.animations;
    act(() => result.current.updateData([{ id: 'A', value: 10 }]));

    expect(result.current.animations).toBe(animations);
  });

  it('animates changes and removes state for deleted rows', () => {
    vi.useFakeTimers();
    const { result } = renderHook(() =>
      useRowAnimation({
        rowKey,
        getValue,
        animationDuration: 100,
      }),
    );

    act(() => result.current.updateData([{ id: 'A', value: 10 }]));
    act(() => result.current.updateData([{ id: 'A', value: 11 }]));
    expect(result.current.animations.get('A')).toBe('up');

    act(() => result.current.updateData([]));
    expect(result.current.animations.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});
