// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, render, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CodeIcon, CollapseIcon, ExpandIcon } from '../src/components';
import { usePortalTheme } from '../src/components/usePortalTheme';

afterEach(() => vi.unstubAllGlobals());

it('copies explicit and custom tokens, typography and direction and follows source mutations', async () => {
  const view = render(<section><div data-testid="source" /></section>);
  const source = view.getByTestId('source');
  source.style.setProperty('--drasi-color-surface', 'rgb(240, 250, 255)');
  source.style.setProperty('--drasi-inventory-accent', 'purple');
  source.style.fontFamily = 'monospace';
  source.style.fontSize = '18px';
  source.style.fontWeight = '500';
  source.style.lineHeight = '1.6';
  source.style.direction = 'rtl';
  const ref = { current: source };
  const theme = renderHook(() => usePortalTheme(true, ref));
  expect(theme.result.current).toMatchObject({
    '--drasi-color-surface': 'rgb(240, 250, 255)',
    '--drasi-inventory-accent': 'purple',
    '--drasi-color-border': 'initial',
    fontFamily: 'monospace', fontSize: '18px', fontWeight: '500', lineHeight: '1.6', direction: 'rtl',
  });
  source.style.setProperty('--drasi-color-surface', 'pink');
  await waitFor(() => expect(theme.result.current).toHaveProperty('--drasi-color-surface', 'pink'));
  const unchanged = theme.result.current;
  act(() => window.dispatchEvent(new Event('resize')));
  expect(theme.result.current).toBe(unchanged);
  theme.unmount();
});

it('subscribes to color-scheme changes only while open and releases all observers/listeners', () => {
  const changes = new Set<() => void>();
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    addEventListener: (_type: string, change: () => void) => changes.add(change),
    removeEventListener: (_type: string, change: () => void) => changes.delete(change),
  })));
  const view = render(<div data-testid="source" />);
  const ref = { current: view.getByTestId('source') };
  const styles = vi.spyOn(window, 'getComputedStyle');
  const hook = renderHook(({ open }) => usePortalTheme(open, ref), { initialProps: { open: false } });
  expect(hook.result.current).toEqual({});
  expect(changes.size).toBe(0);
  hook.rerender({ open: true });
  expect(changes.size).toBe(1);
  const before = styles.mock.calls.length;
  act(() => changes.forEach(change => change()));
  expect(styles.mock.calls.length).toBeGreaterThan(before);
  hook.rerender({ open: false });
  expect(changes.size).toBe(0);
  const closed = styles.mock.calls.length;
  act(() => window.dispatchEvent(new Event('resize')));
  expect(styles.mock.calls.length).toBe(closed);
  hook.unmount();
});

it('does not access an absent theme source or a document without a window', () => {
  const missing = { current: null };
  const hook = renderHook(() => usePortalTheme(true, missing));
  expect(hook.result.current).toEqual({});
  const detached = document.implementation.createHTMLDocument('Detached');
  const other = renderHook(() => usePortalTheme(true, { current: detached.body }));
  expect(other.result.current).toEqual({});
});

it.each([CodeIcon, ExpandIcon, CollapseIcon])('keeps reusable icons decorative under a labelled control', Icon => {
  const view = render(<><Icon /><Icon className="custom-icon" /></>);
  const icons = view.container.querySelectorAll('svg');
  expect(icons[0].getAttribute('class')).toBe('drasi-icon');
  expect(icons[1].getAttribute('class')).toBe('custom-icon');
  for (const icon of icons) {
    expect(icon.getAttribute('aria-hidden')).toBe('true');
    expect(icon.getAttribute('focusable')).toBe('false');
  }
});
