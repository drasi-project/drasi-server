// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode } from 'react';
import { act, render, renderHook, screen } from '@testing-library/react';
import { renderToString } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { DataTable } from '../src/components';
import { useReducedMotion, useRowAnimation } from '../src/react';

function motionPreference(initial: boolean) {
  let matches = initial;
  const listeners = new Set<() => void>();
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    get matches() { return matches; },
    addEventListener: (_type: string, callback: () => void) => listeners.add(callback),
    removeEventListener: (_type: string, callback: () => void) => listeners.delete(callback),
  })));
  return {
    listeners,
    set(value: boolean) { act(() => { matches = value; listeners.forEach(callback => callback()); }); },
  };
}

afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

it('tracks preference changes under StrictMode and removes every listener on unmount', () => {
  const preference = motionPreference(false);
  const hook = renderHook(() => useReducedMotion(), { wrapper: StrictMode });
  expect(hook.result.current).toBe(false);
  expect(preference.listeners.size).toBe(1);
  preference.set(true);
  expect(hook.result.current).toBe(true);
  preference.set(false);
  expect(hook.result.current).toBe(false);
  hook.unmount();
  expect(preference.listeners.size).toBe(0);
});

it('has an SSR-safe false snapshot without requiring matchMedia or presentation', () => {
  vi.stubGlobal('matchMedia', undefined);
  expect(renderHook(() => useReducedMotion()).result.current).toBe(false);
  motionPreference(true);
  function State() { return <span>{String(useReducedMotion())}</span>; }
  expect(renderToString(<State />)).toBe('<span>false</span>');
});

it('cancels live animation timers on a mode change and keeps the latest data baseline', () => {
  vi.useFakeTimers();
  const preference = motionPreference(false);
  const hook = renderHook(() => useRowAnimation({
    rowKey: (row: { id: string; value: number }) => row.id,
    getValue: row => row.value,
  }), { wrapper: StrictMode });
  act(() => hook.result.current.updateData([{ id: 'A', value: 10 }]));
  act(() => hook.result.current.updateData([{ id: 'A', value: 20 }]));
  expect(hook.result.current.animations.get('A')).toBe('up');
  const firstRevision = hook.result.current.revisions.get('A');
  expect(typeof firstRevision).toBe('number');
  expect(vi.getTimerCount()).toBe(1);
  preference.set(true);
  expect(hook.result.current.animations.size).toBe(0);
  expect(hook.result.current.revisions.size).toBe(0);
  expect(vi.getTimerCount()).toBe(0);
  act(() => hook.result.current.updateData([{ id: 'A', value: 40 }]));
  expect(hook.result.current.animations.size).toBe(0);
  expect(hook.result.current.revisions.size).toBe(0);
  expect(vi.getTimerCount()).toBe(0);
  preference.set(false);
  act(() => hook.result.current.updateData([{ id: 'A', value: 30 }]));
  expect(hook.result.current.animations.get('A')).toBe('down');
  expect(typeof hook.result.current.revisions.get('A')).toBe('number');
  expect(hook.result.current.revisions.get('A')).not.toBe(firstRevision);
  hook.unmount();
  expect(vi.getTimerCount()).toBe(0);
  expect(preference.listeners.size).toBe(0);
});

it('renders current rows but suppresses even externally controlled animations in reduced mode', () => {
  const preference = motionPreference(true);
  const props = {
    rows: [{ id: 'A', value: 10 }],
    columns: [{ key: 'value', label: 'Value' }],
    rowKey: (row: { id: string }) => row.id,
    rowAnimations: new Map([['A', 'up' as const]]),
  };
  const table = render(<DataTable {...props} />);
  expect(screen.getAllByRole('row')[1].className).not.toContain('drasi-row--up');
  table.rerender(<DataTable {...props} rows={[{ id: 'A', value: 50 }]} />);
  expect(screen.getByRole('cell').textContent).toBe('50');
  preference.set(false);
  expect(screen.getAllByRole('row')[1].className).toContain('drasi-row--up');
});
