// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { expect, it, vi } from 'vitest';
import { installPausedClock } from '../browser/clock';

function fixture(ticks: number, url = 'about:blank') {
  return {
    url: () => url,
    goto: vi.fn(async (_url: string) => {}),
    evaluate: vi.fn(async (_callback: () => number) => ticks),
    clock: {
      install: vi.fn(async (_options: { time?: Date | number | string }) => {}),
      pauseAt: vi.fn(async (_anchor: Date | number | string) => {}),
      runFor: vi.fn(async (_ticks: number | string) => {}),
      setSystemTime: vi.fn(async (_anchor: Date | number | string) => {}),
    },
  };
}

it.each([0, 16, 29, 53, 103])('aligns replayed ticks %s only before application navigation and restores the exact anchor', async ticks => {
  const page = fixture(ticks);
  const anchor = new Date('2026-01-15T12:00:00Z');
  await installPausedClock(page, anchor);
  expect(page.clock.install).toHaveBeenCalledExactlyOnceWith({ time: new Date(anchor.getTime() - 60_000) });
  expect(page.clock.pauseAt).toHaveBeenCalledExactlyOnceWith(anchor);
  expect(page.goto).toHaveBeenCalledExactlyOnceWith('about:blank');
  expect(page.evaluate).toHaveBeenCalledExactlyOnceWith(expect.any(Function));
  if (ticks % 16) expect(page.clock.runFor).toHaveBeenCalledExactlyOnceWith(16 - ticks % 16);
  else expect(page.clock.runFor).not.toHaveBeenCalled();
  expect(page.clock.setSystemTime).toHaveBeenCalledExactlyOnceWith(anchor);
  const calls = [
    page.clock.install, page.clock.pauseAt, page.goto, page.evaluate,
    ...(ticks % 16 ? [page.clock.runFor] : []), page.clock.setSystemTime,
  ].map(method => method.mock.invocationCallOrder[0]);
  expect(calls).toEqual([...calls].sort((a, b) => a - b));
});

it.each([
  ['http://app.invalid/', new Date(0)],
  ['about:blank', new Date(NaN)],
] as const)('rejects nonblank pages or invalid anchors before clock work: %s', async (url, anchor) => {
  const page = fixture(0, url);
  await expect(installPausedClock(page, anchor)).rejects.toThrow('blank page and a valid anchor');
  expect(page.clock.install).not.toHaveBeenCalled();
  expect(page.goto).not.toHaveBeenCalled();
});

it.each([NaN, Infinity, -1])('rejects invalid replayed monotonic ticks: %s', async ticks => {
  const page = fixture(ticks);
  await expect(installPausedClock(page, new Date(0))).rejects.toThrow('Invalid replayed clock ticks');
  expect(page.clock.runFor).not.toHaveBeenCalled();
  expect(page.clock.setSystemTime).not.toHaveBeenCalled();
});
