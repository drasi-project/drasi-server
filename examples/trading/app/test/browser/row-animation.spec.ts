// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { test, expect, type Page } from '@playwright/test';
import { audit } from './axeAudit';

async function open(page: Page, owner: string) {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`/__components/?case=row-animation&owner=${owner}`);
  await expect(page.getByTestId('strict-setups')).toHaveText('2');
  return errors;
}

for (const owner of ['local', 'shared']) {
  for (const direction of ['up', 'down', 'change'] as const) {
    test(`repeated ${direction} restarts native CSS without replacing rows (${owner})`, async ({ page }, testInfo) => {
      await page.emulateMedia({ reducedMotion: 'no-preference' });
      const errors = await open(page, owner);
      const table = page.getByRole('table', { name: 'Primary', exact: true });
      const row = table.locator('tbody tr').first();
      const input = row.getByRole('textbox', { name: 'Note a' });
      await input.fill('retain this draft');
      await input.focus();
      const observer = await row.evaluateHandle(element => {
        if (!(element instanceof HTMLTableRowElement)) throw new Error('Expected an animated table row');
        const originalInput = element.querySelector('input');
        let previous: Animation | undefined;
        const starts: string[] = [];
        const onStart = (event: AnimationEvent) => {
          if (event.target === element) starts.push(event.animationName);
        };
        element.addEventListener('animationstart', onStart);
        return {
          starts,
          sample() {
            const animations = element.getAnimations();
            const animation = animations[0];
            const result = {
              active: animations.length,
              replaced: !!animation && animation !== previous,
              duration: animation?.effect?.getTiming().duration,
              easing: getComputedStyle(element).animationTimingFunction,
              sameInput: originalInput === element.querySelector('input'),
              connected: element.isConnected,
            };
            previous = animation;
            return result;
          },
          dispose() { element.removeEventListener('animationstart', onStart); },
        };
      });
      const samples = [];
      try {
        await page.getByRole('button', { name: 'Change b up', exact: true }).dispatchEvent('click');
        for (let index = 1; index <= 5; index++) {
          await page.getByRole('button', { name: `Change a ${direction}`, exact: true }).dispatchEvent('click');
          const value = direction === 'change' ? `10${'!'.repeat(index)}` : String(10 + (direction === 'up' ? index : -index));
          await expect(row.getByRole('cell').nth(1)).toHaveText(value);
          await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
          samples.push(await observer.evaluate(state => state.sample()));
          await expect(input).toBeFocused();
          await expect(input).toHaveValue('retain this draft');
          if (owner === 'shared') {
            const mirror = page.getByRole('table', { name: 'Mirror', exact: true }).locator('tbody tr').first();
            await expect(mirror.getByRole('cell').nth(1)).toHaveText(value);
            expect(await mirror.evaluate(element => element.getAnimations().length)).toBe(1);
          }
          // Deliberate update cadence, not a readiness or animation-settlement wait.
          if (index < 5) await page.waitForTimeout(200);
        }
        expect(samples).toEqual(Array.from({ length: 5 }, () => ({
          active: 1, replaced: true, duration: 500, easing: 'ease-in-out', sameInput: true, connected: true,
        })));
        expect(await observer.evaluate(state => state.starts.length)).toBe(5);
        await expect(table.locator('tbody tr').nth(1)).not.toHaveClass(/drasi-row--/);
        await expect(row).not.toHaveClass(/drasi-row--/);
        expect(await row.evaluate(element => element.getAnimations().length)).toBe(0);
        await expect(input).toBeFocused();
        await expect(input).toHaveValue('retain this draft');
        await audit(page, testInfo, `row-restart-${direction}-${owner}`, false);
        expect(errors).toEqual([]);
      } finally {
        await testInfo.attach('native-row-animation-samples.json', {
          body: JSON.stringify(samples, null, 2), contentType: 'application/json',
        });
        await observer.evaluate(state => state.dispose());
        await observer.dispose();
      }
    });
  }

  test(`reduced motion cancels repeated decoration while data, removal and unmount stay live (${owner})`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    const errors = await open(page, owner);
    const row = page.getByRole('table', { name: 'Primary', exact: true }).locator('tbody tr').first();
    for (let index = 1; index <= 5; index++) {
      await page.getByRole('button', { name: 'Change a up', exact: true }).click();
      await expect(row.getByRole('cell').nth(1)).toHaveText(String(10 + index));
      await expect(row).not.toHaveClass(/drasi-row--/);
      expect(await row.evaluate(element => element.getAnimations().length)).toBe(0);
    }
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.getByRole('button', { name: 'Change a down', exact: true }).click();
    await expect(row).toHaveClass(/drasi-row--down/);
    expect(await row.evaluate(element => element.getAnimations().length)).toBe(1);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await expect(row).not.toHaveClass(/drasi-row--/);
    expect(await row.evaluate(element => element.getAnimations().length)).toBe(0);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.getByRole('button', { name: 'Change a up', exact: true }).click();
    await page.getByRole('button', { name: 'Remove a', exact: true }).click();
    await expect(page.getByRole('textbox', { name: 'Note a' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Change b up', exact: true }).click();
    await page.getByRole('button', { name: 'Unmount restart tables', exact: true }).click();
    await expect(page.getByRole('table')).toHaveCount(0);
    expect(errors).toEqual([]);
  });
}
