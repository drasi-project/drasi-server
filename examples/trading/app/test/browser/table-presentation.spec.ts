// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect, type Page } from '@playwright/test';
import { build } from 'vite';
import type { ContractRow } from './table-contract-view';

const app = fileURLToPath(new URL('../../', import.meta.url));
const require = createRequire(import.meta.url);
let client: string;
let serverEntry: string;
let serverDirectory: string | undefined;
let css: string;

test.use({ locale: 'sv-SE' });

async function fixtureBundle(entry: string, ssr: boolean): Promise<string> {
  const result = await build({
    configFile: false,
    root: app,
    logLevel: 'silent',
    resolve: { dedupe: ['react', 'react-dom'] },
    esbuild: { jsx: 'automatic' },
    define: { 'process.env.NODE_ENV': JSON.stringify('development') },
    ssr: { noExternal: ['@drasi/react'] },
    build: {
      write: false,
      minify: false,
      ssr: ssr ? fileURLToPath(new URL(entry, import.meta.url)) : false,
      rollupOptions: {
        input: fileURLToPath(new URL(entry, import.meta.url)),
        output: { format: ssr ? 'es' : 'iife', name: 'TableContract' },
      },
    },
  });
  assert(!Array.isArray(result) && 'output' in result, 'Expected a single in-memory test bundle');
  const entries = result.output.filter(item => item.type === 'chunk' && item.isEntry);
  assert.equal(entries.length, 1, 'Expected one fixture entry');
  const chunk = entries[0];
  assert(chunk?.type === 'chunk');
  if (!ssr) {
    assert.equal(result.output.filter(item => item.type === 'chunk').length, 1);
    return chunk.code;
  }
  // P6 retains a lazy client-only modal chunk; execute Node's real module graph.
  await mkdir(join(app, '.test-runtime'), { recursive: true });
  serverDirectory = await mkdtemp(join(app, '.test-runtime/table-ssr-'));
  for (const output of result.output) {
    const file = resolve(serverDirectory, output.fileName);
    assert(file.startsWith(`${serverDirectory}${sep}`), 'Fixture output escaped its owned directory');
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, output.type === 'chunk' ? output.code : output.source);
  }
  return join(serverDirectory, chunk.fileName);
}

test.beforeAll(async () => {
  client = await fixtureBundle('table-contract-client.tsx', false);
  serverEntry = await fixtureBundle('table-contract-ssr.ts', true);
  css = await readFile(require.resolve('@drasi/react/styles.css'), 'utf8');
});

test.afterAll(async () => {
  if (serverDirectory) await rm(serverDirectory, { recursive: true, force: true });
});

async function prepare(page: Page, html = '') {
  const diagnostics: string[] = [];
  page.on('pageerror', error => diagnostics.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error' || message.type() === 'warning') diagnostics.push(message.text());
  });
  await page.setContent(`<main class="table-contract-host" style="text-align: center"><div id="table-contract">${html}</div></main>`);
  await page.addStyleTag({ content: css });
  await page.addStyleTag({ content: '.table-contract-host table { text-align: inherit; }' });
  await page.addScriptTag({ content: client });
  return diagnostics;
}

test('real en-US SSR hydrates in a Swedish browser without replacing rows or reporting mismatches', async ({ page }) => {
  const rendered = JSON.parse(execFileSync(process.execPath, [serverEntry], {
    cwd: app,
    env: { ...process.env, LANG: 'en_US.UTF-8', LC_ALL: 'en_US.UTF-8', NODE_ENV: 'development' },
    encoding: 'utf8',
  }));
  expect(rendered.locale).toBe('en-US');
  expect(rendered.ambientOrder).toEqual(['A', '\u00c4', 'Z']);
  const diagnostics = await prepare(page, rendered.html);
  const ambient = await page.evaluate(() => ({
    locale: new Intl.Collator().resolvedOptions().locale,
    order: ['A', '\u00c4', 'Z'].sort((a, b) => a.localeCompare(b)),
  }));
  expect(ambient.locale).toMatch(/^sv/);
  expect(ambient.order).toEqual(['A', 'Z', '\u00c4']);
  const original = await page.locator('#table-contract > .drasi-query-table').elementHandle();
  assert(original);
  await page.evaluate((rows: ContractRow[]) => window.tableContract.hydrate(rows), rendered.rows);
  await expect.poll(() => page.evaluate(() => window.tableContract.commits)).toBe(1);
  expect(await page.evaluate(() => window.tableContract.errors)).toEqual([]);
  expect(diagnostics).toEqual([]);
  expect(await original.evaluate(element => element === document.querySelector('#table-contract > .drasi-query-table'))).toBe(true);
  await expect(page.locator('tbody tr td:nth-child(2)')).toHaveText(['A', '\u00c4', 'Z']);
});

test('explicit left applies to body and header under inherited center without changing unspecified alignment', async ({ page }) => {
  const diagnostics = await prepare(page);
  await page.evaluate(() => window.tableContract.render([{ id: 'rack', value: 'Reading' }], 'alignment'));
  await expect(page.locator('tbody td')).toHaveCount(4);
  await expect(page.locator('th').filter({ hasText: /^Explicit left$/ })).toHaveCSS('text-align', 'left');
  await expect(page.locator('tbody td').nth(0)).toHaveCSS('text-align', 'left');
  await expect(page.locator('tbody td').nth(1)).toHaveCSS('text-align', 'center');
  await expect(page.locator('tbody td').nth(2)).toHaveCSS('text-align', 'right');
  await expect(page.locator('tbody td').nth(3)).toHaveCSS('text-align', 'center');
  expect(diagnostics).toEqual([]);
});

test('mixed-value sorting is independent of rotations with stable row nodes and correct aria-sort', async ({ page }) => {
  const diagnostics = await prepare(page);
  const rows = [{ id: 'two', value: 2 }, { id: 'ten', value: 10 }, { id: 'text', value: '11' }];
  await page.evaluate(rows => window.tableContract.render(rows), rows);
  await expect(page.locator('tbody tr')).toHaveCount(3);
  const original = await page.getByRole('cell', { name: 'two', exact: true }).elementHandle();
  assert(original);
  for (let index = 0; index < rows.length; index++) {
    const rotated = rows.slice(index).concat(rows.slice(0, index));
    await page.evaluate(rows => window.tableContract.render(rows), rotated);
    await expect(page.locator('tbody tr td:first-child')).toHaveText(['two', 'ten', 'text']);
    await expect(page.locator('th').filter({ hasText: /^Value$/ })).toHaveAttribute('aria-sort', 'ascending');
    expect(await original.evaluate(element => element.isConnected)).toBe(true);
  }
  await page.getByRole('button', { name: 'Value', exact: true }).press('Enter');
  await expect(page.locator('tbody tr td:first-child')).toHaveText(['text', 'ten', 'two']);
  await expect(page.locator('th').filter({ hasText: /^Value$/ })).toHaveAttribute('aria-sort', 'descending');
  expect(diagnostics).toEqual([]);
});
