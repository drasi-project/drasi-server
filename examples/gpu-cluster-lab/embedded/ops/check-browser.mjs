import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { queryIds, rowKey, validateRow } from '../../shared/ui/src/rows.ts';

assert.ok(process.env.PLAYWRIGHT_MODULE, 'An installed Playwright module is required');
assert.ok(process.env.EVIDENCE_DIR, 'A private evidence directory is required');
assert.ok(process.env.DEMO_URL, 'The isolated control URL is required');
const demo = process.env.DEMO_URL;
const evidence = path.resolve(process.env.EVIDENCE_DIR);
await mkdir(evidence, { recursive: true });
const { chromium } = await import(pathToFileURL(path.resolve(process.env.PLAYWRIGHT_MODULE)).href);
const failures = [], observed = new Set();
const browser = await chromium.launch({ headless: true });
let page;
async function rows(query) {
  const response = await fetch(`${demo}/api/v1/instances/gpu-demo/queries/${query}/results`);
  assert.equal(response.status, 200);
  const { data } = await response.json();
  assert.ok(Array.isArray(data));
  const keys = new Set();
  for (const row of data) {
    validateRow(query, row);
    const key = rowKey(query, row);
    assert.ok(!keys.has(key), `${query}: duplicate identity`);
    keys.add(key);
  }
  return data;
}
async function until(description, check) {
  const deadline = Date.now() + 45_000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(200);
  }
  assert.fail(description);
}
try {
  page = await browser.newPage({ viewport: { width: 1600, height: 1100 } });
  page.on('pageerror', error => failures.push(error.message));
  page.on('console', message => { if (message.type() === 'error') failures.push(message.text()); });
  page.on('response', response => {
    const url = new URL(response.url());
    if (url.pathname === '/events/gpu-demo' && response.status() === 200) observed.add('native-sse');
    for (const query of queryIds) {
      if (url.pathname === `/snapshot/${query}` && response.status() === 200) {
        observed.add(query);
      }
    }
    if (response.status() >= 400) failures.push(`${response.status()} ${url.pathname}`);
  });
  await page.goto(demo, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'GPU Cluster Lab', exact: true }).waitFor();
  await until('all nine real query snapshots and native SSE must reach the browser', () =>
    observed.has('native-sse') && queryIds.every(query => observed.has(query)));
  const [gpu] = await rows('ui-gpus');
  const region = page.locator(`article.cluster[data-hierarchy-id="${gpu.cluster_id}"] > details.cluster-body`);
  await region.locator(':scope > summary').click();
  const card = page.locator(`article.gpu[data-hierarchy-id="${gpu.gpu_id}"]`);
  await card.getByRole('button', { name: 'Expand GPU details', exact: true }).click();
  await card.getByRole('button', { name: 'Edit background load', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Edit background load', exact: true });
  const demand = gpu.background_compute_units === 21 ? 22 : 21;
  await dialog.locator('input[name="demand"]').fill(String(demand));
  const saved = page.waitForResponse(response => response.url().endsWith(`/api/gpus/${gpu.gpu_id}/telemetry`)
    && response.request().method() === 'PATCH');
  await dialog.getByRole('button', { name: 'Save change', exact: true }).click();
  assert.equal((await saved).status(), 200, 'A real browser CSRF-authorized command must commit');
  await until('browser command must become fresh measured and confirmed state', async () => {
    const current = (await rows('ui-gpus')).find(row => row.gpu_id === gpu.gpu_id);
    const [plan] = await rows('ui-placements');
    return current.background_compute_units === demand && current.reported_background_compute_units === demand
      && BigInt(current.telemetry_revision) > BigInt(gpu.telemetry_revision)
      && current.sample_telemetry_revision === current.telemetry_revision
      && plan.status === 'confirmed' && plan.desired_plan_version === plan.confirmed_plan_version;
  });
  await until('native SSE must update the rendered GPU without reloading', async () =>
    (await card.innerText()).includes(`+ ${demand} from background activity`));
  await page.getByRole('button', { name: 'Global analysis', exact: true }).click();
  await page.locator('#global-analysis').waitFor({ state: 'visible' });
  await page.screenshot({ path: path.join(evidence, 'browser-live.png'), fullPage: true });
  assert.deepEqual(failures, [], 'Browser console/network failures');
  await writeFile(path.join(evidence, 'browser-result.json'), `${JSON.stringify({
    passed: true, observed: [...observed], command: { gpu_id: gpu.gpu_id, demand },
    consoleErrors: failures,
  }, null, 2)}\n`);
  console.log('Real browser passed: nine native snapshots, SSE, CSRF command, fresh measured confirmation and rendered updates without reload.');
} catch (error) {
  if (page) {
    await page.screenshot({ path: path.join(evidence, 'browser-failure.png'), fullPage: true }).catch(() => {});
    await writeFile(path.join(evidence, 'browser-failure.txt'), `${error.stack}\n${failures.join('\n')}\n${await page.locator('body').innerText()}`);
  }
  throw error;
} finally {
  await browser.close();
}
