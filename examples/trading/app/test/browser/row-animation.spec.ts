// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { test, expect, type Page } from '@playwright/test';
import { audit } from './axeAudit';
import { assertRowPulse, type NativeRowAnimationFacts } from './rowAnimationFacts';

async function open(page: Page, owner: string, scenario = 'row-animation', direction?: 'up' | 'down') {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const query = new URLSearchParams({ case: scenario, owner, ...(direction ? { direction } : {}) });
  await page.goto(`/__components/?${query}`);
  await expect(page.getByTestId('strict-setups')).toHaveText('2');
  return errors;
}

for (const direction of ['up', 'down'] as const) {
  test(`inactive/reactivated ${direction} commits before paint restart the native animation`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    const errors = await open(page, 'controlled', 'row-boundary', direction);
    const row = page.getByRole('table', { name: 'Boundary', exact: true }).locator('tbody tr');
    const input = row.getByRole('textbox', { name: 'Note a' });
    await input.fill('keep this boundary draft');
    await input.focus();
    await page.getByRole('button', { name: 'Activate boundary', exact: true }).dispatchEvent('click');
    await expect(row.getByRole('cell').nth(1)).toHaveText(direction === 'up' ? '11' : '9');
    const original = await row.evaluateHandle(element => {
      const animation = element.getAnimations()[0];
      if (!animation) throw new Error('Expected the initial native row animation');
      return { row: element, input: element.querySelector('input'), animation };
    });
    try {
      // Both React commits run in one event, with no style/layout read between them.
      await page.getByRole('button', { name: 'Reactivate before paint', exact: true }).dispatchEvent('click');
      const stages: string[] = JSON.parse(await page.getByTestId('boundary-stages').innerText());
      expect(stages).toHaveLength(2);
      expect(stages[0]).not.toContain('drasi-row--');
      expect(stages[1]).toContain(`drasi-row--${direction}`);
      await expect(row.getByRole('cell').nth(1)).toHaveText(direction === 'up' ? '12' : '8');
      expect(await row.evaluate((element, prior) => {
        const animations = element.getAnimations();
        return {
          active: animations.length,
          restarted: !!animations[0] && animations[0] !== prior.animation,
          sameRow: element === prior.row,
          sameInput: element.querySelector('input') === prior.input,
        };
      }, original)).toEqual({ active: 1, restarted: true, sameRow: true, sameInput: true });
      await expect(row).toHaveCSS('animation-duration', '0.5s');
      await expect(row).toHaveCSS('animation-timing-function', 'ease-in-out');
      await expect(input).toBeFocused();
      await expect(input).toHaveValue('keep this boundary draft');
      await page.getByRole('button', { name: 'Clear boundary', exact: true }).dispatchEvent('click');
      await expect(row).not.toHaveClass(/drasi-row--/);
      expect(await row.evaluate(element => element.getAnimations().length)).toBe(0);
      expect(errors).toEqual([]);
    } finally {
      await original.dispose();
    }
  });
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
      const mirror = owner === 'shared'
        ? await page.getByRole('table', { name: 'Mirror', exact: true }).locator('tbody tr').first().elementHandle()
        : null;
      if (owner === 'shared') expect(mirror).not.toBeNull();
      const observer = await row.evaluateHandle((element, mirrorElement) => {
        const observe = (element: Element) => {
          if (!(element instanceof HTMLTableRowElement)) throw new Error('Expected an animated table row');
          const originalInput = element.querySelector('input');
          const originalTable = element.closest('table');
          if (!originalInput || !originalTable) throw new Error('Expected the initial row input and table');
          let previous: Animation | undefined;
          const starts: string[] = [];
          const onStart = (event: AnimationEvent) => {
            if (event.target === element) starts.push(event.animationName);
          };
          element.addEventListener('animationstart', onStart);
          return {
            starts,
            animations: () => element.getAnimations(),
            sample(animations: Animation[], focused: Element | null) {
              const pulses = animations.filter(animation => animation instanceof CSSAnimation);
              const nativeObjects: NativeRowAnimationFacts[] = animations.map(animation => {
                const timing = animation.effect?.getTiming();
                return {
                  type: animation.constructor.name,
                  animationName: animation instanceof CSSAnimation ? animation.animationName : null,
                  transitionProperty: animation instanceof CSSTransition ? animation.transitionProperty : null,
                  targetIsRow: animation.effect instanceof KeyframeEffect && animation.effect.target === element,
                  duration: typeof timing?.duration === 'object' ? timing.duration.toString() : timing?.duration,
                  timingEasing: timing?.easing,
                  keyframeEasings: animation.effect instanceof KeyframeEffect
                    ? animation.effect.getKeyframes().map(frame => frame.easing) : [],
                  currentTimeMs: typeof animation.currentTime === 'number' ? animation.currentTime : null,
                  startTimeMs: typeof animation.startTime === 'number' ? animation.startTime : null,
                  playState: animation.playState,
                };
              });
              const animation = pulses[0];
              const currentTime = animation?.currentTime;
              const startTime = animation?.startTime;
              const result = {
                nativeObjects,
                active: pulses.length,
                replaced: !!animation && animation !== previous,
                duration: animation?.effect?.getTiming().duration,
                easing: getComputedStyle(element).animationTimingFunction,
                sameRow: originalTable.querySelector('tbody tr') === element,
                sameInput: originalInput === element.querySelector('input'),
                connected: element.isConnected,
                value: element.cells[1]?.textContent,
                inputValue: originalInput.value,
                focused: focused === originalInput,
                currentTimeMs: typeof currentTime === 'number' ? currentTime : null,
                startTimeMs: typeof startTime === 'number' ? startTime : null,
              };
              previous = animation;
              return result;
            },
            dispose() { element.removeEventListener('animationstart', onStart); },
          };
        };
        const primary = observe(element);
        const mirror = mirrorElement ? observe(mirrorElement) : null;
        return {
          starts: primary.starts,
          mirrorStarts: mirror?.starts,
          sample() {
            const sampledAtMs = performance.now();
            const primaryAnimations = primary.animations();
            const mirrorAnimations = mirror?.animations();
            const focused = document.activeElement;
            return {
              sampledAtMs,
              primary: primary.sample(primaryAnimations, focused),
              mirror: mirror && mirrorAnimations ? mirror.sample(mirrorAnimations, focused) : null,
              completedAtMs: performance.now(),
            };
          },
          dispose() { primary.dispose(); mirror?.dispose(); },
        };
      }, mirror);
      const samples = [];
      try {
        await page.getByRole('button', { name: 'Change b up', exact: true }).dispatchEvent('click');
        for (let index = 1; index <= 5; index++) {
          await page.getByRole('button', { name: `Change a ${direction}`, exact: true }).dispatchEvent('click');
          const value = direction === 'change' ? `10${'!'.repeat(index)}` : String(10 + (direction === 'up' ? index : -index));
          await expect(row.getByRole('cell').nth(1)).toHaveText(value);
          // Read both rows at the same paint boundary, before later RPCs can outlive the pulse.
          const sample = await observer.evaluate(state => new Promise<ReturnType<typeof state.sample>>(resolve =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve(state.sample())))));
          samples.push(sample);
          const animation = {
            active: 1, replaced: true, duration: 500, easing: 'ease-in-out',
            sameRow: true, sameInput: true, connected: true, value,
          };
          assertRowPulse(sample.primary.nativeObjects);
          expect(sample.primary).toMatchObject({ ...animation, inputValue: 'retain this draft', focused: true });
          if (owner === 'shared') {
            expect(sample.mirror).not.toBeNull();
            if (sample.mirror) assertRowPulse(sample.mirror.nativeObjects);
            expect(sample.mirror).toMatchObject({ ...animation, inputValue: '', focused: false });
          } else {
            expect(sample.mirror).toBeNull();
          }
          // Deliberate update cadence, not a readiness or animation-settlement wait.
          if (index < 5) await page.waitForTimeout(200);
        }
        expect(samples).toHaveLength(5);
        expect(await observer.evaluate(state => state.starts.length)).toBe(5);
        if (owner === 'shared') expect(await observer.evaluate(state => state.mirrorStarts?.length)).toBe(5);
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
        await mirror?.dispose();
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
