// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { test, expect } from '@playwright/test';
import { audit } from './audit';

test('explicit simulation renders every state, useful stale rows and deterministic retry without requests', async ({ page }, info) => {
  const requests: string[] = [];
  const errors: string[] = [];
  page.on('request', request => {
    if (/\/api\/|\/events/.test(request.url())) requests.push(request.url());
  });
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/showcase.html');
  await expect(page.getByText('Simulation only.', { exact: true })).toBeVisible();
  const state = page.getByLabel('Simulated query state');
  for (const status of ['initial-loading', 'empty', 'live', 'reconnecting', 'resynchronizing', 'stale-last-good-data', 'terminal-error']) {
    await state.selectOption(status);
    await expect(page.getByText(`Simulated query: ${status}`, { exact: false })).toBeVisible();
    if (status === 'initial-loading') await expect(page.getByRole('table')).toHaveCount(0);
    else if (status === 'empty') await expect(page.getByText('No simulated readings.')).toBeVisible();
    else await expect(page.getByRole('cell', { name: 'sim-probe-101', exact: true })).toBeVisible();
    await audit(page, info, status);
  }
  await page.getByRole('button', { name: 'Retry simulated query' }).click();
  await expect(state).toHaveValue('resynchronizing');
  await page.getByRole('button', { name: 'Finish simulated refresh' }).click();
  await expect(state).toHaveValue('live');
  await audit(page, info, 'retry-complete');
  expect(requests).toEqual([]);
  expect(errors).toEqual([]);
});

test('controlled sorting and updates retain domain identity under actual keyboard input', async ({ page }, info) => {
  await page.goto('/showcase.html');
  await page.getByLabel('Simulated query state').selectOption('live');
  const heading = page.getByRole('columnheader', { name: 'Temperature (C)' });
  const button = heading.getByRole('button');
  const keys = page.locator('tbody tr td:first-child');
  await expect(keys).toHaveText(['sim-probe-102', 'sim-probe-101']);
  await button.focus();
  await page.keyboard.press('Enter');
  await expect(heading).toHaveAttribute('aria-sort', 'ascending');
  await expect(keys).toHaveText(['sim-probe-101', 'sim-probe-102']);
  await page.keyboard.press('Space');
  await expect(heading).toHaveAttribute('aria-sort', 'descending');
  await expect(keys).toHaveText(['sim-probe-102', 'sim-probe-101']);
  await page.getByRole('button', { name: 'Input order' }).click();
  await expect(heading).not.toHaveAttribute('aria-sort');
  await page.getByRole('button', { name: 'Apply simulated update' }).click();
  await expect(page.getByRole('row').filter({ hasText: 'sim-probe-101' })).toContainText('4.0');
  await expect(keys).toHaveText(['sim-probe-102', 'sim-probe-101']);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.getByRole('button', { name: 'Apply simulated update' }).click();
  const row = page.getByRole('row').filter({ hasText: 'sim-probe-101' });
  await expect(row).toContainText('5.0');
  await expect(row).toHaveCSS('animation-name', 'none');
  await audit(page, info, 'keyboard-sort-reduced-motion');
});

for (const themed of [false, true]) {
  test(`${themed ? 'scoped' : 'default'} theme, body portal and narrow keyboard dialog`, async ({ page }, info) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/showcase.html');
    await page.getByLabel('Simulated query state').selectOption('live');
    if (themed) await page.getByLabel('Use cool theme').check();
    await audit(page, info, `${themed ? 'cool' : 'default'}-table`);
    const trigger = page.getByRole('button', { name: 'Inspect probe' }).first();
    await trigger.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Simulated probe details' });
    await expect(dialog).toBeVisible();
    const close = dialog.getByRole('button', { name: 'Close details' });
    await expect(close).toBeFocused();
    await expect(close).toBeInViewport();
    await expect(dialog).toHaveCSS('background-color', themed ? 'rgb(240, 249, 255)' : 'rgb(255, 255, 255)');
    expect(await dialog.evaluate(element => element.closest('main'))).toBeNull();
    await page.keyboard.press('Tab');
    await expect(close).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(close).toBeFocused();
    await audit(page, info, `${themed ? 'cool' : 'default'}-dialog`, '[role="dialog"]');
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(trigger).toBeFocused();
    await expect(page.locator('body')).not.toHaveAttribute('data-scroll-locked');
  });
}
