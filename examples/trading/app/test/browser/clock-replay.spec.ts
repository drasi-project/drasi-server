// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { test, expect } from '@playwright/test';
import { installPausedClock } from './clock';
import { FIXED_TIME } from '../fixtures/synthetic/trading';

declare global {
  interface Window {
    clockProbe: { date: number; ticks: number; frames: number[] };
  }
}

for (const gap of [0, 25, 50, 100]) {
  test(`clock navigation replay retains anchor and 312 frames after a ${gap}ms setup gap`, async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', route => route.fulfill({
      status: 200,
      contentType: 'text/html',
      body: `<!doctype html><html><body><script>
        window.clockProbe = { date: Date.now(), ticks: performance.now(), frames: [] };
        function frame(now) {
          window.clockProbe.frames.push(now);
          requestAnimationFrame(frame);
        }
        requestAnimationFrame(frame);
      </script></body></html>`,
    }));
    const anchor = new Date(FIXED_TIME);
    await installPausedClock({
      url: () => page.url(),
      goto: url => page.goto(url),
      evaluate: callback => page.evaluate(callback),
      clock: {
        install: async options => {
          await page.clock.install(options);
          if (gap) await new Promise(resolve => setTimeout(resolve, gap));
        },
        pauseAt: value => page.clock.pauseAt(value),
        runFor: value => page.clock.runFor(value),
        setSystemTime: value => page.clock.setSystemTime(value),
      },
    }, anchor);
    const before = await page.evaluate(() => ({ date: Date.now(), ticks: performance.now() }));
    expect(page.url()).toBe('about:blank');
    expect(before.date).toBe(anchor.getTime());
    expect(before.ticks % 16).toBe(0);
    await page.goto('http://clock-probe.invalid/');
    const after = await page.evaluate(() => window.clockProbe);
    expect(after).toEqual({ ...before, frames: [] });
    await page.clock.runFor(5000);
    const final = await page.evaluate(() => ({
      date: Date.now(), ticks: performance.now(),
      count: window.clockProbe.frames.length,
      first: window.clockProbe.frames[0], last: window.clockProbe.frames.at(-1),
    }));
    expect(final).toEqual({
      date: anchor.getTime() + 5000, ticks: before.ticks + 5000,
      count: 312, first: before.ticks + 16, last: before.ticks + 4992,
    });
    expect(errors).toEqual([]);
    await testInfo.attach('clock-navigation-frame-phase.json', {
      body: JSON.stringify({ gap, before, after, final }, null, 2), contentType: 'application/json',
    });
  });
}
