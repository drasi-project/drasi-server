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
import { afterEach, describe, expect, expectTypeOf, it, vi } from 'vitest';
import {
  useRowAnimation,
  type AnimationDirection,
  type UseRowAnimationOptions,
  type UseRowAnimationResult,
} from '../src/react/useRowAnimation';

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

  it('infers a non-Trading row shape and tracks string and numeric changes without index signatures', () => {
    vi.useFakeTimers();
    interface Sensor {
      address: { room: string };
      reading: number | string | undefined;
    }
    const options: UseRowAnimationOptions<Sensor> = {
      rowKey: row => row.address.room,
      getValue: row => row.reading,
      animationDuration: 100,
    };
    const { result, unmount } = renderHook(() => useRowAnimation(options));
    expectTypeOf(result.current).toEqualTypeOf<UseRowAnimationResult<Sensor>>();
    expectTypeOf(result.current.animations).toEqualTypeOf<Map<string, AnimationDirection>>();

    const row = (reading: Sensor['reading']): Sensor => ({ address: { room: 'boiler-room' }, reading });
    act(() => result.current.updateData([row(12)]));
    act(() => result.current.updateData([row(10)]));
    expect(result.current.animations.get('boiler-room')).toBe('down');
    act(() => result.current.updateData([row('offline')]));
    expect(result.current.animations.get('boiler-room')).toBe('change');
    act(() => vi.advanceTimersByTime(100));
    expect(result.current.animations.size).toBe(0);

    act(() => result.current.updateData([row('online')]));
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('automatically tracks readonly data, retains its baseline during loading and removes deleted keys', () => {
    vi.useFakeTimers();
    const initialProps: { data: readonly Row[] | null } = {
      data: Object.freeze([{ id: 'A', value: 10 }, { id: 'B', value: 5 }]),
    };
    const hook = renderHook(({ data }) => useRowAnimation({ rowKey, getValue, data }), { initialProps });
    hook.rerender({ data: null });
    hook.rerender({ data: Object.freeze([{ id: 'B', value: 5 }, { id: 'A', value: 11 }]) });
    expect(hook.result.current.animations.get('A')).toBe('up');
    expect(vi.getTimerCount()).toBe(1);
    hook.rerender({ data: Object.freeze([{ id: 'B', value: 5 }]) });
    expect(hook.result.current.animations.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});
