import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdir, readFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../ui/node_modules/playwright/index.mjs';

const dist = fileURLToPath(new URL('../ui/dist/', import.meta.url));
const artifacts = fileURLToPath(new URL('../artifacts/', import.meta.url));
await mkdir(artifacts, { recursive: true });
await readFile(resolve(dist, 'about.html'));
const server = createServer(async (request, response) => {
  const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
  const path = resolve(dist, `.${pathname === '/' ? '/index.html' : pathname}`);
  if (!path.startsWith(`${dist.replace(/\/$/, '')}${sep}`)) {
    response.writeHead(403).end();
    return;
  }
  try {
    const bytes = await readFile(path);
    response.setHeader('Content-Type', { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' }[extname(path)] ?? 'application/octet-stream');
    response.end(bytes);
  } catch (error) {
    response.writeHead(error.code === 'ENOENT' ? 404 : 500).end();
  }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
const base = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1050 } });
  const page = await context.newPage();
  const errors = [], backend = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (/\/api\/|\/commands|\/events(?:\?|$)/.test(request.url())) backend.push(request.url());
  });
  await page.goto(`${base}/about.html`);
  assert.equal(await page.title(), 'Obstacle Impact | Drasi');
  await page.getByRole('heading', { level: 1, name: 'Obstacle Impact', exact: true }).waitFor();
  assert.equal(await page.getByRole('link', { name: 'Back to the demo' }).count(), 0);
  assert.equal(await page.getByRole('button', { name: 'Close architecture overview' }).count(), 1);
  const closeSize = await page.getByRole('button', { name: 'Close architecture overview' }).boundingBox();
  assert.ok(closeSize.width >= 44 && closeSize.height >= 44, 'The X must have a prominent, touch-sized target');
  const closeContrast = await page.getByRole('button', { name: 'Close architecture overview' }).evaluate(element => {
    const style = getComputedStyle(element);
    const luminance = color => {
      const [r,g,b] = color.match(/\d+/g).slice(0,3).map(Number).map(channel => {
        const value = channel / 255;
        return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
      });
      return .2126*r + .7152*g + .0722*b;
    };
    const ink = luminance(style.color), background = luminance(style.backgroundColor);
    return (Math.max(ink,background)+.05) / (Math.min(ink,background)+.05);
  });
  assert.ok(closeContrast >= 4.5, 'The close icon must contrast clearly with its button background');
  assert.equal(await page.locator('.architecture-header > span').count(), 0, 'No top-right title');
  assert.equal(await page.getByText('A CHANGE-PROCESSING PLATFORM, NOT A SIMULATION', { exact: true }).count(), 0);
  const titleBounds = await page.locator('#architecture-title').boundingBox();
  const subtitle = page.getByText('Architecture Overview', { exact: true });
  const subtitleBounds = await subtitle.boundingBox();
  assert.ok(subtitleBounds && subtitleBounds.y >= titleBounds.y + titleBounds.height && subtitleBounds.x === titleBounds.x, 'Architecture Overview sits below the demo title');
  const boardBounds = await page.locator('.architecture-board').boundingBox();
  assert.ok(titleBounds.y < 45 && boardBounds.y < 120, 'The compact header must bring the architecture close to the top');
  const nodes = page.locator('.architecture-node');
  assert.equal(await nodes.count(), 9);
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Details stay out of the initial overview');
  assert.equal(await page.locator('.architecture-connections > path').count(), 13);
  const graph = page.locator('.architecture-server'), client = nodes.filter({ hasText: 'React UI' });
  assert.equal(await graph.textContent(), 'DRASI SERVER', 'No subtitle after the host name');
  assert.equal(await page.locator('footer,.architecture-takeaway').count(), 0, 'No bottom-of-page text');
  assert.equal(await page.locator('[data-component="results"] > strong').textContent(), 'Query results outlet');
  assert.equal(await page.locator('[data-component="results"] .architecture-node-summary').textContent(), 'query-results');
  assert.equal(await page.locator('[data-component="delivery"]').count(), 0, 'REST and SSE are no longer combined');
  const hostBounds = await graph.boundingBox(), clientBounds = await client.boundingBox();
  assert.ok(clientBounds.x + clientBounds.width < hostBounds.x, 'Browser is outside the stock Server boundary');
  for (const id of ['api', 'sse']) {
    const box = await page.locator(`[data-component="${id}"]`).boundingBox();
    assert.ok(box.x >= hostBounds.x && box.x + box.width < hostBounds.x + hostBounds.width / 2, `${id} is in the left half of Drasi Server`);
  }
  for (const node of await nodes.all()) {
    const id = await node.getAttribute('data-component');
    if (id !== 'browser') {
      const box = await node.boundingBox();
      assert.ok(box.x >= hostBounds.x && box.y >= hostBounds.y && box.x + box.width <= hostBounds.x + hostBounds.width && box.y + box.height <= hostBounds.y + hostBounds.height, `${id} must be inside Drasi Server`);
    }
    await node.hover();
    const tooltip = page.getByRole('tooltip');
    await tooltip.waitFor();
    assert.equal(await node.getAttribute('aria-expanded'), 'true');
    assert.equal(await tooltip.getByRole('heading').textContent(), await node.locator(':scope > strong').textContent());
    assert.equal(await tooltip.locator('dt').count(), 2);
    assert.doesNotMatch(await tooltip.innerText(), /\bmetres?\b|\bmeters?\b|\b\d+(?:\.\d+)? m\b/i);
    const box = await tooltip.boundingBox();
    assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 1440 && box.y + box.height <= 1050, `${id} details fit on screen`);
  }
  const geometry = page.locator('[data-component="geometry"]');
  await geometry.hover();
  await page.getByRole('tooltip').hover();
  await page.waitForTimeout(200);
  assert.equal(await page.getByRole('tooltip').count(), 1, 'Hover details remain open while the pointer is over them');
  await page.screenshot({ path: `${artifacts}/architecture-detail.png`, fullPage: true });
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Escape dismisses without moving the pointer');
  await page.mouse.move(0, 0);
  await geometry.focus();
  await page.getByRole('tooltip').waitFor();
  await page.keyboard.press('Tab');
  assert.equal(await page.locator('[data-component="impact"]').getAttribute('aria-expanded'), 'true', 'Keyboard focus switches details');
  await page.keyboard.press('Escape');
  await geometry.click();
  await page.mouse.move(0, 0);
  await page.waitForTimeout(200);
  assert.equal(await page.getByRole('tooltip').count(), 1, 'Click pins details');
  await geometry.click();
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Click again dismisses');
  await geometry.click();
  await page.getByRole('heading', { level: 1 }).click();
  assert.equal(await page.getByRole('tooltip').count(), 0, 'Click outside dismisses');
  await page.screenshot({ path: `${artifacts}/architecture.png`, fullPage: true });
  for (const width of [1100, 1000, 768, 390]) {
    await page.setViewportSize({ width, height: 850 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `No overflow at ${width}px`);
    for (const node of await nodes.all()) {
      const id = await node.getAttribute('data-component');
      assert.equal(await node.evaluate(element => element.scrollHeight <= element.clientHeight && element.scrollWidth <= element.clientWidth), true, `${id} content fits at ${width}px`);
      await node.focus();
      const tooltip = page.getByRole('tooltip');
      await tooltip.waitFor();
      const box = await tooltip.boundingBox();
      assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= width && box.y + box.height <= 850, `Details stay visible at ${width}px`);
      await page.keyboard.press('Escape');
    }
  }
  await page.locator('[data-component="scene"]').click();
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await page.waitForTimeout(200);
  const scrolled = await page.getByRole('tooltip').boundingBox();
  assert.ok(scrolled.y >= 0 && scrolled.y + scrolled.height <= 850, 'Pinned details stay in the viewport when their card scrolls out of view');
  await page.keyboard.press('Escape');
  await page.screenshot({ path: `${artifacts}/architecture-mobile.png`, fullPage: true });
  await page.reload();
  await page.getByRole('heading', { level: 1 }).waitFor();
  assert.deepEqual(backend, [], 'The built About page makes no backend requests');
  const touch = await browser.newContext({ viewport: { width: 390, height: 850 }, isMobile: true, hasTouch: true });
  const mobile = await touch.newPage();
  mobile.on('pageerror', error => errors.push(error.message));
  await mobile.goto(`${base}/about.html`);
  await mobile.locator('[data-component="scene"]').tap();
  await mobile.getByRole('tooltip').waitFor();
  await mobile.touchscreen.tap(4, 20);
  assert.equal(await mobile.getByRole('tooltip').count(), 0, 'Touch can open and dismiss component details');
  await mobile.getByRole('button', { name: 'Close architecture overview' }).tap();
  await mobile.getByRole('button', { name: 'Information about this demo' }).tap();
  await mobile.getByRole('dialog', { name: 'Architecture Overview' }).waitFor();
  assert.equal(mobile.url(), `${base}/`, 'Touch opens an overlay without navigating');
  await mobile.getByRole('button', { name: 'Close architecture overview' }).tap();
  assert.equal(await mobile.getByRole('dialog').count(), 0);
  await touch.close();
  await page.getByRole('button', { name: 'Close architecture overview' }).click();
  await page.getByRole('heading', { level: 1, name: 'Obstacle Impact', exact: true }).waitFor();
  await page.setViewportSize({ width: 1440, height: 900 });
  const info = page.getByRole('button', { name: 'Information about this demo', exact: true });
  const title = await page.locator('.demo-title h1').boundingBox();
  const infoBounds = await info.boundingBox();
  assert.ok(infoBounds.x >= title.x + title.width && infoBounds.y < title.y + title.height, 'Info button sits to the right of the title');
  assert.equal(infoBounds.width, infoBounds.height);
  assert.equal(await info.evaluate(element => getComputedStyle(element).borderRadius), '50%');
  assert.equal(await page.getByRole('link', { name: 'About', exact: true }).count(), 0);
  const stage = page.getByRole('button', { name: 'Context query geometry-context', exact: true });
  await stage.click();
  const floor = await page.locator('svg.scene').elementHandle();
  const navigations = [];
  page.on('framenavigated', frame => { if (!frame.parentFrame()) navigations.push(frame.url()); });
  await info.click();
  const overlay = page.getByRole('dialog', { name: 'Architecture Overview' });
  const close = overlay.getByRole('button', { name: 'Close architecture overview' });
  await overlay.waitFor();
  assert.equal(await overlay.evaluate(element => element.matches(':modal')), true);
  assert.equal(await page.evaluate(() => document.body.style.overflow), 'hidden', 'Background scrolling is locked');
  assert.equal(await close.evaluate(element => element === document.activeElement), true, 'X receives initial focus');
  await page.locator('.demo-actions button').first().evaluate(element => element.focus());
  assert.equal(await overlay.evaluate(element => element.contains(document.activeElement)), true, 'Background controls cannot receive focus');
  const overlayBounds = await overlay.boundingBox();
  assert.deepEqual(overlayBounds, { x: 0, y: 0, width: 1440, height: 900 }, 'Overlay fills the viewport');
  const fade = await overlay.evaluate(element => {
    const animation = element.getAnimations().find(item => item instanceof CSSAnimation && item.animationName === 'architecture-fade-in');
    if (!animation) throw new Error('Missing overlay fade-in');
    const duration = Number(animation.effect.getTiming().duration);
    animation.pause();
    animation.currentTime = 0;
    const start = Number(getComputedStyle(element).opacity);
    animation.currentTime = duration / 2;
    const middle = Number(getComputedStyle(element).opacity);
    animation.finish();
    return { duration, start, middle, end: Number(getComputedStyle(element).opacity), backdrop: getComputedStyle(element, '::backdrop').backgroundColor };
  });
  assert.equal(fade.duration, 220);
  assert.equal(fade.start, 0);
  assert.ok(fade.middle > 0 && fade.middle < 1, 'The underlying demo remains visible during the fade');
  assert.equal(fade.end, 1);
  assert.equal(fade.backdrop, 'rgba(0, 0, 0, 0)', 'The backdrop must not abruptly cover the demo');
  await close.focus();
  await page.keyboard.press('Shift+Tab');
  assert.equal(await overlay.evaluate(element => element.contains(document.activeElement)), true, 'Tab focus stays inside the overlay');
  await overlay.locator('[data-component="geometry"]').focus();
  await overlay.getByRole('tooltip').waitFor();
  await page.keyboard.press('Escape');
  assert.equal(await overlay.getByRole('tooltip').count(), 0, 'First Escape dismisses component details');
  assert.equal(await overlay.count(), 1, 'Dismissing component details must not dismiss the overlay');
  await page.keyboard.press('Escape');
  await page.locator('#architecture-overlay').waitFor({ state: 'detached' });
  assert.equal(await info.evaluate(element => element === document.activeElement), true, 'Closing returns focus to the info button');
  assert.equal(await page.evaluate(() => document.body.style.overflow), '');
  assert.equal(await stage.getAttribute('aria-pressed'), 'true', 'Inspector selection survives the overlay');
  assert.equal(await floor.evaluate(element => element.isConnected), true, 'The live demo is never unmounted');
  for (const width of [1920, 1440, 1100, 1000, 768, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.evaluate(() => window.scrollTo(0, 0));
    const demoTitle = await page.locator('.demo-title h1').boundingBox();
    await info.click();
    await overlay.waitFor();
    assert.deepEqual(await overlay.locator('#architecture-title').boundingBox(), demoTitle, `Title position and size must not change at ${width}px`);
    assert.equal(await overlay.evaluate(element => element.scrollWidth <= element.clientWidth), true, `Overlay has no horizontal overflow at ${width}px`);
    await overlay.evaluate(element => { element.scrollTop = element.scrollHeight; });
    const closeBounds = await close.boundingBox();
    assert.ok(closeBounds.y >= 0 && closeBounds.y < 40, 'X stays visible when the overlay scrolls');
    await close.click();
    await page.locator('#architecture-overlay').waitFor({ state: 'detached' });
    assert.equal(await info.evaluate(element => element === document.activeElement), true);
  }
  await page.evaluate(() => window.scrollTo(0, 12));
  const scrolledTitle = await page.locator('.demo-title h1').boundingBox();
  await info.click();
  await overlay.waitFor();
  assert.deepEqual(await overlay.locator('#architecture-title').boundingBox(), scrolledTitle, 'A partially scrolled title stays in place too');
  await close.click();
  await page.locator('#architecture-overlay').waitFor({ state: 'detached' });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await info.click();
  await overlay.waitFor();
  assert.equal(await overlay.evaluate(element => getComputedStyle(element).animationName), 'none', 'Reduced motion disables the overlay fade');
  await close.click();
  await page.locator('#architecture-overlay').waitFor({ state: 'detached' });
  assert.deepEqual(navigations, [], 'Opening and closing never navigates away from the demo');
  assert.deepEqual(errors, []);
  console.log('PASS: static architecture, hover/touch/keyboard details, circled title info button, full-page modal, X/Escape dismissal, focus trap/restore, scroll locking, retained demo state, no navigation and responsive layouts.');
} finally {
  if (browser) await browser.close();
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
}
