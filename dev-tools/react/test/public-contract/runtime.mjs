// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const [kind, format] = process.argv.slice(2);
assert(['client', 'react', 'components', 'root'].includes(kind));
assert(['esm', 'cjs'].includes(format));
const require = createRequire(import.meta.url);
const specifier = kind === 'root' ? '@drasi/react' : `@drasi/react/${kind}`;
const load = name => format === 'esm' ? import(name) : require(name);
const resolution = format === 'esm' ? fileURLToPath(import.meta.resolve(specifier)) : require.resolve(specifier);
assert(resolution.endsWith(format === 'esm' ? '.js' : '.cjs'), `Wrong runtime export: ${resolution}`);

// Server-rendering peers are test infrastructure, not part of the no-React run.
const React = kind === 'client' ? null : await load('react');
const server = kind === 'client' ? null : await load('react-dom/server');
if (kind !== 'client') await load('react-dom');
const originalGlobals = new Map();
const accesses = [];
for (const name of ['window', 'document', 'EventSource', 'fetch']) {
  originalGlobals.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
  Object.defineProperty(globalThis, name, {
    configurable: true,
    get() {
      accesses.push(name);
      throw new Error(`Import/render unexpectedly accessed global ${name}`);
    },
  });
}
const noNetwork = () => {
  accesses.push('fetch()');
  throw new Error('Import/construction/render unexpectedly performed a request');
};
function setFetch(fetcher) {
  Object.defineProperty(globalThis, 'fetch', { configurable: true, writable: true, value: fetcher });
}
const options = {
  serverUrl: 'https://drasi.example',
  instanceId: 'warehouse-west',
  queryIds: ['temperatures'],
  reaction: { id: 'warehouse-events', endpoint: 'https://events.example/warehouse' },
};

try {
  const api = await load(specifier);
  assert.deepEqual(accesses, [], `${specifier} import touched browser/network globals`);
  setFetch(noNetwork);
  if (kind === 'client') {
    for (const name of ['DrasiClient', 'DrasiSSEClient', 'DrasiError']) {
      assert.equal(typeof api[name], 'function', `Missing ${name} from client export`);
    }
    const client = new api.DrasiClient(options);
    const sse = new api.DrasiSSEClient();
    assert.equal(client.isInitialized(), false);
    assert.equal(client.getConnectionStatus().connected, false);
    await client.disconnect();
    await sse.disconnect();
    assert.deepEqual(accesses, [], 'Client construction/disconnect performed browser/network work');

    const queryPath = '/api/v1/instances/warehouse-west/queries/temperatures';
    const reactionPath = '/api/v1/instances/warehouse-west/reactions/warehouse-events';
    const query = {
      id: 'temperatures', status: 'Running',
      links: { self: queryPath, full: `${queryPath}?view=full` },
      config: {
        id: 'temperatures', query: 'MATCH (s:Station) RETURN s.stationId AS stationId',
        queryLanguage: 'Cypher', sources: [], autoStart: false, middleware: [],
        enableBootstrap: true, bootstrapBufferSize: 100, outboxCapacity: 100, bootstrapTimeoutSecs: 30,
      },
    };
    const reaction = {
      id: 'warehouse-events', status: 'Running',
      links: { self: reactionPath, full: `${reactionPath}?view=full` },
      config: {
        id: 'warehouse-events', kind: 'sse', queries: ['temperatures'],
        host: '0.0.0.0', port: 9000, ssePath: '/warehouse', heartbeatIntervalMs: 30000,
      },
    };
    const rows = [{ stationId: 'cold-room', celsius: 3 }];
    for (const transport of ['injected', 'global']) {
      for (const authentication of ['default', 'static', 'provider']) {
        const requests = [];
        let headerCalls = 0;
        const headers = { 'X-Consumer': 'packed-rest-contract' };
        const auth = authentication === 'default' ? {} : {
          credentials: authentication === 'static' ? 'omit' : 'include',
          headers: authentication === 'static' ? headers : async () => {
            headerCalls += 1;
            return headers;
          },
        };
        const fetcher = async (url, init) => {
          const path = new URL(url).pathname;
          assert.equal(init.method, 'GET');
          assert.equal(init.credentials, auth.credentials ?? 'same-origin');
          assert.equal(new Headers(init.headers).get('X-Consumer'),
            authentication === 'default' ? null : headers['X-Consumer']);
          assert(path.startsWith('/api/v1/instances/warehouse-west/'));
          requests.push(path);
          const data = path.endsWith('/results') ? rows : path.includes('/reactions/') ? reaction : query;
          return new Response(JSON.stringify({ success: true, data }), { headers: { 'content-type': 'application/json' } });
        };
        setFetch(transport === 'global' ? fetcher : noNetwork);
        // Authenticated REST-only inspection needs no custom stream factory.
        const rest = new api.DrasiClient({ ...options, ...auth, ...(transport === 'injected' ? { fetch: fetcher } : {}) });
        assert.equal(requests.length, 0, 'REST client construction made a request');
        assert.equal(headerCalls, 0, 'Construction executed an authentication provider');
        assert.deepEqual(await rest.getQuery('temperatures'), query);
        assert.deepEqual(await rest.getQueryResults('temperatures'), rows);
        assert.deepEqual(await rest.getReaction(), reaction);
        assert.equal(requests.length, 4, 'REST calls did not use the selected fetch implementation');
        assert.equal(headerCalls, authentication === 'provider' ? requests.length : 0);
        assert.equal(rest.getConnectionStatus().connected, false, 'REST reads opened an SSE stream');
        await rest.disconnect();
      }
    }
    setFetch(noNetwork);
    const eventSourceTrap = Object.getOwnPropertyDescriptor(globalThis, 'EventSource');
    Object.defineProperty(globalThis, 'EventSource', { configurable: true, value: undefined });
    const browserStream = new api.DrasiSSEClient({ maxReconnectAttempts: 0 });
    try {
      await assert.rejects(browserStream.connect(['temperatures'], options.reaction.endpoint),
        error => error instanceof api.DrasiError && error.code === 'INVALID_CONFIGURATION');
    } finally {
      await browserStream.disconnect();
      Object.defineProperty(globalThis, 'EventSource', eventSourceTrap);
    }
    for (const file of Object.keys(require.cache)) {
      assert(!/[/\\]node_modules[/\\](?:@types[/\\])?react(?:-dom)?(?:[/\\]|$)/.test(file),
        `Client import loaded React: ${file}`);
    }
  } else {
    const hooks = kind === 'components' ? await load('@drasi/react/react') : api;
    function HookConsumer() {
      const result = hooks.useDrasiQuery('temperatures');
      const definition = hooks.useDrasiQueryDefinition('temperatures');
      assert.equal(hooks.useDrasiClient().initialized, false);
      assert.equal(hooks.useDrasiConnectionStatus().connected, false);
      assert.equal(hooks.useDrasiServerUiUrl(), null);
      assert.equal(result.data, null);
      assert.equal(definition.config, null);
      return React.createElement('span', null, 'idle SSR consumer');
    }
    const child = kind === 'react'
      ? React.createElement(HookConsumer)
      : React.createElement(React.Fragment, null,
        React.createElement(HookConsumer),
        React.createElement(api.QueryTable, {
          queryId: 'temperatures', title: 'Warehouse temperatures',
          columns: [{ key: 'stationId', label: 'Station' }],
          rowKey: row => row.stationId,
        }),
        React.createElement(api.CodeViewerDialog, {
          isOpen: false, onClose() {}, title: 'Definition', reactCode: '', cypherQuery: '',
        }));
    const html = server.renderToString(React.createElement(hooks.DrasiProvider, options, child));
    assert(html.includes('idle SSR consumer'));
    if (kind !== 'react') assert(html.includes('Warehouse temperatures'));
    if (kind === 'root') {
      assert.equal(typeof api.DrasiClient, 'function');
      assert.equal(typeof api.QueryTable, 'function');
    }
  }
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.deepEqual(accesses, [], `${specifier} performed implicit DOM/network work`);

  // CSS is an explicit asset export, not JavaScript that Node can execute.
  const esmStyle = fileURLToPath(import.meta.resolve('@drasi/react/styles.css'));
  const cjsStyle = require.resolve('@drasi/react/styles.css');
  assert.equal(esmStyle, cjsStyle);
  assert((await readFile(esmStyle, 'utf8')).includes('.drasi-query-table'));
  for (const privatePath of ['@drasi/react/src/index.ts', '@drasi/react/dist/index.js']) {
    assert.throws(() => require.resolve(privatePath), error => error.code === 'ERR_PACKAGE_PATH_NOT_EXPORTED');
    assert.throws(() => import.meta.resolve(privatePath), error => error.code === 'ERR_PACKAGE_PATH_NOT_EXPORTED');
  }
  const proof = kind === 'client' ? 'idle construction; browser-free Node REST' : 'idle import and SSR';
  console.log(`Packed ${specifier} ${format}: ${proof}; explicit CSS resolves`);
} finally {
  for (const [name, descriptor] of originalGlobals) {
    if (descriptor) Object.defineProperty(globalThis, name, descriptor);
    else delete globalThis[name];
  }
}
