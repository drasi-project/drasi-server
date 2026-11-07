import * as assert from 'assert';
import axios from 'axios';
import { DrasiClient } from '../../drasi-client';

const id = 'b20c42ee-59a1-441f-a549-87a72be1f1e9';
const otherId = 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481';
const base = 'http://localhost:8080/api/v1/instances/main';

type Request = { method: string; base: string; path: string; body?: Record<string, unknown> };
type Reply = { status?: number; data: unknown };

suite('Component UUIDs and names', () => {
  const originalAdapter = axios.defaults.adapter;
  let requests: Request[];
  let reply: (request: Request) => Reply;
  let management: (request: Request) => Reply;
  let connection = { id: 'server', name: 'Server', url: 'http://localhost:8080', instanceId: 'main' };
  let client: DrasiClient;

  setup(() => {
    requests = [];
    connection = { id: 'server', name: 'Server', url: 'http://localhost:8080', instanceId: 'main' };
    client = new DrasiClient({ getCurrentConnection: () => connection });
    reply = () => { throw new Error('Unexpected request'); };
    management = () => ({ data: { success: true, data: {
      revision: 0, persistent: false, desired: { topology: { components: [] }, ordinary: [] },
    } } });
    axios.defaults.adapter = async (config) => {
      const request: Request = {
        method: config.method ?? 'get',
        base: config.baseURL ?? '',
        path: config.url ?? '',
        body: typeof config.data === 'string' ? JSON.parse(config.data) : config.data,
      };
      requests.push(request);
      const result = request.path === '/computation/desired' ? management(request) : reply(request);
      return {
        data: result.data, status: result.status ?? 200, statusText: 'Response',
        headers: {}, config,
      };
    };
  });

  teardown(() => { axios.defaults.adapter = originalAdapter; });
  const lastRequest = () => requests[requests.length - 1];

  test('name-only authoring creates once and returns the assigned UUID', async () => {
    reply = (request) => request.method === 'get'
      ? { status: 404, data: { code: 'COMPUTATION_NOT_FOUND' } }
      : { data: { success: true, data: { id, name: 'orders' } } };
    assert.deepStrictEqual(
      await client.applySource({ kind: 'Source', name: 'orders', spec: { name: 'orders', kind: 'mock' } }),
      { id, name: 'orders' }
    );
    assert.strictEqual(requests[0].path, '/components/by-name/orders');
    assert.strictEqual(lastRequest().method, 'post');
    assert.deepStrictEqual(lastRequest().body, { name: 'orders', kind: 'mock' });
  });

  test('a UUID-shaped name selects by name, then updates by the resolved UUID', async () => {
    reply = (request) => request.method === 'get'
      ? { data: { success: true, data: { id: otherId, name: id } } }
      : { data: { success: true } };
    assert.deepStrictEqual(await client.applyQuery({ name: id, query: 'RETURN 1' }), { id: otherId, name: id });
    assert.strictEqual(requests[0].path, `/components/by-name/${id}`);
    assert.strictEqual(lastRequest().path, `/queries/${otherId}`);
    assert.strictEqual(lastRequest().body?.id, otherId);
  });

  test('an explicit UUID never falls back to the same-named component', async () => {
    reply = (request) => request.method === 'get'
      ? { status: 404, data: { code: 'SOURCE_NOT_FOUND' } }
      : { status: 400, data: { code: 'EXTERNAL_COMPONENT_ID_REJECTED', message: 'External component IDs are rejected' } };
    await assert.rejects(client.applySource({ id, name: 'orders', kind: 'mock' }), /External component IDs are rejected/);
    assert.deepStrictEqual(requests.map((request) => request.path), [`/sources/${id}`, '/computation/desired', '/sources']);
    assert.strictEqual(lastRequest().body?.id, id);
  });

  test('explicit query creation uses POST and keeps the requested UUID', async () => {
    reply = (request) => request.method === 'get'
      ? { status: 404, data: { code: 'QUERY_NOT_FOUND' } }
      : { data: { success: true, data: { id, name: 'orders' } } };
    assert.deepStrictEqual(await client.applyQuery({ id, name: 'orders', query: 'RETURN 1' }), { id, name: 'orders' });
    assert.strictEqual(lastRequest().method, 'post');
    assert.strictEqual(lastRequest().body?.id, id);
  });

  test('lookup failure is not treated as permission to create', async () => {
    reply = () => ({ status: 404, data: { code: 'INSTANCE_NOT_FOUND', message: 'Instance removed' } });
    await assert.rejects(client.applyReaction({ name: 'alerts', kind: 'log', queries: ['orders'] }), /Instance removed/);
    assert.strictEqual(requests.length, 1);
  });

  test('malformed or conflicting identities reject before HTTP', async () => {
    await assert.rejects(client.applyQuery({ id: 'orders', name: 'orders' }), /must be a UUID/);
    await assert.rejects(client.applySource({
      kind: 'Source', id, name: 'orders', spec: { id: otherId, name: 'orders', kind: 'mock' },
    }), /UUIDs disagree/);
    await assert.rejects(client.applySource({
      kind: 'Source', name: 'orders', spec: { name: 'other', kind: 'mock' },
    }), /names disagree/);
    assert.strictEqual(requests.length, 0);
  });

  test('rename is metadata-only, without source replacement', async () => {
    management = () => ({ status: 400, data: { code: 'MANAGED_CONFIGURATION_REQUIRED' } });
    reply = (request) => request.method === 'get'
      ? { status: 400, data: { code: 'MANAGED_CONFIGURATION_REQUIRED' } }
      : { data: { success: true, data: { id, name: 'new-name' } } };
    await client.renameComponent(id, 'new-name');
    assert.deepStrictEqual(requests.map((request) => request.path), ['/computation/desired', `/components/${id}/rename`]);
    assert.deepStrictEqual(requests[1].body, { name: 'new-name' });
  });

  test('managed rename preserves revision checks and the selected instance', async () => {
    management = () => {
        connection = { ...connection, url: 'http://localhost:9999', instanceId: 'other' };
        return { data: { success: true, data: {
          revision: 7, persistent: true, desired: { topology: { components: [] }, ordinary: [{ id }] },
        } } };
    };
    reply = () => ({ data: { success: true } });
    await client.renameComponent(id, 'new-name');
    assert.deepStrictEqual(requests.map((request) => request.base), [base, base]);
    assert.strictEqual(requests[1].body?.expectedRevision, 7);
    assert.match(String(requests[1].body?.requestId), /^[0-9a-f-]{36}$/);
  });

  test('name resolution cannot move an in-flight apply to a different server', async () => {
    reply = (request) => {
      if (request.method === 'get') {
        connection = { ...connection, url: 'http://localhost:9999', instanceId: 'other' };
        return { data: { success: true, data: { id, name: 'orders' } } };
      }
      return { data: { success: true } };
    };
    await client.applySource({ name: 'orders', kind: 'mock' });
    assert.deepStrictEqual(requests.map((request) => request.base), [base, base, base]);
    assert.strictEqual(lastRequest().path, `/sources/${id}`);
  });

  test('viewed configuration always includes the current graph UUID and name', async () => {
    reply = () => ({ data: { success: true, data: {
      id, name: 'renamed', status: 'Running', config: { kind: 'mock' },
    } } });
    assert.deepStrictEqual(await client.getSourceConfig(id), { id, name: 'renamed', kind: 'mock' });
    reply = () => ({ data: { success: true, data: { id, name: 'pending', status: 'Starting' } } });
    await assert.rejects(client.getSourceConfig(id), /Configuration is not available/);
  });

  test('persistent creation submits an ID-omitting revisioned definition', async () => {
    management = () => ({ data: { success: true, data: {
      revision: 9, persistent: true, desired: { topology: { components: [] }, ordinary: [] },
    } } });
    reply = request => request.method === 'get'
      ? { status: 404, data: { code: 'COMPUTATION_NOT_FOUND' } }
      : { data: { success: true, data: { components: [{ id, name: 'orders' }] } } };
    assert.deepStrictEqual(await client.applySource({ name: 'orders', kind: 'mock' }), { id, name: 'orders' });
    assert.strictEqual(lastRequest().path, '/computation/ordinary-definitions');
    assert.strictEqual(lastRequest().body?.expectedRevision, 9);
    assert.deepStrictEqual(lastRequest().body?.component, {
      type: 'source', definition: { name: 'orders', kind: 'mock' },
    });
  });

  test('persistent updates and deletion retain UUID and revision checks', async () => {
    management = () => ({ data: { success: true, data: {
      revision: 10, persistent: true, desired: { topology: { components: [] }, ordinary: [{ id }] },
    } } });
    reply = request => request.method === 'get'
      ? { data: { success: true, data: { id, name: 'orders' } } }
      : { data: { success: true } };
    await client.applySource({ id, name: 'orders', kind: 'mock' });
    assert.strictEqual(lastRequest().path, `/computation/ordinary-definitions/${id}`);
    assert.strictEqual(lastRequest().method, 'put');
    assert.strictEqual(lastRequest().body?.expectedRevision, 10);
    await client.deleteSource(id);
    assert.strictEqual(lastRequest().path, `/computation/ordinary-definitions/${id}`);
    assert.strictEqual(lastRequest().method, 'delete');
    assert.strictEqual(lastRequest().body?.expectedRevision, 10);
  });
});
