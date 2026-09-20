// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { access, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { probes, sendReading } from '../scripts/feed.mjs';
import { serveExample } from '../scripts/web.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const bounded = { timeout: 5000 };
const get = (url, init = {}) => fetch(url, { signal: AbortSignal.timeout(2000), ...init });

async function directory(t, prefix = 'node-tests-') {
  await mkdir(join(root, '.runtime'), { recursive: true });
  const path = await mkdtemp(join(root, '.runtime', prefix));
  t.after(() => rm(path, { recursive: true, force: true }));
  return path;
}
async function assets(t) {
  const path = await directory(t);
  const dist = join(path, 'dist');
  await mkdir(join(dist, 'assets'), { recursive: true });
  const files = {
    'index.html': '<!doctype html><title>Owned live entry</title>',
    'hooks.html': '<!doctype html><title>Owned hooks entry</title>',
    'showcase.html': '<!doctype html><title>Owned simulated entry</title>',
    'assets/main.js': 'export const owned = true;\n',
    'assets/main.css': 'body { color: #111; }\n',
    'manifest.json': '{"owned":true}\n',
  };
  await Promise.all(Object.entries(files).map(([name, content]) => writeFile(join(dist, name), content)));
  await writeFile(join(path, 'outside.js'), 'not a public asset');
  return { dist, files };
}
async function localServer(t, handler) {
  const server = createServer(handler);
  await new Promise((accept, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => { server.off('error', reject); accept(); });
  });
  const address = server.address();
  assert(address && typeof address !== 'string');
  let closed = false;
  const close = async () => {
    if (closed) return;
    closed = true;
    server.closeAllConnections();
    await new Promise((accept, reject) => server.close(error => error ? reject(error) : accept()));
  };
  t.after(close);
  return { server, port: address.port, url: `http://127.0.0.1:${address.port}`, close };
}
async function webServer(t, dist, options = {}) {
  const web = await serveExample({ dist, port: 0, ...options });
  let closed = false;
  const close = async () => {
    if (closed) return;
    closed = true;
    await web.close();
  };
  t.after(close);
  return { ...web, close };
}

test('static handler serves the three built entries and assets with correct MIME and host headers', bounded, async t => {
  const { dist, files } = await assets(t);
  const web = await webServer(t, dist);
  for (const [name, content] of Object.entries(files)) {
    const response = await get(`${web.url}/${name === 'index.html' ? '' : name}`);
    assert.equal(response.status, 200);
    assert.equal(await response.text(), content);
    const type = name.endsWith('.html') ? 'text/html' : name.endsWith('.js') ? 'text/javascript' :
      name.endsWith('.css') ? 'text/css' : 'application/json';
    assert.equal(response.headers.get('content-type'), type);
    assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
    assert.equal(response.headers.get('x-frame-options'), 'DENY');
    assert.match(response.headers.get('content-security-policy'), /connect-src 'self'/);
    assert.match(response.headers.get('content-security-policy'), /frame-ancestors 'none'/);
  }
  assert.deepEqual(web.requests, []);
});

test('asset handler rejects traversal, malformed paths and unknown assets without SPA fallback', bounded, async t => {
  const { dist } = await assets(t);
  const web = await webServer(t, dist);
  for (const path of [
    '/%2e%2e%2foutside.js', '/%2e%2e%5coutside.js', '/%00.js',
    '/assets/missing.js', '/server.yaml', '/missing.html', '/api/v1/instances/another/queries/north-room',
  ]) {
    const response = await get(web.url + path);
    assert.equal(response.status, 404, path);
    assert.doesNotMatch(await response.text(), /not a public asset|Owned live entry/);
  }
  assert.equal((await get(`${web.url}/%ZZ.js`)).status, 400);
  assert.deepEqual(web.requests, []);
});

test('browser proxy refuses every write method before forwarding to its owned upstream', bounded, async t => {
  const { dist } = await assets(t);
  const forwarded = [];
  const upstream = await localServer(t, (request, response) => {
    forwarded.push(request.method);
    response.writeHead(200).end('unexpected forwarding');
  });
  const web = await webServer(t, dist, { rest: upstream.url, events: upstream.url });
  for (const method of ['POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']) {
    for (const path of ['/', '/events', '/api/v1/instances/cold-chain/queries']) {
      const response = await get(web.url + path, { method });
      assert.equal(response.status, 405, `${method} ${path}`);
      assert.equal(response.headers.get('allow'), 'GET');
      await response.arrayBuffer();
    }
  }
  assert.deepEqual(forwarded, []);
  assert.deepEqual(web.requests, []);
});

test('read proxy preserves actual upstream bytes, status, query strings and Accept without widening instance scope', bounded, async t => {
  const { dist } = await assets(t);
  const forwarded = [];
  const body = '{ "fixture": "unit transport bytes, not Drasi protocol evidence" }\n';
  const upstream = await localServer(t, (request, response) => {
    forwarded.push({ method: request.method, url: request.url, accept: request.headers.accept });
    response.writeHead(404, { 'Content-Type': 'application/json' }).end(body);
  });
  const web = await webServer(t, dist, { rest: upstream.url });
  const path = '/api/v1/instances/cold-chain/queries/north-room?view=full';
  const response = await get(web.url + path, { headers: { Accept: 'application/json' } });
  assert.equal(response.status, 404);
  assert.equal(await response.text(), body);
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal(response.headers.get('content-type'), 'application/json');
  assert.deepEqual(forwarded, [{ method: 'GET', url: path, accept: 'application/json' }]);
  assert.deepEqual(web.requests, [{ method: 'GET', path }]);
  assert.equal((await get(`${web.url}/api/v1/instances/other/queries/north-room`)).status, 404);
  assert.equal((await get(`${web.url}/api/v1/plugins`)).status, 404);
  assert.equal(forwarded.length, 1);
});

test('static-only mode fails visibly and proxy target validation rejects non-owned URL shapes', bounded, async t => {
  const { dist } = await assets(t);
  const web = await webServer(t, dist);
  for (const path of ['/events', '/api/v1/instances/cold-chain/queries/north-room/results']) {
    const response = await get(web.url + path);
    assert.equal(response.status, 503);
    assert.match(await response.text(), /Static showcase only/);
  }
  for (const target of [
    'https://127.0.0.1:12345', 'http://localhost:12345', 'http://127.0.0.1:12345/path',
    'http://127.0.0.1:12345?token=example', 'http://user@127.0.0.1:12345',
  ]) {
    await assert.rejects(serveExample({ dist, port: 0, rest: target }), /owned loopback/);
    await assert.rejects(serveExample({ dist, port: 0, events: target }), /owned loopback/);
  }
});

test('owned stream outage closes upstream, leaves REST alive, reopens cleanly and disposes all sockets', bounded, async t => {
  const { dist } = await assets(t);
  const active = new Set();
  let lastResponse;
  const upstream = await localServer(t, (request, response) => {
    if (request.url === '/events') {
      active.add(response);
      lastResponse = response;
      response.on('close', () => active.delete(response));
      response.writeHead(200, { 'Content-Type': 'text/event-stream' });
      response.write('data: local transport fixture only\n\n');
    } else response.writeHead(200, { 'Content-Type': 'application/json' }).end('{"alive":true}');
  });
  const web = await webServer(t, dist, { rest: upstream.url, events: upstream.url });
  const first = await get(`${web.url}/events`);
  const reader = first.body.getReader();
  assert.equal(new TextDecoder().decode((await reader.read()).value), 'data: local transport fixture only\n\n');
  assert.equal(active.size, 1);
  const ended = once(lastResponse, 'close');
  web.setStreamsAvailable(false);
  assert.equal((await reader.read()).done, true);
  reader.releaseLock();
  await ended;
  assert.equal(active.size, 0);
  assert.equal((await get(`${web.url}/events`)).status, 503);
  const rest = await get(`${web.url}/api/v1/instances/cold-chain/queries/north-room/results`);
  assert.equal(rest.status, 200);
  assert.deepEqual(await rest.json(), { alive: true });
  web.setStreamsAvailable(true);
  const second = await get(`${web.url}/events`);
  const secondReader = second.body.getReader();
  assert.equal(new TextDecoder().decode((await secondReader.read()).value), 'data: local transport fixture only\n\n');
  assert.equal(active.size, 1);
  const disposed = once(lastResponse, 'close');
  await web.close();
  assert.equal((await secondReader.read()).done, true);
  secondReader.releaseLock();
  await disposed;
  assert.equal(active.size, 0);
  await assert.rejects(get(`${web.url}/`), TypeError);
});

test('an occupied explicit web port fails without changing the existing owned listener', bounded, async t => {
  const { dist } = await assets(t);
  const existing = await localServer(t, (_request, response) => response.end('still owned by this test'));
  await assert.rejects(serveExample({ dist, port: existing.port }), error => error.code === 'EADDRINUSE');
  assert.equal(await (await get(existing.url)).text(), 'still owned by this test');
});

test('feed sends exact insert/update/delete HTTP bodies for only the two owned probe identities', bounded, async t => {
  const received = [];
  const upstream = await localServer(t, (request, response) => {
    let raw = '';
    request.on('data', chunk => { raw += chunk; });
    request.on('end', () => {
      received.push({ method: request.method, path: request.url, type: request.headers['content-type'], body: JSON.parse(raw) });
      response.writeHead(202).end();
    });
  });
  const before = Date.now() * 1_000_000;
  await sendReading(upstream.url, probes[0], 'insert');
  await sendReading(upstream.url, { ...probes[0], temperatureC: 8 });
  await sendReading(upstream.url, probes[1], 'delete');
  const after = Date.now() * 1_000_000;
  for (const record of received) {
    assert.equal(record.method, 'POST');
    assert.equal(record.path, '/sources/probe-feed/events');
    assert.equal(record.type, 'application/json');
    assert(Number.isInteger(record.body.timestamp) && record.body.timestamp >= before && record.body.timestamp <= after);
    delete record.body.timestamp;
  }
  const element = temperatureC => ({
    type: 'node', id: 'north-probe', labels: ['Probe'],
    properties: { probeId: 'probe-1001', room: 'North', temperatureC },
  });
  assert.deepEqual(received.map(record => record.body), [
    { operation: 'insert', element: element(3) },
    { operation: 'update', element: element(8) },
    { operation: 'delete', id: 'south-probe', labels: ['Probe'] },
  ]);
  assert.equal(probes[0].temperatureC, 3);
});

test('feed rejects identity mismatches, nonfinite values, invalid operations and URL shapes before any I/O', bounded, async t => {
  let requests = 0;
  const upstream = await localServer(t, (_request, response) => { requests += 1; response.end(); });
  for (const probe of [
    { ...probes[0], id: probes[1].id }, { ...probes[0], probeId: probes[1].probeId },
    { ...probes[0], room: probes[1].room }, { ...probes[0], id: 'unowned' },
  ]) await assert.rejects(sendReading(upstream.url, probe), /Only this example owns/);
  for (const temperatureC of [NaN, Infinity, -Infinity, '3', null]) {
    for (const operation of ['update', 'delete']) {
      await assert.rejects(sendReading(upstream.url, { ...probes[0], temperatureC }, operation), /finite temperature/);
    }
  }
  for (const operation of ['upsert', 'replace', '']) {
    await assert.rejects(sendReading(upstream.url, probes[0], operation), /Unsupported feed operation/);
  }
  for (const endpoint of [
    upstream.url.replace('http:', 'https:'), upstream.url.replace('127.0.0.1', 'localhost'),
    `${upstream.url}/events`, `${upstream.url}?extra=true`, `${upstream.url}#fragment`,
  ]) await assert.rejects(sendReading(endpoint, probes[0]), /owned loopback feed URL/);
  assert.equal(requests, 0);
});

test('feed surfaces non-success HTTP responses and closed-service failures instead of claiming delivery', bounded, async t => {
  const upstream = await localServer(t, (_request, response) => response.writeHead(503).end('private diagnostic'));
  await assert.rejects(sendReading(upstream.url, probes[0]), error =>
    /Feed update failed \(503\)/.test(error.message) && !error.message.includes('private diagnostic'));
  await upstream.close();
  await assert.rejects(sendReading(upstream.url, probes[0]), TypeError);
});

test('cleanup CLI refuses unfinished or wrongly owned runs and removes only the explicitly completed run', bounded, async t => {
  const path = await directory(t, 'live-nodetest');
  const protectedPath = await directory(t, 'live-neighbour');
  const clean = target => spawnSync(process.execPath, [join(root, 'scripts/clean-run.mjs'), target], {
    cwd: root, encoding: 'utf8', timeout: 2000,
  });
  for (const cleanup of [
    { owner: 'drasi-react-cold-storage', stopped: false },
    { owner: 'not-this-example', stopped: true },
  ]) {
    await writeFile(join(path, 'cleanup.json'), JSON.stringify(cleanup));
    const result = clean(path);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /unowned or unfinished/);
    await access(path);
  }
  assert.equal(clean(join(root, '.runtime')).status, 1);
  await writeFile(join(path, 'cleanup.json'), JSON.stringify({ owner: 'drasi-react-cold-storage', stopped: true }));
  const result = clean(path);
  assert.equal(result.status, 0, result.stderr);
  await assert.rejects(access(path), error => error.code === 'ENOENT');
  await access(protectedPath);
});
