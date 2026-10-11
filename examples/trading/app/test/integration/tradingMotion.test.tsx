// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, fireEvent, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { panel, renderTrading } from './renderTrading';

function motionPreference(initial: boolean) {
  let reduced = initial;
  const listeners = new Set<() => void>();
  vi.stubGlobal('matchMedia', vi.fn((query: string) => ({
    get matches() { return query.includes('reduced-motion') && reduced; },
    addEventListener: (_type: string, change: () => void) => listeners.add(change),
    removeEventListener: (_type: string, change: () => void) => listeners.delete(change),
  })));
  return {
    listeners,
    set(value: boolean) { act(() => { reduced = value; listeners.forEach(change => change()); }); },
  };
}

afterEach(() => vi.useRealTimers());

it('expands, applies live rows and collapses immediately under reduced motion', async () => {
  motionPreference(true);
  const app = await renderTrading();
  const normal = panel('Watchlist');
  const timer = vi.spyOn(globalThis, 'setTimeout');
  fireEvent.click(within(normal).getByRole('button', { name: 'Expand table' }));
  const modal = await screen.findByRole('dialog', { name: 'Watchlist' });
  expect(modal.style.top).toBe('32px');
  expect(modal.style.width).toBe('calc(100vw - 64px)');
  expect(modal.style.transition).toBe('none');
  act(() => app.backend.changePrice('AAPL', 111));
  const row = within(modal).getByRole('row', { name: /^AAPL / });
  expect(row.textContent).toContain('$111.00');
  expect(row.className).not.toMatch(/drasi-row--(?:up|down|change)/);
  expect(normal.querySelector('tbody')?.textContent).toContain('$111.00');
  fireEvent.click(within(modal).getByRole('button', { name: 'Collapse table' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
  expect(timer.mock.calls.filter(([, milliseconds]) => milliseconds === 350 || milliseconds === 500)).toEqual([]);
  app.unmount();
});

it('cancels scheduled expansion frames on a preference change instead of stranding the layout', async () => {
  const preference = motionPreference(false);
  const frames = vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(17);
  const cancel = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
  const app = await renderTrading();
  fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'Expand table' }));
  await screen.findByRole('dialog', { name: 'Watchlist' });
  expect(frames).toHaveBeenCalled();
  preference.set(true);
  const modal = screen.getByRole('dialog', { name: 'Watchlist' });
  expect(modal.style.top).toBe('32px');
  expect(modal.style.transition).toBe('none');
  expect(cancel).toHaveBeenCalledWith(17);
  fireEvent.click(within(modal).getByRole('button', { name: 'Collapse table' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  app.unmount();
  expect(preference.listeners.size).toBe(0);
});

it('finishes a pending animated collapse when reduced motion is enabled and releases ownership', async () => {
  const preference = motionPreference(false);
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(19);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
  const app = await renderTrading();
  const cancelTimer = vi.spyOn(globalThis, 'clearTimeout');
  const timers = vi.spyOn(globalThis, 'setTimeout');
  fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'Expand table' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Collapse table' }));
  const index = timers.mock.calls.findIndex(([, duration]) => duration === 350);
  expect(index).toBeGreaterThanOrEqual(0);
  expect(screen.getByRole('dialog')).not.toBeNull();
  preference.set(true);
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(cancelTimer).toHaveBeenCalledWith(timers.mock.results[index].value);
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
  app.unmount();
});

it('cleans a pending FLIP close on unmount without stale locks or preference listeners', async () => {
  const preference = motionPreference(false);
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(23);
  const cancelFrame = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
  const app = await renderTrading();
  fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'Expand table' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Collapse table' }));
  app.unmount();
  expect(cancelFrame).toHaveBeenCalledWith(23);
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
  expect(document.body.style.pointerEvents).toBe('');
  expect(preference.listeners.size).toBe(0);
});
