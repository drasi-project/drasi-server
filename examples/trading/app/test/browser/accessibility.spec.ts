// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { test as base, expect, type Locator, type Page, type TestInfo } from '@playwright/test';
import { test as trading, openTrading, panel, row } from './fixtures';
import { audit, auditReadiness, settleTradingAudit } from './axeAudit';

const test = base.extend({
  page: async ({ page, baseURL }, use, testInfo) => {
    const errors: string[] = [];
    const requests: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('request', request => requests.push(request.url()));
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      if (url.origin === new URL(baseURL!).origin) return route.continue();
      errors.push(`Blocked unexpected outbound request: ${url.origin}${url.pathname}`);
      return route.abort('blockedbyclient');
    });
    await use(page);
    await testInfo.attach('provider-free-network.json', {
      body: JSON.stringify(requests, null, 2), contentType: 'application/json',
    });
    expect(requests.every(url => !/\/(?:api\/|events(?:\?|$))/.test(url))).toBe(true);
    expect(errors).toEqual([]);
  },
});

async function openConsumer(page: Page, query = ''): Promise<void> {
  await page.goto(`/__components/${query ? `?${query}` : ''}`);
  // The isolated build uses React development artifacts: this proves actual replay.
  await expect(page.getByTestId('strict-setups')).toHaveText('2');
}

type AXNode = NonNullable<Awaited<ReturnType<Page['accessibility']['snapshot']>>>;

async function accessibilityEvidence(
  page: Page, testInfo: TestInfo, name: string, expected: { role: string; name?: string }[],
): Promise<void> {
  const aria = await page.locator('body').ariaSnapshot();
  await testInfo.attach(`${name}-dom-aria.yml`, { body: aria, contentType: 'text/yaml' });
  // Pinned Playwright 1.56.1 implements native AX snapshot protocols in all three engines.
  // locator.ariaSnapshot is separate DOM-derived evidence, not a platform AT result.
  const snapshot = await page.accessibility.snapshot({ interestingOnly: false });
  expect(snapshot).not.toBeNull();
  await testInfo.attach(`${name}-browser-ax.json`, {
    body: JSON.stringify(snapshot, null, 2), contentType: 'application/json',
  });
  const nodes: AXNode[] = [];
  const visit = (node: AXNode) => {
    nodes.push(node);
    node.children?.forEach(visit);
  };
  if (snapshot) visit(snapshot);
  for (const entry of expected) {
    expect(nodes.some(node => node.role === entry.role && (entry.name === undefined || node.name === entry.name)),
      `${name}: native AX contains ${entry.role}${entry.name === undefined ? '' : ` "${entry.name}"`}`).toBe(true);
  }
  if (testInfo.project.name === 'chromium') {
    const session = await page.context().newCDPSession(page);
    try {
      const fullTree = await session.send('Accessibility.getFullAXTree');
      await testInfo.attach(`${name}-chromium-full-ax.json`, {
        body: JSON.stringify(fullTree, null, 2), contentType: 'application/json',
      });
      for (const entry of expected) {
        expect(fullTree.nodes.some(node => !node.ignored &&
          node.role?.value === entry.role && (entry.name === undefined || node.name?.value === entry.name))).toBe(true);
      }
    } finally {
      await session.detach();
    }
  }
  testInfo.annotations.push({
    type: 'accessibility-evidence',
    description: 'Automated keyboard + DOM ARIA + browser-native AX; not human screen-reader or OS accessibility validation.',
  });
}

async function fullControlTab(page: Page, testInfo: TestInfo): Promise<string> {
  if (testInfo.project.name === 'webkit' && await page.evaluate(() => navigator.platform.startsWith('Mac'))) {
    testInfo.annotations.push({
      type: 'keyboard-platform-boundary',
      description: 'macOS WebKit uses native Option+Tab for ordinary buttons; unmodified Tab/Shift+Tab containment is also tested. No host accessibility settings changed.',
    });
    return 'Alt+Tab';
  }
  return 'Tab';
}

function modal(page: Page, owner = 'primary'): Locator {
  return page.locator('[data-drasi-modal-content]').filter({ has: page.getByTestId(`${owner}-contents`) });
}

async function observePaints(page: Page): Promise<void> {
  // Input follows committed passive effects, including Radix's deferred scope teardown.
  await page.evaluate(() => new Promise<void>(resolve => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  }));
}

async function expectNativeScrollSettled(scroller: Locator): Promise<void> {
  let lastTop = -1;
  let stableChecks = 0;
  await expect.poll(async () => {
    const top = await scroller.evaluate(element => element.scrollTop);
    stableChecks = top === lastTop ? stableChecks + 1 : 0;
    lastTop = top;
    return stableChecks;
  }, { intervals: [100] }).toBeGreaterThanOrEqual(2);
}

async function nativePageScroll(page: Page, scroller: Locator, key: 'PageUp' | 'PageDown') {
  const before = await scroller.evaluate(element => element.scrollTop);
  const completion = await scroller.evaluateHandle(element => {
    if (!('onscrollend' in element)) throw new Error('The pinned browser must expose native scroll completion');
    let completed = false;
    const ended = () => { completed = true; };
    element.addEventListener('scrollend', ended);
    return {
      get completed() { return completed; },
      dispose() { element.removeEventListener('scrollend', ended); },
    };
  });
  try {
    // Observe the native gesture's end, not a temporary compositor plateau.
    await page.keyboard.down(key);
    try {
      const position = expect.poll(() => scroller.evaluate(element => element.scrollTop));
      if (key === 'PageDown') await position.toBeGreaterThan(before);
      else await position.toBeLessThan(before);
    } finally {
      await page.keyboard.up(key);
    }
    await expect.poll(() => completion.evaluate(state => state.completed)).toBe(true);
    await expectNativeScrollSettled(scroller);
    return { before, after: await scroller.evaluate(element => element.scrollTop) };
  } finally {
    await completion.evaluate(state => state.dispose());
    await completion.dispose();
  }
}

async function expectModalReady(page: Page, dialog: Locator): Promise<void> {
  await expect(dialog).toHaveCSS('pointer-events', 'auto');
  await expect(page.locator('body')).toHaveAttribute(
    'data-scroll-locked', String(await page.locator('[data-drasi-modal-content]').count()),
  );
  await observePaints(page);
}

async function openOwner(page: Page, owner = 'primary'): Promise<void> {
  const trigger = page.getByRole('button', { name: `Open ${owner} dialog`, exact: true });
  await trigger.focus();
  await trigger.press('Enter');
  await expect(modal(page, owner)).toBeFocused();
  await expectModalReady(page, modal(page, owner));
}

async function focusIsWithin(dialog: Locator): Promise<void> {
  await expect.poll(() => dialog.evaluate(element => element.contains(document.activeElement))).toBe(true);
}

async function bodyInline(page: Page) {
  return page.locator('body').evaluate(element => ({
    overflow: element.style.overflow,
    paddingRight: element.style.paddingRight,
    scrollPadding: element.style.scrollPadding,
    margin: element.style.margin,
    pointerEvents: element.style.pointerEvents,
  }));
}

async function expectLocked(page: Page): Promise<void> {
  await expect(page.locator('body')).toHaveAttribute('data-scroll-locked', /^[1-9]\d*$/);
  await expect(page.locator('body')).toHaveCSS('overflow', 'hidden');
}

async function expectReleased(page: Page, original: Awaited<ReturnType<typeof bodyInline>>): Promise<void> {
  await expect(page.locator('[data-drasi-modal-content], .drasi-modal-layer, [data-radix-focus-guard]')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
  await expect(page.locator('body')).toHaveCSS('overflow', original.overflow);
  expect(await bodyInline(page)).toEqual(original);
  await expect(page.locator('#outside-control')).not.toHaveAttribute('aria-hidden');
  await observePaints(page);
  await page.locator('#outside-control').focus();
  await expect(page.locator('#outside-control')).toBeFocused();
}

async function deferModalChunk(page: Page) {
  const requests: string[] = [];
  let release: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/__components/assets/ModalLayer-*.js', async route => {
    requests.push(route.request().url());
    await gate;
    await route.fallback();
  });
  return {
    requests,
    release: () => release(),
    finish: async () => {
      const response = page.waitForResponse(value => requests.includes(value.url()));
      release();
      const received = await response;
      expect(received.ok()).toBe(true);
      await received.finished();
      await observePaints(page);
    },
  };
}

test.describe('installed provider-free consumer', () => {
  for (const action of ['Cancel pending open', 'Unmount pending owner']) {
    test(`slow Modal chunk: closed mounting warms up, then ${action.toLowerCase()} prevents late ownership`, async ({ page }, testInfo) => {
      const chunk = await deferModalChunk(page);
      try {
        await openConsumer(page, 'case=deferred-modal');
        const original = await bodyInline(page);
        expect(chunk.requests).toHaveLength(0);
        await page.getByRole('button', { name: 'Mount closed modal' }).press('Enter');
        await expect.poll(() => chunk.requests.length).toBe(1);
        await expect(page.getByRole('dialog')).toHaveCount(0);
        await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
        expect(await bodyInline(page)).toEqual(original);
        await page.getByRole('button', { name: 'Open deferred modal' }).press('Enter');
        await expect(page.getByRole('dialog')).toHaveCount(0);
        const cancel = page.getByRole('button', { name: action, exact: true });
        await cancel.press('Enter');
        await chunk.finish();
        await expect(cancel).toBeFocused();
        await expect(page.getByTestId('deferred-closes')).toHaveText('0');
        await expectReleased(page, original);
        await page.getByRole('button', { name: 'Open deferred modal' }).press('Enter');
        const dialog = page.getByRole('dialog', { name: 'Deferred dialog', exact: true });
        await expect(dialog).toBeFocused();
        await expectModalReady(page, dialog);
        await page.keyboard.press('Escape');
        await expect(page.getByRole('button', { name: 'Open deferred modal' })).toBeFocused();
        await expect(page.getByTestId('deferred-closes')).toHaveText('1');
        expect(chunk.requests).toHaveLength(1);
        await expectReleased(page, original);
        await testInfo.attach('deferred-modal-chunk.json', {
          body: JSON.stringify({ action, requests: chunk.requests }), contentType: 'application/json',
        });
      } finally {
        chunk.release();
      }
    });
  }

  test('slow Modal chunk: cold opening captures the trigger before the primitive loads', async ({ page }, testInfo) => {
    const chunk = await deferModalChunk(page);
    try {
      await openConsumer(page, 'case=deferred-modal');
      const original = await bodyInline(page);
      const trigger = page.getByRole('button', { name: 'Open deferred modal' });
      await trigger.press('Enter');
      await expect.poll(() => chunk.requests.length).toBe(1);
      await expect(page.getByRole('dialog')).toHaveCount(0);
      await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
      const background = page.getByRole('textbox', { name: 'Background note' });
      await background.fill('Editable before any modal ownership exists');
      await expect(background).toBeFocused();
      await chunk.finish();
      const dialog = page.getByRole('dialog', { name: 'Deferred dialog', exact: true });
      await expect(dialog).toBeFocused();
      await expectModalReady(page, dialog);
      await page.keyboard.press('Escape');
      await expect(trigger).toBeFocused();
      await expect(page.getByTestId('deferred-closes')).toHaveText('1');
      await expectReleased(page, original);
      await testInfo.attach('deferred-modal-chunk.json', {
        body: JSON.stringify({ requests: chunk.requests }), contentType: 'application/json',
      });
    } finally {
      chunk.release();
    }
  });

  test('native sorting buttons notify exactly once under real StrictMode, including controlled null', async ({ page }, testInfo) => {
    await openConsumer(page);
    for (const mode of ['controlled', 'uncontrolled'] as const) {
      const table = page.getByRole('table', {
        name: mode === 'controlled' ? 'Controlled inventory' : 'Uncontrolled inventory', exact: true,
      });
      const heading = table.getByRole('columnheader', { name: 'Name', exact: true });
      const button = heading.getByRole('button', { name: 'Name', exact: true });
      await expect(heading).toHaveAttribute('scope', 'col');
      await expect(heading).not.toHaveAttribute('tabindex');
      expect(await button.evaluate(element => element.tagName)).toBe('BUTTON');
      await expect(table.getByRole('columnheader', { name: 'Code', exact: true }).getByRole('button')).toHaveCount(0);
      await button.focus();
      await button.press('Enter');
      await expect(page.getByTestId(`${mode}-calls`)).toHaveText('1');
      await expect(heading).toHaveAttribute('aria-sort', 'ascending');
      await expect(table.locator('tbody tr td:first-child')).toHaveText(['Alpha', 'Bravo', 'Charlie']);
      await button.press('Space');
      await expect(page.getByTestId(`${mode}-calls`)).toHaveText('2');
      await expect(heading).toHaveAttribute('aria-sort', 'descending');
      await expect(table.locator('tbody tr td:first-child')).toHaveText(['Charlie', 'Bravo', 'Alpha']);
      await page.getByRole('button', { name: `Clear ${mode} sort`, exact: true }).click();
      await expect(page.getByTestId(`${mode}-calls`)).toHaveText('3');
      await expect(page.getByTestId(`${mode}-request`)).toHaveText('null');
      await expect(table.locator('[aria-sort]')).toHaveCount(0);
      await expect(table.locator('tbody tr td:first-child')).toHaveText(['Bravo', 'Alpha', 'Charlie']);
    }
    const controlled = page.getByRole('table', { name: 'Controlled inventory', exact: true });
    await page.getByRole('button', { name: 'Set sort from owner', exact: true }).click();
    await expect(controlled.getByRole('columnheader', { name: 'Quantity', exact: true })).toHaveAttribute('aria-sort', 'descending');
    await expect(page.getByTestId('controlled-calls')).toHaveText('3');
    await page.getByRole('button', { name: 'Clear sort from owner', exact: true }).click();
    await expect(controlled.locator('[aria-sort]')).toHaveCount(0);
    await page.getByRole('checkbox', { name: 'Apply controlled sort requests' }).uncheck();
    await controlled.getByRole('button', { name: 'Name', exact: true }).press('Enter');
    await expect(page.getByTestId('controlled-calls')).toHaveText('4');
    await expect(page.getByTestId('controlled-request')).toHaveText('{"column":"name","direction":"asc"}');
    await expect(controlled.locator('[aria-sort]')).toHaveCount(0);
    await expect(controlled.locator('tbody tr td:first-child')).toHaveText(['Bravo', 'Alpha', 'Charlie']);
    await audit(page, testInfo, 'pure-sorting');
    await accessibilityEvidence(page, testInfo, 'pure-sorting', [
      { role: 'table', name: 'Controlled inventory' },
      { role: 'button', name: 'Name' },
      { role: 'button', name: 'Quantity' },
      // WebKit's native AX represents the name on the child button, not its columnheader.
      { role: 'columnheader' },
    ]);
  });

  test('a non-sortable action-free table has a named native keyboard scroll stop and exit', async ({ page }, testInfo) => {
    await openConsumer(page, 'case=static-scroll');
    const viewport = page.getByRole('region', { name: 'Read-only records table viewport', exact: true });
    const table = viewport.getByRole('table', { name: 'Read-only records', exact: true });
    await expect(viewport).toHaveAttribute('tabindex', '0');
    await expect(table.locator('button, a, input, select, textarea, [tabindex]')).toHaveCount(0);
    const size = await viewport.evaluate(element => ({
      clientHeight: element.clientHeight, scrollHeight: element.scrollHeight,
      clientWidth: element.clientWidth, scrollWidth: element.scrollWidth,
    }));
    expect(size.scrollHeight).toBeGreaterThan(size.clientHeight);
    expect(size.scrollWidth).toBeGreaterThan(size.clientWidth);
    const forwardTab = await fullControlTab(page, testInfo);
    const backwardTab = forwardTab === 'Alt+Tab' ? 'Alt+Shift+Tab' : 'Shift+Tab';
    await page.getByRole('textbox', { name: 'Before viewport', exact: true }).focus();
    await page.keyboard.press(forwardTab);
    await expect(viewport).toBeFocused();
    const backgroundY = await page.evaluate(() => window.scrollY);
    const vertical = {
      down: await nativePageScroll(page, viewport, 'PageDown'),
      up: await nativePageScroll(page, viewport, 'PageUp'),
    };
    // WebKit's native arrow scrolling needs a held key, not a zero-duration tap.
    await page.keyboard.press('ArrowRight', { delay: 100 });
    await expect.poll(() => viewport.evaluate(element => element.scrollLeft)).toBeGreaterThan(0);
    expect(await page.evaluate(() => window.scrollY)).toBe(backgroundY);
    const scrolled = await viewport.evaluate(element => ({ top: element.scrollTop, left: element.scrollLeft }));
    await page.keyboard.press(forwardTab);
    await expect(page.getByRole('textbox', { name: 'After viewport', exact: true })).toBeFocused();
    await page.keyboard.press(backwardTab);
    await expect(viewport).toBeFocused();
    await page.keyboard.press(backwardTab);
    await expect(page.getByRole('textbox', { name: 'Before viewport', exact: true })).toBeFocused();
    await audit(page, testInfo, 'static-keyboard-scroll');
    await accessibilityEvidence(page, testInfo, 'static-keyboard-scroll', [
      { role: 'region', name: 'Read-only records table viewport' },
      { role: 'table', name: 'Read-only records' },
    ]);
    await testInfo.attach('native-table-scroll.json', {
      body: JSON.stringify({ size, vertical, scrolled, backgroundY }), contentType: 'application/json',
    });
  });

  for (const state of ['loading', 'refreshing', 'empty', 'error', 'stale', 'stale-error', 'actions']) {
    test(`axe: pure ${state} state has accessible names and native actions`, async ({ page }, testInfo) => {
      await openConsumer(page, `case=states&state=${state}`);
      if (state === 'loading' || state === 'refreshing') {
        await expect(page.getByRole('status', { name: 'Loading', exact: true })).toBeVisible();
      }
      if (state === 'empty') await expect(page.getByRole('cell', { name: 'No data available' })).toBeVisible();
      if (state === 'error' || state === 'stale-error') {
        await expect(page.getByRole('alert')).toContainText('Synthetic table unavailable');
        await page.getByRole('button', { name: 'Retry', exact: true }).press('Enter');
        await expect(page.getByTestId('retry-count')).toHaveText('1');
      }
      if (state === 'stale' || state === 'stale-error') {
        await expect(page.getByText('Showing last known data.', { exact: state === 'stale' })).toBeVisible();
        await expect(page.getByRole('table', { name: `${state} records`, exact: true }).locator('tbody tr')).toHaveCount(3);
      }
      if (state === 'actions') {
        const table = page.getByRole('table', { name: 'actions records', exact: true });
        const loadingAction = table.getByRole('row').filter({ hasText: 'Bravo' }).getByRole('button');
        await expect(loadingAction).toHaveAccessibleName('Inspect item');
        await expect(loadingAction).toHaveAttribute('aria-busy', 'true');
        await expect(loadingAction).toBeDisabled();
        await expect(table.getByRole('row').filter({ hasText: 'Charlie' }).getByRole('button')).toBeDisabled();
        await table.getByRole('row').filter({ hasText: 'Alpha' }).getByRole('button', { name: 'Inspect item' }).press('Space');
        await expect(page.getByTestId('action-result')).toHaveText('Alpha');
        await expect(table.getByRole('columnheader', { name: 'Actions', exact: true })).toHaveAttribute('scope', 'col');
      }
      await audit(page, testInfo, `pure-${state}`);
    });
  }

  test('modal initial focus, forward/backward containment, control removal and trigger restoration', async ({ page }, testInfo) => {
    await openConsumer(page, 'case=modal');
    const original = await bodyInline(page);
    await openOwner(page);
    const dialog = modal(page);
    await expect(dialog).toHaveAccessibleName('primary dialog');
    await expect(dialog).toHaveAccessibleDescription('primary test controls');
    await expect(dialog).toHaveAttribute('aria-modal', 'true');
    await expectLocked(page);
    await page.keyboard.press('Tab');
    await focusIsWithin(dialog);
    await page.keyboard.press('Shift+Tab');
    await focusIsWithin(dialog);
    await dialog.focus();
    const forwardTab = await fullControlTab(page, testInfo);
    await page.keyboard.press(forwardTab);
    const first = dialog.getByRole('button', { name: 'First primary control' });
    const last = dialog.getByRole('button', { name: 'Close primary dialog', exact: true });
    await expect(first).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(last).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(first).toBeFocused();
    const controls = await dialog.locator('button, input').count();
    for (let index = 0; index < controls + 2; index += 1) {
      await page.keyboard.press(forwardTab);
      await focusIsWithin(dialog);
    }
    // A programmatic background focus attempt is also contained; no forced background clicks.
    await page.locator('#outside-control').focus();
    await focusIsWithin(dialog);
    await dialog.getByRole('button', { name: 'Remove focused primary control' }).press('Enter');
    await expect(dialog.getByRole('button', { name: 'Remove focused primary control' })).toHaveCount(0);
    await focusIsWithin(dialog);
    await page.keyboard.press('Tab');
    await focusIsWithin(dialog);
    await audit(page, testInfo, 'pure-modal', false, dialog);
    await accessibilityEvidence(page, testInfo, 'pure-modal', [
      { role: 'dialog', name: 'primary dialog' },
      { role: 'textbox', name: 'primary note' },
    ]);
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: 'Open primary dialog', exact: true })).toBeFocused();
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
    await expectReleased(page, original);
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
  });

  for (const [action, state] of [['Remove', 'missing'], ['Disable', 'disabled'], ['Hide', 'hidden']] as const) {
    test(`modal falls back when its trigger is ${state}`, async ({ page }) => {
      await openConsumer(page, 'case=modal');
      const original = await bodyInline(page);
      await openOwner(page);
      await modal(page).getByRole('button', { name: `${action} primary trigger` }).click();
      await page.keyboard.press('Escape');
      await expect(page.getByRole('button', { name: 'Fallback target' })).toBeFocused();
      await expectReleased(page, original);
    });
  }

  test('explicit focus refs and final body fallback are honored', async ({ page }) => {
    await openConsumer(page, 'case=modal&initial-focus=1&explicit-return=1');
    await page.getByRole('button', { name: 'Open primary dialog', exact: true }).press('Enter');
    await expect(modal(page).getByRole('textbox', { name: 'primary note' })).toBeFocused();
    await expectModalReady(page, modal(page));
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: 'Explicit return target' })).toBeFocused();

    await openConsumer(page, 'case=modal&body-fallback=1');
    await openOwner(page);
    await modal(page).getByRole('button', { name: 'Remove primary trigger' }).click();
    await page.keyboard.press('Escape');
    await expect(page.locator('body')).toBeFocused();
    await expect(page.locator('body')).not.toHaveAttribute('tabindex');
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
  });

  test('only the top layer dismisses on Escape or a real outside pointer press', async ({ page }) => {
    await openConsumer(page, 'case=modal');
    const original = await bodyInline(page);
    await openOwner(page);
    const primary = modal(page);
    const openSecond = primary.getByRole('button', { name: 'Open secondary layer' });
    await openSecond.press('Enter');
    await expect(modal(page, 'secondary')).toBeFocused();
    await expectModalReady(page, modal(page, 'secondary'));
    await page.keyboard.press('Escape');
    await expect(modal(page, 'secondary')).toHaveCount(0);
    await expect(primary).toBeVisible();
    await expect(openSecond).toBeFocused();
    await expect(page.getByTestId('primary-closes')).toHaveText('0');
    await expect(page.getByTestId('secondary-closes')).toHaveText('1');
    await expectLocked(page);
    await openSecond.press('Enter');
    await expectModalReady(page, modal(page, 'secondary'));
    await page.mouse.click(2, 2);
    await expect(modal(page, 'secondary')).toHaveCount(0);
    await expect(primary).toBeVisible();
    await expect(page.getByTestId('primary-closes')).toHaveText('0');
    await expect(page.getByTestId('secondary-closes')).toHaveText('2');
    await expectLocked(page);
    await expectModalReady(page, primary);
    await page.mouse.click(2, 2);
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
    await expectReleased(page, original);
  });

  test('disabled Escape/outside dismissal does not compromise the visible close control', async ({ page }) => {
    await openConsumer(page, 'case=modal&no-dismiss=1');
    const original = await bodyInline(page);
    await openOwner(page);
    const input = modal(page).getByRole('textbox', { name: 'primary note' });
    await input.focus();
    await page.keyboard.press('Escape');
    await expect(input).toBeFocused();
    await page.mouse.click(2, 2);
    await expect(input).toBeFocused();
    await expect(modal(page)).toBeVisible();
    await expectLocked(page);
    await expect(page.getByTestId('primary-closes')).toHaveText('0');
    await modal(page).getByRole('button', { name: 'Close primary dialog', exact: true }).press('Enter');
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
    await expectReleased(page, original);
  });

  test('an outside close request retains focus and scroll ownership until the controlled owner applies it', async ({ page }) => {
    await openConsumer(page, 'case=modal&pending-close=1');
    const original = await bodyInline(page);
    await openOwner(page);
    const dialog = modal(page);
    const input = dialog.getByRole('textbox', { name: 'primary note' });
    await input.fill('Still editing');
    await page.mouse.click(2, 2);
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
    await expect(dialog).toBeVisible();
    await expect(input).toBeFocused();
    await expectLocked(page);
    await page.keyboard.press('End');
    await page.keyboard.type(' safely');
    await expect(input).toHaveValue('Still editing safely');
    await dialog.getByRole('button', { name: 'Apply primary close request' }).press('Enter');
    await expect(page.getByRole('button', { name: 'Open primary dialog', exact: true })).toBeFocused();
    await expect(page.getByTestId('primary-closes')).toHaveText('1');
    await expectReleased(page, original);
  });

  for (const lower of ['primary', 'secondary'] as const) {
    for (const order of ['top first', 'lower first', 'unmount lower'] as const) {
      test(`independent owners: ${lower} below, ${order}, retain the surviving scroll/focus owner`, async ({ page }) => {
        await openConsumer(page, 'case=modal');
        const original = await bodyInline(page);
        const top = lower === 'primary' ? 'secondary' : 'primary';
        await openOwner(page, lower);
        await modal(page, lower).getByRole('button', { name: `Open ${top} layer` }).click();
        await expect(modal(page, top)).toBeFocused();
        await expectModalReady(page, modal(page, top));
        await expectLocked(page);
        expect(await bodyInline(page)).toEqual({ ...original, pointerEvents: 'none' });
        let survivor: 'primary' | 'secondary';
        if (order === 'top first') {
          await page.keyboard.press('Escape');
          await expect(modal(page, top)).toHaveCount(0);
          survivor = lower;
        } else {
          await modal(page, top).getByRole('button', {
            name: `${order === 'lower first' ? 'Close' : 'Unmount'} ${lower} owner`, exact: true,
          }).click();
          await expect(modal(page, lower)).toHaveCount(0);
          survivor = top;
        }
        await expectLocked(page);
        await expect(modal(page, survivor)).toBeVisible();
        await focusIsWithin(modal(page, survivor));
        await modal(page, survivor).getByRole('button', { name: `Close ${survivor} dialog`, exact: true }).click();
        await expectReleased(page, original);
        const counts = await page.locator('[data-testid$="-closes"]').allTextContents();
        await page.keyboard.press('Escape');
        expect(await page.locator('[data-testid$="-closes"]').allTextContents()).toEqual(counts);
        await openOwner(page, lower);
        await page.keyboard.press('Escape');
        await expectReleased(page, original);
      });
    }
  }

  for (const action of ['Close all owners', 'Unmount all owners', 'Unmount fixture']) {
    test(`StrictMode open and ${action.toLowerCase()} remove locks, shields and focus guards`, async ({ page }) => {
      await openConsumer(page, 'case=modal&initial=1');
      const original = { ...(await bodyInline(page)), pointerEvents: '' };
      await expect(modal(page)).toBeFocused();
      await modal(page).getByRole('button', { name: 'Open secondary layer' }).click();
      await expect(modal(page, 'secondary')).toBeFocused();
      await modal(page, 'secondary').getByRole('button', { name: action, exact: true }).click();
      await expectReleased(page, original);
      await page.keyboard.press('Escape');
      await page.locator('#outside-control').press('Enter');
      await expect(page.locator('#outside-control')).toBeFocused();
    });
  }

  test('default light tokens, unrelated host styles and independently themed portals remain isolated', async ({ page }, testInfo) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await openConsumer(page, 'case=theme');
    const hostStyles = () => page.locator('[data-testid^="host-"], #outside-control').evaluateAll(elements =>
      elements.map(element => {
        const style = getComputedStyle(element);
        return {
          color: style.color, background: style.backgroundColor, fontFamily: style.fontFamily,
          fontSize: style.fontSize, fontWeight: style.fontWeight, borderWidth: style.borderWidth,
          borderCollapse: style.borderCollapse, borderSpacing: style.borderSpacing, margin: style.margin,
        };
      }));
    const original = await hostStyles();
    await expect(page.getByTestId('host-button')).toHaveCSS('font-size', '19px');
    await expect(page.getByTestId('host-heading')).toHaveCSS('font-size', '23px');
    await expect(page.getByTestId('host-table')).toHaveCSS('border-collapse', 'separate');
    await expect(page.locator('#outside-control')).toHaveCSS('font-size', '17px');
    expect(await page.evaluate(() =>
      Array.from(getComputedStyle(document.documentElement)).filter(property => property.startsWith('--drasi-')),
    )).toEqual([]);
    const defaultCard = page.getByTestId('default-theme').locator('.drasi-query-table');
    await expect(defaultCard).toHaveCSS('background-color', 'rgb(255, 255, 255)');
    await expect(defaultCard).toHaveCSS('color', 'rgb(17, 24, 39)');
    const scopedCard = page.getByTestId('first-theme').locator('.drasi-query-table');
    await expect(scopedCard).toHaveCSS('background-color', 'rgb(245, 236, 255)');
    await expect(scopedCard).toHaveCSS('font-family', 'Georgia, serif');
    await page.getByRole('button', { name: 'Open themed dialog' }).press('Enter');
    const first = page.locator('[data-drasi-modal-content]').filter({ hasText: 'Portaled theme content' });
    await expect(first).toHaveCSS('background-color', 'rgb(245, 236, 255)');
    await expect(first).toHaveCSS('color', 'rgb(40, 17, 62)');
    await expect(first).toHaveCSS('font-family', 'Georgia, serif');
    await expect(first).toHaveCSS('font-size', '18px');
    await expect.soft(first).toHaveCSS('font-weight', '500');
    expect(await first.evaluate(element => getComputedStyle(element).getPropertyValue('--drasi-line-height').trim())).toBe('1.6');
    await expect(first).toHaveCSS('direction', 'rtl');
    expect(await first.evaluate(element => getComputedStyle(element).getPropertyValue('--drasi-test-token').trim())).toBe('fixture-violet');
    expect(await first.evaluate(element => !document.querySelector('.fixture-main')?.contains(element))).toBe(true);
    await first.getByRole('button', { name: 'Open other theme' }).click();
    const second = page.getByRole('dialog', { name: 'Other theme dialog', exact: true });
    await expect(second).toHaveCSS('background-color', 'rgb(230, 246, 250)');
    await expect(second).toHaveCSS('direction', 'ltr');
    await expect(second).toHaveCSS('font-family', '"Courier New", monospace');
    await expect(first).toHaveCSS('background-color', 'rgb(245, 236, 255)');
    await second.getByRole('button', { name: 'Close other theme' }).click();
    await first.getByRole('button', { name: 'Change ancestor theme' }).click();
    await expect(first).toHaveCSS('background-color', 'rgb(230, 246, 250)');
    await expect(first).toHaveCSS('font-family', '"Courier New", monospace');
    await expect(first).toHaveCSS('font-size', '20px');
    await expect(first).toHaveCSS('direction', 'ltr');
    await page.getByTestId('first-theme').evaluate(element => {
      element.style.setProperty('--drasi-color-dialog', '#fff4d6');
      element.style.fontFamily = 'Verdana, sans-serif';
    });
    await expect(first).toHaveCSS('background-color', 'rgb(255, 244, 214)');
    await expect(first).toHaveCSS('font-family', 'Verdana, sans-serif');
    await page.setViewportSize({ width: 1100, height: 800 });
    await page.emulateMedia({ colorScheme: 'light' });
    await expect(first).toHaveCSS('background-color', 'rgb(255, 244, 214)');
    expect(await hostStyles()).toEqual(original);
    await audit(page, testInfo, 'scoped-theme-modal', false, first);
    await first.getByRole('button', { name: 'Close themed dialog' }).click();
    expect(await hostStyles()).toEqual(original);
    await expect(defaultCard).toHaveCSS('background-color', 'rgb(255, 255, 255)');
    await audit(page, testInfo, 'isolated-host-and-tables');

    await openConsumer(page, 'case=theme&environment=1');
    await page.getByRole('button', { name: 'Open themed dialog' }).press('Enter');
    await page.getByRole('button', { name: 'Open other theme' }).press('Enter');
    const environmental = page.getByRole('dialog', { name: 'Other theme dialog', exact: true });
    await expect(environmental).toHaveCSS('background-color', 'rgb(255, 244, 214)');
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect(environmental).toHaveCSS('background-color', 'rgb(243, 232, 255)');
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(environmental).toHaveCSS('background-color', 'rgb(220, 252, 231)');
  });

  test('unitless typography tokens scale table and portaled descendants instead of freezing source pixels', async ({ page }, testInfo) => {
    await openConsumer(page, 'case=theme');
    const defaultCard = page.getByTestId('default-theme').locator('.drasi-query-table');
    const source = page.getByTestId('first-theme');
    const scopedCard = source.locator('.drasi-query-table');
    const title = scopedCard.locator('.drasi-query-table__title');
    const lineHeight = (locator: Locator) => locator.evaluate(element => parseFloat(getComputedStyle(element).lineHeight));
    const metrics = (locator: Locator) => locator.evaluate(element => {
      const style = getComputedStyle(element);
      return { fontSize: parseFloat(style.fontSize), lineHeight: parseFloat(style.lineHeight) };
    });
    await expect(defaultCard).toHaveCSS('line-height', '24px');
    await page.getByRole('button', { name: 'Open themed dialog' }).press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Scoped theme dialog', exact: true });
    const enlargedText = dialog.getByTestId('themed-typography');
    const measurements = [];
    for (const [token, bodyLineHeight, titleLineHeight, dialogLineHeight, textLineHeight] of [
      ['1.6', 25.6, 32, 28.8, 48],
      ['1.5', 24, 30, 30, 45],
      ['1.8', 28.8, 36, 36, 54],
    ] as const) {
      if (token === '1.5') await dialog.getByRole('button', { name: 'Change ancestor theme' }).click();
      if (token === '1.8') await source.evaluate(element => element.style.setProperty('--drasi-line-height', '1.8'));
      await expect.poll(() => lineHeight(scopedCard)).toBeCloseTo(bodyLineHeight, 3);
      await expect.poll(() => lineHeight(title)).toBeCloseTo(titleLineHeight, 3);
      await expect.poll(() => lineHeight(dialog)).toBeCloseTo(dialogLineHeight, 3);
      await expect.poll(() => lineHeight(enlargedText)).toBeCloseTo(textLineHeight, 3);
      measurements.push({
        token, table: await metrics(scopedCard), title: await metrics(title),
        dialog: await metrics(dialog), enlargedText: await metrics(enlargedText),
      });
    }
    await expect(defaultCard).toHaveCSS('line-height', '24px');
    await testInfo.attach('unitless-typography.json', {
      body: JSON.stringify(measurements, null, 2), contentType: 'application/json',
    });
  });

  test('actual layout honors numeric, string, token, percentage, rem, zero and automatic heights', async ({ page }, testInfo) => {
    await openConsumer(page, 'case=sizing');
    const measure = () => page.locator('[data-testid^="size-"]').evaluateAll(elements =>
      Object.fromEntries(elements.map(parent => {
        const element = parent.querySelector<HTMLElement>('.drasi-query-table')!;
        const style = getComputedStyle(element);
        return [parent.getAttribute('data-testid')!.slice(5), {
          height: element.getBoundingClientRect().height,
          inline: element.style.height,
          border: parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth),
        }];
      })));
    const sizes = await measure();
    for (const [name, height] of Object.entries({
      default: 400, numeric: 180, pixels: 150, rem: 192, em: 160,
      viewport: 250, 'dynamic-viewport': 250, percentage: 180,
      variable: 216, 'variable-fallback': 144, 'default-token': 232,
    })) expect(sizes[name].height, name).toBeCloseTo(height, 1);
    for (const name of ['zero', 'zero-string']) {
      expect(sizes[name].inline).toBe('0px');
      // Border-box zero still has its two visible one-pixel borders; no 400px fallback.
      expect(sizes[name].height).toBe(sizes[name].border);
    }
    expect(sizes.auto.inline).toBe('auto');
    expect(sizes.auto.height).toBeGreaterThan(50);
    expect(sizes.auto.height).toBeLessThan(400);
    await page.getByTestId('size-variable').evaluate(element => element.style.setProperty('--fixture-size', '248px'));
    await expect(page.getByTestId('size-variable').locator('.drasi-query-table')).toHaveCSS('height', '248px');
    await page.setViewportSize({ width: 390, height: 844 });
    const narrow = await measure();
    expect(narrow.viewport.height).toBeCloseTo(211, 1);
    expect(narrow['dynamic-viewport'].height).toBeCloseTo(211, 1);
    expect(narrow.numeric.height).toBe(180);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await testInfo.attach('measured-table-heights.json', {
      body: JSON.stringify({ desktop: sizes, narrow }, null, 2), contentType: 'application/json',
    });
  });

  test('small viewport modal content is keyboard-scrollable without an inaccessible focus trap', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 320, height: 568 });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await openConsumer(page, 'case=modal&long=1&scrolled-host=1');
    await page.evaluate(() => window.scrollTo(0, 160));
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
    const scrollY = await page.evaluate(() => window.scrollY);
    await openOwner(page);
    const dialog = modal(page);
    expect(await page.evaluate(() => window.scrollY)).toBe(scrollY);
    const bounds = await dialog.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.y).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(320);
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(568);
    const vertical = {
      down: await nativePageScroll(page, dialog, 'PageDown'),
      up: await nativePageScroll(page, dialog, 'PageUp'),
      returnedDown: await nativePageScroll(page, dialog, 'PageDown'),
    };
    expect(await page.evaluate(() => window.scrollY)).toBe(scrollY);
    const first = dialog.getByRole('button', { name: 'First primary control' });
    await first.focus();
    const close = dialog.getByRole('button', { name: 'Close primary dialog', exact: true });
    const reveals = [];
    for (const [key, target] of [['Shift+Tab', close], ['Tab', first], ['Shift+Tab', close]] as const) {
      await page.keyboard.press(key);
      const immediate = await target.evaluate(element => {
        const rect = element.getBoundingClientRect();
        const range = document.createRange();
        range.selectNodeContents(element);
        const label = range.getBoundingClientRect();
        const container = element.closest('[data-drasi-modal-content]');
        if (!(container instanceof HTMLElement)) throw new Error('Expected an owned modal scroll container');
        const clip = container.getBoundingClientRect();
        return {
          focused: element === document.activeElement,
          top: rect.top, bottom: rect.bottom, left: rect.left, right: rect.right,
          labelTop: label.top, labelBottom: label.bottom,
          clipTop: clip.top + container.clientTop,
          clipBottom: clip.top + container.clientTop + container.clientHeight,
          backgroundY: window.scrollY,
        };
      });
      expect(immediate.focused).toBe(true);
      expect(immediate.top).toBeGreaterThanOrEqual(0);
      expect(immediate.bottom).toBeLessThanOrEqual(568);
      expect(immediate.left).toBeGreaterThanOrEqual(0);
      expect(immediate.right).toBeLessThanOrEqual(320);
      // Validate rendered labels, not browser-specific native button borders.
      expect(immediate.labelTop).toBeGreaterThanOrEqual(immediate.clipTop);
      expect(immediate.labelBottom).toBeLessThanOrEqual(immediate.clipBottom);
      expect(immediate.backgroundY).toBe(scrollY);
      await expect(target).toBeInViewport();
      reveals.push({ key, ...immediate });
    }
    await testInfo.attach('bounded-focus-reveal.json', {
      body: JSON.stringify({ reducedMotion: 'no-preference', backgroundY: scrollY, vertical, reveals }, null, 2),
      contentType: 'application/json',
    });
    await audit(page, testInfo, 'narrow-scrollable-modal', false, dialog);
    await close.press('Enter');
    await expect(page.getByRole('button', { name: 'Open primary dialog', exact: true })).toBeFocused();
    expect(await page.evaluate(() => window.scrollY)).toBe(scrollY);
  });

  test('initial reduced motion suppresses tracked and controlled decoration without delaying data', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await openConsumer(page, 'case=motion');
    await expect(page.getByTestId('motion-preference')).toHaveText('reduce');
    await page.getByRole('button', { name: 'Update quantities' }).click();
    for (const title of ['Tracked motion', 'Controlled motion']) {
      const changed = page.getByRole('table', { name: title, exact: true }).getByRole('row').filter({ hasText: 'Bravo' });
      await expect(changed.getByRole('cell', { name: '7', exact: true })).toBeVisible();
      await expect(changed).not.toHaveClass(/drasi-row--/);
      await expect(changed).toHaveCSS('animation-name', 'none');
      await expect(changed).toHaveCSS('transition-duration', '0s');
    }
  });

  test('live motion preference cancels pending row decoration and unmount cleanup without losing updates', async ({ page }) => {
    const time = new Date();
    await page.clock.install({ time });
    await page.clock.pauseAt(time);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await openConsumer(page, 'case=motion');
    const tracked = page.getByRole('table', { name: 'Tracked motion' }).getByRole('row').filter({ hasText: 'Bravo' });
    const controlled = page.getByRole('table', { name: 'Controlled motion' }).getByRole('row').filter({ hasText: 'Bravo' });
    await expect(controlled).toHaveClass(/drasi-row--up/);
    await expect(controlled).toHaveCSS('animation-name', 'drasi-flash-green');
    await page.getByRole('button', { name: 'Update quantities' }).click();
    await expect(tracked).toHaveClass(/drasi-row--up/);
    await expect(tracked).toHaveCSS('animation-duration', '0.5s');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await expect(page.getByTestId('motion-preference')).toHaveText('reduce');
    await expect(tracked).not.toHaveClass(/drasi-row--/);
    await expect(controlled).not.toHaveClass(/drasi-row--/);
    await page.getByRole('button', { name: 'Update quantities' }).click();
    await expect(tracked.getByRole('cell', { name: '12', exact: true })).toBeVisible();
    await page.clock.runFor(1000);
    await expect(tracked.getByRole('cell', { name: '12', exact: true })).toBeVisible();
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await expect(page.getByTestId('motion-preference')).toHaveText('no-preference');
    await page.getByRole('button', { name: 'Update quantities' }).click();
    await expect(tracked).toHaveClass(/drasi-row--up/);
    await expect(tracked.getByRole('cell', { name: '17', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Unmount animated tables' }).click();
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.clock.runFor(1000);
    await expect(page.getByRole('table')).toHaveCount(0);
    await expect(page.getByTestId('motion-preference')).toHaveText('reduce');
  });

  test('audit readiness rejects settled content while its own overlay is still fading', async ({ page }, testInfo) => {
    await openConsumer(page, 'case=modal');
    await openOwner(page);
    const dialog = modal(page);
    const owner = page.locator('.drasi-modal-layer').filter({ has: dialog });
    const animation = await owner.evaluateHandle(element => {
      const fade = element.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 1000, fill: 'both' });
      fade.pause();
      fade.currentTime = 500;
      return fade;
    });
    const fading = await auditReadiness(dialog);
    expect(fading).toEqual({ contentOpacity: '1', ownerOpacity: '0.5', finiteMotion: 0 });
    let ready = false;
    const pending = settleTradingAudit(page, dialog).then(() => { ready = true; });
    try {
      await observePaints(page);
      expect(ready).toBe(false);
      await animation.evaluate(fade => fade.finish());
      await pending;
      expect(ready).toBe(true);
      const settled = await auditReadiness(dialog);
      expect(settled).toEqual({ contentOpacity: '1', ownerOpacity: '1', finiteMotion: 0 });
      await testInfo.attach('audit-owner-readiness.json', {
        body: JSON.stringify({ fading, settled }), contentType: 'application/json',
      });
    } finally {
      await animation.evaluate(fade => fade.cancel());
      await animation.dispose();
      await pending;
    }
  });

  test('the fixture server keeps encoded traversal and malformed paths outside its static root', async ({ request }) => {
    for (const path of [
      '/__components/..%2f..%2fpackage.json',
      '/__components/%2e%2e%5cpackage.json',
      '/__components/%00',
      '/__components/%ZZ',
    ]) {
      const response = await request.get(path);
      expect([400, 404], path).toContain(response.status());
      expect(await response.text()).not.toContain('"dependencies"');
    }
    const response = await request.get('/__components/');
    expect(response.ok()).toBe(true);
    expect(await response.text()).toContain('Installed component browser fixture');
  });
});

trading.describe('Trading keyboard, modal interoperability and motion', () => {
  trading('small-height Trading dialog scrolls its outer overlay and keeps keyboard controls reachable', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 390, height: 320 });
    await page.clock.resume();
    await openTrading(page);
    const opener = page.getByTitle('Add position', { exact: true });
    await opener.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Add Position', exact: true });
    const overlay = page.locator('.trading-dialog-overlay').filter({ has: dialog });
    await expect(dialog).toBeFocused();
    await expectModalReady(page, dialog);
    await expect(dialog).toHaveCSS('overflow', 'visible');
    await expect(dialog).toHaveCSS('max-height', 'none');
    await expect(overlay).toHaveCSS('overflow-y', 'auto');
    const size = await overlay.evaluate(element => ({
      clientHeight: element.clientHeight, scrollHeight: element.scrollHeight,
    }));
    expect(size.scrollHeight).toBeGreaterThan(size.clientHeight);
    await expect(dialog.getByRole('heading', { name: 'Add Position', exact: true })).toBeInViewport();
    const backgroundY = await page.evaluate(() => window.scrollY);
    const vertical = {
      down: await nativePageScroll(page, overlay, 'PageDown'),
      up: await nativePageScroll(page, overlay, 'PageUp'),
    };
    await page.keyboard.press('End');
    const cancel = dialog.getByRole('button', { name: 'Cancel', exact: true });
    await expect(cancel).toBeInViewport();
    await expect(dialog.getByRole('button', { name: 'Add', exact: true })).toBeInViewport();
    expect(await page.evaluate(() => window.scrollY)).toBe(backgroundY);
    const forwardTab = await fullControlTab(page, testInfo);
    await page.keyboard.press(forwardTab);
    const first = dialog.getByRole('combobox', { name: 'Stock', exact: true });
    const last = dialog.getByRole('button', { name: 'Add', exact: true });
    await expect(first).toBeFocused();
    await expect(last).toBeEnabled();
    const wraps = [];
    for (const [key, target] of [['Shift+Tab', last], ['Tab', first]] as const) {
      await page.keyboard.press(key);
      const immediate = await target.evaluate(element => {
        const rect = element.getBoundingClientRect();
        return {
          focused: element === document.activeElement,
          top: rect.top, bottom: rect.bottom,
          backgroundY: window.scrollY,
        };
      });
      expect(immediate.focused).toBe(true);
      expect(immediate.top).toBeGreaterThanOrEqual(0);
      expect(immediate.bottom).toBeLessThanOrEqual(320);
      expect(immediate.backgroundY).toBe(backgroundY);
      await expect(target).toBeInViewport();
      wraps.push({ key, ...immediate });
    }
    for (let index = 0; index < 14 && !await cancel.evaluate(element => element === document.activeElement); index += 1) {
      await page.keyboard.press(forwardTab);
      await focusIsWithin(dialog);
    }
    await expect(cancel).toBeFocused();
    await expect(cancel).toBeInViewport();
    expect(await page.evaluate(() => window.scrollY)).toBe(backgroundY);
    await testInfo.attach('small-height-trading-scroll.json', {
      body: JSON.stringify({ size, vertical, wraps, backgroundY, scrollTop: await overlay.evaluate(element => element.scrollTop) }),
      contentType: 'application/json',
    });
    await cancel.press('Enter');
    await expect(dialog).toHaveCount(0);
    await expect(opener).toBeFocused();
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
    expect(await page.evaluate(() => window.scrollY)).toBe(backgroundY);
  });

  trading('nested code viewer owns dismissal, roving tabs and AX above the expanded table', async ({ page, browserName }, testInfo) => {
    await page.clock.resume();
    await openTrading(page);
    const expand = panel(page, 'Watchlist').getByRole('button', { name: 'Expand table' });
    await expand.focus();
    await expand.press('Enter');
    const expanded = page.locator('[data-drasi-modal-content].trading-expanded-dialog');
    await expect(expanded).toBeFocused();
    await expect(expanded).toHaveCSS('transition-duration', '0.35s');
    await expect.poll(async () => {
      const bounds = await expanded.boundingBox();
      return bounds && { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height };
    }).toEqual({ x: 32, y: 32, width: 1376, height: 936 });
    const firstHeading = expanded.getByRole('table').getByRole('columnheader').first();
    const nextDirection = await firstHeading.getAttribute('aria-sort') === 'ascending' ? 'descending' : 'ascending';
    await firstHeading.getByRole('button').press('Enter');
    await expect(firstHeading).toHaveAttribute('aria-sort', nextDirection);
    await expect(page.locator('.drasi-query-table--hidden th').first()).toHaveAttribute('aria-sort', nextDirection);
    const opener = expanded.getByRole('button', { name: 'View code' });
    await opener.press('Enter');
    const dialog = page.locator('[data-drasi-modal-content].drasi-code-dialog');
    await expect(dialog).toBeFocused();
    const tabs = dialog.getByRole('tablist', { name: 'Code examples' });
    await expect(tabs.getByRole('tab')).toHaveCount(2);
    const query = tabs.getByRole('tab', { name: 'Query Definition', exact: true });
    const react = tabs.getByRole('tab', { name: 'React Code', exact: true });
    const queryPanel = dialog.getByRole('tabpanel', { name: 'Query Definition', exact: true });
    await expect(query).toHaveAttribute('aria-selected', 'true');
    await expect(queryPanel).toContainText('ON_WATCHLIST');
    await expect(queryPanel).toHaveAttribute('id', (await query.getAttribute('aria-controls'))!);
    await expect(queryPanel).toHaveAttribute('aria-labelledby', (await query.getAttribute('id'))!);
    await settleTradingAudit(page, dialog);
    await audit(page, testInfo, 'trading-code-query', true, dialog, { state: 'inspector-query', browser: browserName });
    await query.focus();
    for (const [key, target] of [
      ['ArrowRight', react], ['ArrowRight', query], ['ArrowLeft', react], ['Home', query],
      ['End', react], ['Home', query], ['Enter', query], ['Space', query],
    ] as const) {
      await page.keyboard.press(key);
      await expect(target).toBeFocused();
      await expect(target).toHaveAttribute('aria-selected', 'true');
      await expect(tabs.locator('[tabindex="0"]')).toHaveCount(1);
    }
    const copy = dialog.getByRole('button', { name: 'Copy', exact: true });
    await expect(tabs.getByRole('button', { name: 'Copy', exact: true })).toHaveCount(0);
    await expect(tabs.getByRole('link')).toHaveCount(0);
    const link = dialog.getByRole('link', { name: 'Open in Drasi UI' });
    await expect(link).toHaveAttribute('href', 'http://localhost:8280/ui?instance=trading-server');
    await expect(link).toHaveAttribute('rel', 'noopener noreferrer');
    const forwardTab = await fullControlTab(page, testInfo);
    await page.keyboard.press(forwardTab);
    await expect(copy).toBeFocused();
    await page.keyboard.press(forwardTab === 'Alt+Tab' ? 'Alt+Shift+Tab' : 'Shift+Tab');
    await expect(query).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(dialog.getByRole('tabpanel', { name: 'React Code' })).toContainText('<TradingQueryTable<Stock>');
    if (browserName === 'chromium') {
      await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
      await copy.press('Enter');
      await expect(dialog.getByRole('button', { name: 'Copied!' })).toBeVisible();
      expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('queryId="watchlist-query"');
    } else {
      testInfo.annotations.push({
        type: 'clipboard-evidence-boundary',
        description: 'Native clipboard write/read permission proof runs in Chromium only; no clipboard success is simulated for Firefox/WebKit.',
      });
    }
    await settleTradingAudit(page, dialog);
    await audit(page, testInfo, 'trading-code-tabs', true, dialog, {
      state: browserName === 'chromium' ? 'inspector-react-copied' : 'inspector-react',
      browser: browserName,
    });
    await accessibilityEvidence(page, testInfo, 'trading-code-tabs', [
      { role: 'dialog', name: 'Watchlist' }, { role: 'tablist', name: 'Code examples' },
      { role: 'tab', name: 'React Code' }, { role: 'tabpanel', name: 'React Code' },
    ]);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(expanded).toBeVisible();
    await expect(opener).toBeFocused();
    await expectLocked(page);
    await opener.press('Enter');
    await expect(dialog.getByRole('tab', { name: 'Query Definition' })).toHaveAttribute('aria-selected', 'true');
    await observePaints(page);
    await page.mouse.click(2, 2);
    await expect(dialog).toHaveCount(0);
    await expect(expanded).toBeVisible();
    await expectLocked(page);
    await page.keyboard.press('Escape');
    await expect(expanded).toHaveCount(0);
    await expect(expand).toBeFocused();
    await expect(panel(page, 'Watchlist').getByRole('columnheader').first()).toHaveAttribute('aria-sort', nextDirection);
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
  });

  trading('full axe reports distinguish preserved dashboard findings from new regressions', async ({ page, browserName }, testInfo) => {
    await page.clock.resume();
    await openTrading(page);
    await settleTradingAudit(page, page.locator('body'));
    await audit(page, testInfo, 'trading-dashboard', false, undefined, { state: 'dashboard', browser: browserName });
  });

  for (const [trigger, title, submit, state] of [
    ['Add position', 'Add Position', 'Add', 'add-position'],
    ['New limit order', 'New Limit Order', 'Create Order', 'new-limit-order'],
    ['Add to watchlist', 'Add to Watchlist', 'Add', 'add-watchlist'],
  ] as const) {
    trading(`full-rule legacy contrast regression: ${state} and validation`, async ({ page, browserName }, testInfo) => {
      await page.clock.resume();
      await openTrading(page);
      const opener = page.getByTitle(trigger, { exact: true });
      await opener.focus();
      await opener.press('Enter');
      const dialog = page.getByRole('dialog', { name: title, exact: true });
      await expect(dialog).toBeFocused();
      for (const field of await dialog.locator('input, select').all()) await expect(field).toHaveAccessibleName(/\S/);
      await settleTradingAudit(page, dialog);
      await audit(page, testInfo, `trading-${state}`, true, dialog, { state, browser: browserName });
      if (title !== 'Add to Watchlist') {
        await dialog.getByRole('button', { name: submit, exact: true }).click();
        await expect(dialog.getByText('Quantity is required', { exact: true })).toBeVisible();
        await settleTradingAudit(page, dialog);
        await audit(page, testInfo, `trading-${state}-validation`, true, dialog, { state: `${state}-validation`, browser: browserName });
      }
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
      await expect(opener).toBeFocused();
    });
  }

  for (const state of [
    'edit-position', 'confirm-delete-position', 'confirm-cancel-order', 'new-limit-order-sell',
  ] as const) {
    trading(`full-rule legacy contrast regression: ${state}`, async ({ page, browserName }, testInfo) => {
      await page.clock.resume();
      await openTrading(page);
      if (state === 'new-limit-order-sell') {
        await page.getByTitle('New limit order', { exact: true }).click();
      } else if (state === 'confirm-cancel-order') {
        await panel(page, 'Limit Orders').getByRole('button', { name: 'Cancel order', exact: true }).first().click();
      } else {
        await row(page, 'Portfolio', 'AAPL').getByRole('button', {
          name: state === 'edit-position' ? 'Edit position' : 'Delete position', exact: true,
        }).click();
      }
      const title = {
        'edit-position': 'Edit Position', 'confirm-delete-position': 'Delete Position',
        'confirm-cancel-order': 'Cancel Order', 'new-limit-order-sell': 'New Limit Order',
      }[state];
      const dialog = page.getByRole('dialog', { name: title, exact: true });
      await expect(dialog).toBeFocused();
      if (state === 'new-limit-order-sell') await dialog.getByRole('button', { name: 'Sell', exact: true }).click();
      await settleTradingAudit(page, dialog);
      await audit(page, testInfo, `trading-${state}`, true, dialog, { state, browser: browserName });
      if (state === 'edit-position' || state === 'new-limit-order-sell') {
        if (state === 'edit-position') await dialog.getByRole('spinbutton', { name: 'Quantity', exact: true }).fill('');
        await dialog.getByRole('button', { name: state === 'edit-position' ? 'Save' : 'Create Order', exact: true }).click();
        await expect(dialog.getByText('Quantity is required', { exact: true })).toBeVisible();
        await settleTradingAudit(page, dialog);
        await audit(page, testInfo, `trading-${state}-validation`, true, dialog, { state: `${state}-validation`, browser: browserName });
      }
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
    });
  }

  trading('BaseDialog above fullscreen shares dismissal and restores focus without unlocking its lower owner', async ({ page }) => {
    await page.clock.resume();
    await openTrading(page);
    await panel(page, 'Portfolio').getByRole('button', { name: 'Expand table' }).press('Enter');
    const expanded = page.locator('[data-drasi-modal-content].trading-expanded-dialog');
    await expect(expanded).toBeFocused();
    const add = expanded.getByTitle('Add position', { exact: true });
    await add.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Add Position', exact: true });
    await expect(dialog).toBeFocused();
    await expectLocked(page);
    await expectModalReady(page, dialog);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(expanded).toBeVisible();
    await expect(add).toBeFocused();
    await expectLocked(page);
    await expanded.getByRole('button', { name: 'Collapse table' }).click();
    await expect(expanded).toHaveCount(0);
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
  });

  trading('initial reduced motion presents fullscreen synchronously and live prices still update', async ({ page, request }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await openTrading(page);
    const expand = panel(page, 'Watchlist').getByRole('button', { name: 'Expand table' });
    await expand.press('Enter');
    const expanded = page.locator('[data-drasi-modal-content].trading-expanded-dialog');
    // The existing fixture clock stays paused: a RAF/timer-dependent open or close cannot pass.
    await expect(expanded).toHaveCSS('top', '32px');
    await expect(expanded).toHaveCSS('left', '32px');
    await expect(expanded).toHaveCSS('transition-duration', '0s');
    expect((await request.post('/__fixture/price', { data: { symbol: 'AAPL', price: 126 } })).ok()).toBe(true);
    const changed = expanded.getByRole('row').filter({ has: page.getByRole('cell', { name: 'AAPL', exact: true }) });
    await expect(changed).toContainText('$126.00');
    await expect(changed).not.toHaveClass(/drasi-row--/);
    await expect(changed).toHaveCSS('animation-name', 'none');
    await expanded.getByRole('button', { name: 'Collapse table' }).press('Enter');
    await expect(expanded).toHaveCount(0);
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
    await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$126.00');
  });

  trading('switching motion preference finishes a scheduled FLIP exit and cancels work before reopen/unmount', async ({ page, request }, testInfo) => {
    testInfo.annotations.push({
      type: 'motion-evidence-boundary',
      description: 'FLIP teardown uses real document navigation; React owner unmount is exercised separately for generic Modal roots and animated DataTables.',
    });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await openTrading(page);
    const expand = panel(page, 'Watchlist').getByRole('button', { name: 'Expand table' });
    await expand.press('Enter');
    const expanded = page.locator('[data-drasi-modal-content].trading-expanded-dialog');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await expect(expanded).toHaveCSS('top', '32px');
    await expect(expanded).toHaveCSS('transition-duration', '0s');
    await expanded.getByRole('button', { name: 'Collapse table' }).press('Enter');
    await expect(expanded).toHaveCount(0);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await expand.press('Enter');
    await page.clock.runFor(400);
    await expect(expanded).toHaveCSS('top', '32px');
    await expect(expanded).toHaveCSS('transition-duration', '0.35s');
    await expanded.getByRole('button', { name: 'Collapse table' }).press('Enter');
    await expect(expanded).toHaveCount(1);
    await expectLocked(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await expect(expanded).toHaveCount(0);
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
    await expand.press('Enter');
    await expect(expanded).toHaveCSS('top', '32px');
    await page.clock.runFor(1000);
    await expect(expanded).toBeVisible();
    expect((await request.post('/__fixture/price', { data: { symbol: 'AAPL', price: 127 } })).ok()).toBe(true);
    await expect(expanded.getByRole('row').filter({ hasText: 'AAPL' })).toContainText('$127.00');
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await expanded.getByRole('button', { name: 'Collapse table' }).press('Enter');
    // Navigate away while the production exit timer is pending; no timer can affect the next mount.
    await page.goto('/__components/?case=modal');
    await expect(page.getByTestId('strict-setups')).toHaveText('2');
    await page.clock.runFor(1000);
    await expect(page.locator('[data-drasi-modal-content]')).toHaveCount(0);
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
    await page.clock.resume();
    await openOwner(page);
    await page.clock.runFor(1000);
    await expect(modal(page)).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(modal(page)).toHaveCount(0);
  });
});
