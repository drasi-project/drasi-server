// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { test, expect, openTrading } from './fixtures';
import { audit, settleTradingAudit } from './axeAudit';

// These two phases have exact predecessor evidence in WebKit only. Other
// engines run the full stable-state Trading audits, not empty/expected-fail tests.
for (const [title, trigger, submit, state, phase] of [
  ['Add Position', 'Add position', 'Add', 'add-position', 109.286],
  ['New Limit Order', 'New limit order', 'Create Order', 'new-limit-order', 42.595],
] as const) {
  test(`exact predecessor hover and transition contrast boundary: ${state}`, async ({ page, browserName }, testInfo) => {
    expect(browserName).toBe('webkit');
    await page.clock.resume();
    await openTrading(page);
    await page.getByTitle(trigger, { exact: true }).click();
    const dialog = page.getByRole('dialog', { name: title, exact: true });
    await expect(dialog).toBeFocused();
    const button = dialog.getByRole('button', { name: submit, exact: true });
    await settleTradingAudit(page, dialog);
    await button.focus();
    await button.hover();
    await expect.poll(() => button.evaluate(element => element.getAnimations()
      .filter(animation => animation.playState === 'running').length)).toBe(0);
    await audit(page, testInfo, `trading-${state}-hover-settled`, false, dialog, {
      state: `${state}-hover-settled`, browser: browserName,
    });
    await settleTradingAudit(page, dialog);
    await button.hover();
    const timing = await button.evaluate((element, phase) => {
      const animation = element.getAnimations().find(candidate =>
        candidate instanceof CSSTransition && candidate.transitionProperty === 'background-color');
      if (!(animation?.effect instanceof KeyframeEffect)) throw new Error('Expected the preserved native background-color transition');
      animation.pause();
      animation.currentTime = phase;
      return { currentTime: animation.currentTime, timing: animation.effect.getTiming(), keyframes: animation.effect.getKeyframes() };
    }, phase);
    await testInfo.attach(`${state}-matched-transition.json`, {
      body: JSON.stringify(timing, null, 2), contentType: 'application/json',
    });
    await audit(page, testInfo, `trading-${state}-hover-ci-sample`, false, dialog, {
      state: `${state}-hover-ci-sample`, browser: browserName,
    });
  });
}
