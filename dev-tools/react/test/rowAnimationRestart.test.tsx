// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode, useState } from 'react';
import { act, fireEvent, render, renderHook, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DataTable, type ColumnDef } from '../src/components';
import { useRowAnimation } from '../src/react';

interface Item { id: string; value: number | string }
const rowKey = (row: Item) => row.id;
const getValue = (row: Item) => row.value;
const initial: readonly Item[] = [{ id: 'a', value: 10 }, { id: 'b', value: 20 }];

function Note({ id }: { id: string }) {
  const [text, setText] = useState('');
  return <input aria-label={`Note ${id}`} value={text} onChange={event => setText(event.target.value)} />;
}

const columns: readonly ColumnDef<Item>[] = [
  { key: 'id', label: 'Item' }, { key: 'value', label: 'Value' },
  { key: 'note', label: 'Note', sortable: false, format: (_value, row) => <Note id={row.id} /> },
];

function Tables({ rows, shared }: { rows: readonly Item[]; shared: boolean }) {
  const { animations, revisions } = useRowAnimation({ rowKey, getValue, data: shared ? rows : undefined });
  return <>{(shared ? ['Primary', 'Mirror'] : ['Primary']).map(title => (
    <DataTable
      key={title} title={title} rows={rows} columns={columns} rowKey={rowKey} animateOnChange="value"
      {...(shared ? { rowAnimations: animations, rowAnimationRevisions: revisions } : {})}
    />
  ))}</>;
}

function ManualTable({ direction }: { direction: 'up' | 'down' }) {
  const [rows, setRows] = useState(initial);
  const { animations, revisions, updateData } = useRowAnimation({ rowKey, getValue, data: initial });
  return <>
    <button type="button" onClick={() => {
      const next = rows.map(row => row.id === 'a'
        ? { ...row, value: Number(row.value) + (direction === 'up' ? 1 : -1) } : row);
      updateData(next);
      setRows(next);
    }}>Change reading</button>
    <DataTable
      title="Manual" rows={rows} rowKey={rowKey} columns={columns}
      rowAnimations={animations} rowAnimationRevisions={revisions}
    />
  </>;
}

afterEach(() => { vi.useRealTimers(); });

for (const coalesced of [true, false]) {
  it.each(['up', 'down'] as const)(`restarts %s across expiry/reactivation (coalesced=${coalesced})`, direction => {
    vi.useFakeTimers();
    const table = render(<StrictMode><ManualTable direction={direction} /></StrictMode>);
    const row = screen.getAllByRole('row')[1];
    const input = within(row).getByRole('textbox', { name: 'Note a' });
    fireEvent.change(input, { target: { value: 'keep across expiry' } });
    input.focus();
    act(() => vi.advanceTimersByTime(0));
    fireEvent.click(screen.getByRole('button', { name: 'Change reading' }));
    const previousClass = row.className;
    expect(previousClass).toContain(`drasi-row--${direction}`);
    if (coalesced) {
      act(() => {
        vi.advanceTimersByTime(500);
        fireEvent.click(screen.getByRole('button', { name: 'Change reading' }));
      });
    } else {
      act(() => vi.advanceTimersByTime(500));
      expect(row.className).not.toContain('drasi-row--');
      fireEvent.click(screen.getByRole('button', { name: 'Change reading' }));
    }
    expect(screen.getAllByRole('row')[1]).toBe(row);
    expect(within(row).getByRole('cell', { name: direction === 'up' ? '12' : '8' })).not.toBeNull();
    expect(row.className).toContain(`drasi-row--${direction}`);
    expect(row.className).not.toBe(previousClass);
    expect(within(row).getByRole('textbox', { name: 'Note a' })).toBe(input);
    expect(input).toHaveProperty('value', 'keep across expiry');
    expect(document.activeElement).toBe(input);
    expect(vi.getTimerCount()).toBe(1);
    act(() => vi.advanceTimersByTime(500));
    expect(row.className).not.toContain('drasi-row--');
    table.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
}

for (const shared of [false, true]) {
  it.each(['up', 'down'] as const)(`automatically reactivates %s after batched expiry (shared=${shared})`, direction => {
    vi.useFakeTimers();
    const table = render(<StrictMode><Tables rows={initial} shared={shared} /></StrictMode>);
    const primary = within(screen.getByRole('table', { name: 'Primary' }));
    const row = primary.getAllByRole('row')[1];
    const input = within(row).getByRole('textbox', { name: 'Note a' });
    fireEvent.change(input, { target: { value: 'keep automatic state' } });
    input.focus();
    act(() => vi.advanceTimersByTime(0));
    const value = direction === 'up' ? 11 : 9;
    table.rerender(<StrictMode><Tables rows={[{ id: 'a', value }, initial[1]]} shared={shared} /></StrictMode>);
    const previousClass = row.className;
    act(() => {
      vi.advanceTimersByTime(500);
      table.rerender(<StrictMode><Tables rows={[{ id: 'a', value: direction === 'up' ? 12 : 8 }, initial[1]]} shared={shared} /></StrictMode>);
    });
    expect(primary.getAllByRole('row')[1]).toBe(row);
    expect(row.className).toContain(`drasi-row--${direction}`);
    expect(row.className).not.toBe(previousClass);
    expect(input).toHaveProperty('value', 'keep automatic state');
    expect(document.activeElement).toBe(input);
    if (shared) expect(within(screen.getByRole('table', { name: 'Mirror' })).getAllByRole('row')[1].className).toBe(row.className);
    act(() => vi.advanceTimersByTime(500));
    expect(row.className).not.toContain('drasi-row--');
    expect(vi.getTimerCount()).toBe(0);
    table.unmount();
  });
}

for (const shared of [false, true]) {
  it.each(['up', 'down', 'change'] as const)(`restarts repeated %s decoration with stable rows and child state (shared=${shared})`, direction => {
    vi.useFakeTimers();
    const table = render(<StrictMode><Tables rows={initial} shared={shared} /></StrictMode>);
    const primary = within(screen.getByRole('table', { name: 'Primary' }));
    const row = primary.getAllByRole('row')[1];
    const other = primary.getAllByRole('row')[2];
    const input = primary.getByRole('textbox', { name: 'Note a' });
    fireEvent.change(input, { target: { value: 'keep this draft' } });
    input.focus();
    act(() => vi.advanceTimersByTime(0));
    let previousClass = row.className;
    for (let index = 1; index <= 5; index++) {
      const value = direction === 'change' ? `state ${index}` : 10 + (direction === 'up' ? index : -index);
      table.rerender(<StrictMode><Tables rows={[{ id: 'a', value }, initial[1]]} shared={shared} /></StrictMode>);
      expect(primary.getAllByRole('row')[1]).toBe(row);
      expect(primary.getByRole('textbox', { name: 'Note a' })).toBe(input);
      expect(input).toHaveProperty('value', 'keep this draft');
      expect(document.activeElement).toBe(input);
      expect(row.className).toContain(`drasi-row--${direction}`);
      expect(row.className).not.toBe(previousClass);
      previousClass = row.className;
      expect(other.className).not.toContain('drasi-row--');
      if (shared) expect(within(screen.getByRole('table', { name: 'Mirror' })).getAllByRole('row')[1].className).toBe(row.className);
      expect(vi.getTimerCount()).toBe(1);
      if (index < 5) act(() => vi.advanceTimersByTime(200));
    }
    act(() => vi.advanceTimersByTime(499));
    expect(row.className).toContain(`drasi-row--${direction}`);
    act(() => vi.advanceTimersByTime(1));
    expect(row.className).not.toContain('drasi-row--');
    expect(vi.getTimerCount()).toBe(0);
    expect(document.activeElement).toBe(input);
    table.unmount();
  });
}

it('revises only relevant rows, retains unchanged identities and clears revisions with timers', () => {
  vi.useFakeTimers();
  const hook = renderHook(() => useRowAnimation({ rowKey, getValue }), { wrapper: StrictMode });
  act(() => hook.result.current.updateData(initial));
  expect(hook.result.current.revisions.size).toBe(0);
  act(() => hook.result.current.updateData([{ id: 'a', value: 11 }, initial[1]]));
  const firstA = hook.result.current.revisions.get('a');
  expect(typeof firstA).toBe('number');
  expect([...hook.result.current.revisions.keys()]).toEqual(['a']);
  act(() => vi.advanceTimersByTime(200));
  act(() => hook.result.current.updateData([{ id: 'a', value: 12 }, { id: 'b', value: 19 }]));
  expect(hook.result.current.revisions.get('a')).not.toBe(firstA);
  const firstB = hook.result.current.revisions.get('b');
  expect(typeof firstB).toBe('number');
  expect([...hook.result.current.revisions.keys()]).toEqual(['a', 'b']);
  const { animations, revisions } = hook.result.current;
  act(() => hook.result.current.updateData([{ id: 'b', value: 19 }, { id: 'a', value: 12 }]));
  expect(hook.result.current.animations).toBe(animations);
  expect(hook.result.current.revisions).toBe(revisions);
  act(() => hook.result.current.updateData([{ id: 'b', value: 19 }]));
  expect([...hook.result.current.revisions]).toEqual([['b', firstB]]);
  expect(vi.getTimerCount()).toBe(1);
  act(() => vi.advanceTimersByTime(500));
  expect(hook.result.current.revisions.size).toBe(0);
  act(() => hook.result.current.updateData([{ id: 'b', value: 18 }]));
  expect([...hook.result.current.revisions.keys()]).toEqual(['b']);
  expect(hook.result.current.revisions.get('b')).not.toBe(firstB);
  act(() => hook.result.current.updateData([]));
  expect(hook.result.current.revisions.size).toBe(0);
  expect(vi.getTimerCount()).toBe(0);
  act(() => hook.result.current.updateData(initial));
  act(() => hook.result.current.updateData([{ id: 'a', value: 11 }, initial[1]]));
  hook.unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it('cleans expired/deleted row entries without recycling the last decoration token', () => {
  vi.useFakeTimers();
  const hook = renderHook(() => useRowAnimation({ rowKey, getValue }), { wrapper: StrictMode });
  let previousToken: number | undefined;
  for (let index = 0; index < 8; index++) {
    const id = `row-${index}`;
    act(() => hook.result.current.updateData([{ id, value: 10 }]));
    act(() => hook.result.current.updateData([{ id, value: 11 }]));
    const token = hook.result.current.revisions.get(id);
    expect(typeof token).toBe('number');
    expect(token).not.toBe(previousToken);
    expect(hook.result.current.animations.size).toBe(1);
    expect(hook.result.current.revisions.size).toBe(1);
    expect(Object.keys(hook.result.current).sort()).toEqual(['animations', 'revisions', 'updateData']);
    act(() => {
      if (index % 2 === 0) vi.advanceTimersByTime(500);
      else hook.result.current.updateData([]);
    });
    expect(hook.result.current.animations.size).toBe(0);
    expect(hook.result.current.revisions.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
    previousToken = token;
  }
  hook.unmount();
});

it('restarts for skipped controlled revisions rather than relying on token parity', () => {
  const animation = new Map([['a', 'up' as const]]);
  const view = (revision: number) => (
    <StrictMode><DataTable
      rows={initial} rowKey={rowKey} columns={columns}
      rowAnimations={animation} rowAnimationRevisions={new Map([['a', revision]])}
    /></StrictMode>
  );
  const table = render(view(0));
  const row = screen.getAllByRole('row')[1];
  let previousClass = row.className;
  for (const revision of [2, 4, NaN]) {
    table.rerender(view(revision));
    expect(screen.getAllByRole('row')[1]).toBe(row);
    expect(row.className).not.toBe(previousClass);
    previousClass = row.className;
  }
  table.rerender(view(NaN));
  expect(row.className).toBe(previousClass);
  table.unmount();
});
