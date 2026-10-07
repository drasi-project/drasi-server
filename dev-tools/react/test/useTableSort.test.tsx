// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode } from 'react';
import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useTableSort, type UseTableSortOptions } from '../src/react';

it('supports headless default state, toggles and explicit clearing without a callback', () => {
  const hook = renderHook(() => useTableSort(), { wrapper: StrictMode });
  expect(hook.result.current.sort).toBeNull();
  act(() => hook.result.current.toggleSort('id'));
  expect(hook.result.current.sort).toEqual({ column: 'id', direction: 'asc' });
  act(() => hook.result.current.toggleSort('id'));
  expect(hook.result.current.sort).toEqual({ column: 'id', direction: 'desc' });
  act(() => hook.result.current.setSort(null));
  expect(hook.result.current.sort).toBeNull();
});

it('preserves stored uncontrolled state across controlled ownership and uses the latest callback', () => {
  const first = vi.fn();
  const next = vi.fn();
  const initialProps: UseTableSortOptions = {
    defaultSort: { column: 'rank', direction: 'desc' }, onSortChange: first,
  };
  const hook = renderHook(options => useTableSort(options), { initialProps, wrapper: StrictMode });
  act(() => hook.result.current.toggleSort('id'));
  expect(first).toHaveBeenCalledExactlyOnceWith({ column: 'id', direction: 'asc' });
  hook.rerender({ sort: null, onSortChange: next });
  expect(hook.result.current.sort).toBeNull();
  act(() => hook.result.current.toggleSort('value'));
  expect(next).toHaveBeenCalledExactlyOnceWith({ column: 'value', direction: 'asc' });
  expect(hook.result.current.sort).toBeNull();
  hook.rerender({ sort: { column: 'value', direction: 'desc' }, onSortChange: next });
  hook.rerender({ defaultSort: null, onSortChange: next });
  expect(hook.result.current.sort).toEqual({ column: 'id', direction: 'asc' });
  expect(first).toHaveBeenCalledTimes(1);
  expect(next).toHaveBeenCalledTimes(1);
});
