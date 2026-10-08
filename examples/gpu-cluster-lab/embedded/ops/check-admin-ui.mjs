import assert from 'node:assert/strict';

const drasi = process.env.DRASI_URL ?? 'http://localhost:8080';
async function get(path) {
  const response = await fetch(`${drasi}${path}`, { signal: AbortSignal.timeout(15_000) });
  assert.equal(response.status, 200, `${path}: HTTP ${response.status}`);
  return response;
}
async function json(path) {
  const body = await (await get(path)).json();
  assert.equal(body.success, true, `${path}: unsuccessful response`);
  return body.data;
}

const root = await fetch(drasi, { redirect: 'manual', signal: AbortSignal.timeout(15_000) });
assert.equal(root.status, 307);
assert.equal(root.headers.get('location'), '/ui/?instance=gpu-demo');
for (const path of ['/ui', '/ui/', '/ui/?instance=gpu-demo']) {
  const response = await get(path);
  assert.match(response.headers.get('content-type') ?? '', /text\/html/);
  const html = await response.text();
  assert.match(html, /<title>Drasi Server<\/title>/);
  const assets = [...html.matchAll(/(?:src|href)="(\/ui\/[^"]+\.(?:js|css))"/g)]
    .map(match => match[1]);
  assert.ok(assets.some(asset => asset.endsWith('.js')), 'Missing built JavaScript');
  assert.ok(assets.some(asset => asset.endsWith('.css')), 'Missing built CSS');
  for (const asset of assets) {
    const result = await get(asset);
    assert.match(result.headers.get('content-type') ?? '',
      asset.endsWith('.css') ? /text\/css/ : /(?:text|application)\/javascript/);
    assert.ok((await result.arrayBuffer()).byteLength > 0, `${asset}: empty asset`);
  }
}
const missing = await fetch(`${drasi}/ui/assets/missing-admin-ui-test.js`,
  { signal: AbortSignal.timeout(15_000) });
assert.equal(missing.status, 404, 'Missing assets must not return HTML');
const instances = await json('/api/v1/instances');
assert.deepEqual(instances.map(instance => instance.id), ['gpu-demo']);
const base = '/api/v1/instances/gpu-demo';
for (const kind of ['sources', 'queries']) {
  const components = await json(`${base}/${kind}`);
  assert.ok(components.length > 0, `No ${kind} found`);
  for (const component of components.filter(component => !component.id.startsWith('__'))) {
    const full = await json(`${base}/${kind}/${encodeURIComponent(component.id)}?view=full`);
    assert.equal(full.id, component.id);
    assert.ok(full.config, `${component.id}: missing configuration`);
  }
}
const results = await json(`${base}/queries/ui-status/results`);
assert.ok(results.length > 0, 'No live status query results');
const computation = await json(`${base}/computation`);
assert.ok(computation.components.some(component => component.id === 'simulator'));
assert.ok(computation.components.some(component => component.id === 'gpu-demo-ui'
  && component.role === 'Sink' && component.implementation.name === 'drasi.network/sse-sink'));
console.log('Drasi admin UI passed: HTML, bundled JS/CSS, live component details, query results and computation inspection; no state mutation.');
