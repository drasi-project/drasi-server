// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import { after, before, test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { accumulateResult, DrasiError, type ResultAdapter, type RowKey } from '@drasi/react/client';

let adapter: ResultAdapter;
let probeKey: RowKey;
let directory: string;
let template: string;
const context = { receivedAt: 123, instanceId: 'cold-chain' };
before(async () => {
  const document = await readFile(new URL('../../docs/reference.md', import.meta.url), 'utf8');
  const recipe = /```tsx\n(\/\/ @drasi-docs: templated-reaction\.tsx\n[\s\S]*?)\n```/.exec(document);
  assert(recipe, 'The literal documented template adapter must exist');
  const yaml = /```yaml\n# @drasi-docs: templated-reaction\.yaml\n([\s\S]*?)\n```/.exec(document);
  assert(yaml, 'The matching literal SSE reaction must exist');
  template = yaml[1];
  const root = fileURLToPath(new URL('../.runtime/', import.meta.url));
  await mkdir(root, { recursive: true });
  directory = await mkdtemp(join(root, 'template-doc-test-'));
  const path = join(directory, 'recipe.tsx');
  await writeFile(path, recipe[1]);
  const recipeModule: { createProbeAdapter: () => ResultAdapter; probeKey: RowKey } = await import(pathToFileURL(path).href);
  adapter = recipeModule.createProbeAdapter();
  probeKey = recipeModule.probeKey;
});
after(async () => {
  if (directory) await rm(directory, { recursive: true, force: true });
});

test('the literal guide adapter normalizes JSON upserts, sparse key-changing updates and deletes without cross-query routing', () => {
  const key = 'probe-"\\\n<&>';
  const row = { probeId: key, temperatureC: 3 };
  const changed = { probeId: `${key}-new`, temperatureC: 8 };
  const records = [
    { queryId: 'north-room', op: 'upsert', after: row },
    { queryId: 'south-room', op: 'upsert', after: row },
    { queryId: 'north-room', op: 'update', before: { probeId: key }, after: changed },
    { queryId: 'north-room', op: 'delete', before: { probeId: changed.probeId } },
  ];
  const deltas = records.map(record => adapter(JSON.parse(JSON.stringify(record)), context));
  assert.deepEqual(deltas, [
    [{ kind: 'delta', queryId: 'north-room', receivedAt: 123, changes: [{ kind: 'upsert', after: row }] }],
    [{ kind: 'delta', queryId: 'south-room', receivedAt: 123, changes: [{ kind: 'upsert', after: row }] }],
    [{ kind: 'delta', queryId: 'north-room', receivedAt: 123, changes: [{ kind: 'update', before: { probeId: key }, after: changed }] }],
    [{ kind: 'delta', queryId: 'north-room', receivedAt: 123, changes: [{ kind: 'delete', before: { probeId: changed.probeId } }] }],
  ]);
  let north = accumulateResult([], deltas[0][0], probeKey);
  const south = accumulateResult([], deltas[1][0], probeKey);
  north = accumulateResult(north, deltas[2][0], probeKey);
  assert.deepEqual(north, [changed]);
  north = accumulateResult(north, deltas[3][0], probeKey);
  assert.deepEqual(north, []);
  assert.deepEqual(south, [row]);
});

test('the guide accepts only valid standard heartbeats without inventing a query route', () => {
  assert.deepEqual(adapter(JSON.parse('{"type":"heartbeat","ts":123}'), context), []);
  for (const ts of [undefined, null, '123', Infinity]) {
    assert.throws(() => adapter({ type: 'heartbeat', ts }, context), DrasiError);
  }
});

test('the guide rejects missing or unknown routes, malformed operations and missing raw keys', () => {
  for (const payload of [
    null, [], 'not an object', {},
    { op: 'upsert', after: { probeId: 'p' } },
    { queryId: 'other', op: 'upsert', after: { probeId: 'p' } },
    { queryId: 'north-room', op: 'ADD', after: { probeId: 'p' } },
    { queryId: 'north-room', op: 'upsert', after: [] },
    { queryId: 'north-room', op: 'upsert', after: { probeId: ' ' } },
    { queryId: 'north-room', op: 'delete', before: { temperatureC: 3 } },
    { queryId: 'north-room', op: 'update', before: {}, after: { probeId: 'p' } },
    { queryId: 'north-room', op: 'update', before: { probeId: 'p' }, after: {} },
  ]) assert.throws(() => adapter(payload, context), DrasiError);
});

test('released SSE renders the literal YAML as escaped JSON and the documented adapter consumes it', {
  skip: !process.env.P7_TEMPLATE_ENDPOINTS, timeout: 30_000,
}, async () => {
  const endpoints = JSON.parse(process.env.P7_TEMPLATE_ENDPOINTS!);
  for (const url of [endpoints.rest, endpoints.feed]) {
    assert(typeof url === 'string' && /^http:\/\/127\.0\.0\.1:\d+$/.test(url), 'Only the owned test runtime is allowed');
  }
  const listener = createServer();
  await new Promise<void>((accept, reject) => {
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', accept);
  });
  const address = listener.address();
  assert(address && typeof address !== 'string');
  await new Promise<void>((accept, reject) => listener.close(error => error ? reject(error) : accept()));
  const reactionUrl = `${endpoints.rest}/api/v1/instances/cold-chain/reactions/template-contract`;
  const signal = AbortSignal.timeout(20_000);
  const abort = new AbortController();
  let created = false;
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  const wire: string[] = [];
  try {
    const config = template.replace('id: cold-chain-events\n', 'id: template-contract\n')
      .replace('port: 8081\n', `port: ${address.port}\n`);
    assert.notEqual(config, template);
    const create = await fetch(`${endpoints.rest}/api/v1/instances/cold-chain/reactions`, {
      method: 'POST', headers: { 'Content-Type': 'application/yaml' }, body: config, signal,
    });
    assert(create.ok, await create.text());
    created = true;
    let stream: Response;
    for (;;) {
      try {
        stream = await fetch(`http://127.0.0.1:${address.port}/events`, {
          signal: AbortSignal.any([signal, abort.signal]),
        });
        break;
      } catch (error) {
        if (!(error instanceof TypeError && error.cause &&
          typeof error.cause === 'object' && 'code' in error.cause && error.cause.code === 'ECONNREFUSED')) throw error;
        await delay(100, undefined, { signal });
      }
    }
    assert.equal(stream.status, 200);
    assert.match(stream.headers.get('content-type') ?? '', /text\/event-stream/);
    assert(stream.body);
    reader = stream.body.getReader();
    let buffered = '';
    const decoder = new TextDecoder();
    const key = 'template-"\\\n<&>';
    async function nextChange() {
      for (;;) {
        const end = buffered.indexOf('\n\n');
        if (end < 0) {
          const chunk = await reader!.read();
          assert(!chunk.done, 'The real SSE stream ended early');
          buffered += decoder.decode(chunk.value, { stream: true }).replaceAll('\r\n', '\n');
          continue;
        }
        const event = buffered.slice(0, end);
        buffered = buffered.slice(end + 2);
        const data = event.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
        if (!data) continue;
        wire.push(data);
        const deltas = adapter(JSON.parse(data), context);
        const delta = deltas.find(value => value.changes.some(change =>
          probeKey(change.kind === 'delete' ? change.before : change.after).startsWith(key)));
        if (delta) return delta;
      }
    }
    for (const room of ['North', 'South']) {
      const queryId = `${room.toLowerCase()}-room`;
      const row = { probeId: key, room, temperatureC: 3 };
      const changed = { ...row, probeId: `${key}-new`, temperatureC: 8 };
      let rows: Record<string, unknown>[] = [];
      for (const [operation, properties, expected] of [
        ['insert', row, { kind: 'upsert', after: row }],
        ['update', changed, { kind: 'update', before: row, after: changed }],
        ['delete', changed, { kind: 'delete', before: { probeId: changed.probeId } }],
      ] as const) {
        const id = `template-${room.toLowerCase()}`;
        const response = await fetch(`${endpoints.feed}/sources/probe-feed/events`, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, signal,
          body: JSON.stringify(operation === 'delete'
            ? { operation, id, labels: ['Probe'], timestamp: Date.now() * 1_000_000 }
            : { operation, element: { type: 'node', id, labels: ['Probe'], properties }, timestamp: Date.now() * 1_000_000 }),
        });
        assert(response.ok, await response.text());
        const delta = await nextChange();
        assert.equal(delta.queryId, queryId);
        assert.deepEqual(delta.changes, [expected]);
        rows = accumulateResult(rows, delta, probeKey);
        assert.deepEqual(rows, operation === 'delete' ? [] : [properties]);
      }
    }
    console.log(`Verified actual SSE template JSON: ${JSON.stringify(wire)}`);
  } finally {
    if (!signal.aborted) await reader?.cancel();
    abort.abort();
    if (created) {
      const response = await fetch(reactionUrl, { method: 'DELETE', signal: AbortSignal.timeout(5000) });
      assert(response.ok, await response.text());
    }
  }
});
