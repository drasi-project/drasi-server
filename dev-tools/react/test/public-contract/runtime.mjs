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
for (const name of ['window', 'document', 'navigator', 'EventSource', 'fetch', 'requestAnimationFrame']) {
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
const queryOptions = {
  getKey: raw => {
    if (typeof raw.stationId !== 'string' || !raw.stationId) throw new Error('Missing station identity');
    return raw.stationId;
  },
  transform: raw => raw,
};

function checkProviderFreeTables(api) {
  assert.equal(typeof api.DataTable, 'function');
  assert.equal(typeof api.queryTableState, 'function');
  for (const name of ['DataTable', 'QueryTable', 'queryTableState', 'CodeIcon', 'ExpandIcon', 'CollapseIcon']) {
    assert.equal(api[name].name, name, `Public/debug component name changed: ${name}`);
  }
  assert.throws(() => api.queryTableState(null, () => {}), error => {
    assert.match(error.stack, /src[/\\]components[/\\]QueryTable\.tsx:\d+:\d+/, 'Published source map did not locate the original source');
    return error instanceof TypeError;
  });
  for (const name of ['CodeViewerDialog', 'formatQueryConfig', 'QueryInspector']) {
    assert.equal(api[name], undefined, `App-owned tutorial API leaked: ${name}`);
  }
  for (const name of ['CodeIcon', 'ExpandIcon', 'CollapseIcon']) {
    assert.equal(typeof api[name], 'function', `Optional generic icon missing: ${name}`);
  }
  const rows = Object.freeze([
    Object.freeze({ routeId: 'north', parcels: 12 }),
    Object.freeze({ routeId: 'south', parcels: 3 }),
    Object.freeze({ routeId: 'waiting', parcels: null }),
    Object.freeze({ routeId: 'east', parcels: 3 }),
  ]);
  const columns = Object.freeze([
    { key: 'routeId', label: 'Route', align: 'left' },
    { key: 'parcels', label: 'Parcels', align: 'right' },
    { key: 'summary', label: 'Summary', sortable: false, format: (_value, row) => `${row.routeId} delivery` },
  ]);
  let notifications = 0;
  const base = {
    rows, columns, rowKey: row => row.routeId, title: 'Warehouse deliveries',
    onSortChange() { notifications += 1; },
  };
  const render = props => server.renderToString(React.createElement(api.DataTable, { ...base, ...props }));
  const html = render({
    defaultSort: { column: 'parcels', direction: 'asc' },
    animateOnChange: 'parcels',
    rowAnimations: new Map([['south', 'up']]),
    headerActions: React.createElement('span', null, 'Dispatch desk'),
    headerControls: React.createElement('button', { type: 'button' }, 'Export'),
    headerSlot: React.createElement('p', null, 'App-supplied rows'),
    actions: Object.freeze([{ icon: 'Send', label: 'Dispatch', onClick() {}, disabled: row => row.parcels === null }]),
    renderHeader(context) {
      assert.deepEqual(context.rows.map(row => row.routeId), ['south', 'east', 'north', 'waiting']);
      assert.deepEqual(context.sort, { column: 'parcels', direction: 'asc' });
      assert.equal(typeof context.setSort, 'function');
      return React.createElement('header', null, context.defaultRender());
    },
    renderRow(row, definitions, animation, defaultRender) {
      assert.equal(definitions, columns);
      assert.equal(animation, row.routeId === 'south' ? 'up' : null);
      return React.createElement(React.Fragment, null,
        defaultRender(),
        React.createElement('tr', null,
          React.createElement('td', { colSpan: 4 }, `${row.routeId} detail`)));
    },
  });
  for (const text of ['Warehouse deliveries', 'south delivery', 'north detail', 'Export', 'Dispatch desk', 'App-supplied rows']) {
    assert(html.includes(text), `Provider-free supplied-row SSR omitted ${text}`);
  }
  assert(html.indexOf('south delivery') < html.indexOf('east delivery'));
  assert(html.indexOf('east delivery') < html.indexOf('north delivery'));
  assert(html.indexOf('north delivery') < html.indexOf('waiting delivery'));
  assert(html.includes('aria-sort="ascending"'));
  assert.match(html, /<td class="[^"]*drasi-align--left[^"]*">north<\/td>/);
  assert(html.includes('drasi-row--up'));
  assert(!html.includes('View code') && !html.includes('Expand table'));

  const cleared = render({ sort: null, defaultSort: { column: 'parcels', direction: 'desc' } });
  assert(cleared.indexOf('north delivery') < cleared.indexOf('south delivery'));
  assert(cleared.indexOf('waiting delivery') < cleared.indexOf('east delivery'));
  assert(!cleared.includes('aria-sort='));
  const descending = render({ sort: { column: 'parcels', direction: 'desc' } });
  assert(descending.indexOf('waiting delivery') < descending.indexOf('north delivery'));
  assert(descending.indexOf('north delivery') < descending.indexOf('south delivery'));
  assert(descending.indexOf('south delivery') < descending.indexOf('east delivery'));
  assert.deepEqual(rows.map(row => row.routeId), ['north', 'south', 'waiting', 'east']);
  assert.equal(notifications, 0, 'SSR/default/controlled props triggered a sort callback');

  const mixed = [{ routeId: 'two', parcels: 2 }, { routeId: 'ten', parcels: 10 }, { routeId: 'text', parcels: '11' }];
  for (let index = 0; index < mixed.length; index++) {
    const mixedHtml = render({ rows: mixed.slice(index).concat(mixed.slice(0, index)), sort: { column: 'parcels', direction: 'asc' } });
    assert(mixedHtml.indexOf('two delivery') < mixedHtml.indexOf('ten delivery'));
    assert(mixedHtml.indexOf('ten delivery') < mixedHtml.indexOf('text delivery'));
  }
  const textHtml = render({
    rows: [{ routeId: 'z', parcels: 'Z' }, { routeId: 'accent', parcels: '\u00c4' }, { routeId: 'a', parcels: 'A' }],
    sort: { column: 'parcels', direction: 'asc' },
  });
  assert(textHtml.indexOf('a delivery') < textHtml.indexOf('accent delivery'));
  assert(textHtml.indexOf('accent delivery') < textHtml.indexOf('z delivery'));

  const error = new Error('Warehouse refresh unavailable');
  for (const scenario of [
    { rows: null, state: { error, stale: true, loading: true }, calls: ['error'], table: false },
    { rows: null, state: { loading: true }, calls: ['loading'], table: false },
    { rows: [], state: { error, stale: true, loading: true }, calls: ['error', 'empty'], table: true },
    { rows, state: { stale: true, loading: true }, calls: ['stale'], table: true },
    { rows, state: { loading: true }, calls: ['loading'], table: true },
  ]) {
    const calls = [];
    const slots = Object.fromEntries(['error', 'stale', 'loading', 'empty'].map(name => [
      `render${name[0].toUpperCase()}${name.slice(1)}`,
      context => {
        calls.push(name);
        assert.equal(context.rows, scenario.rows);
        assert.equal(context.state, scenario.state);
        assert.equal(context.sort, null);
        assert.equal(typeof context.setSort, 'function');
        if (name === 'error') assert.equal(context.error, error);
        return React.createElement('span', null, `${name} content`);
      },
    ]));
    const stateHtml = render({ rows: scenario.rows, state: scenario.state, ...slots });
    assert.deepEqual(calls, scenario.calls, 'Presentation notice precedence changed');
    assert.equal(stateHtml.includes('<table'), scenario.table, 'Supplied/absent baseline handling changed');
    if (scenario.calls.includes('empty')) {
      assert.match(stateHtml, /<td[^>]+colspan="3"[^>]*><span>empty content<\/span><\/td>/i);
    }
  }
  const suppressed = render({
    rows: [], state: { error }, emptyMessage: 'fallback empty',
    renderError: () => null, renderEmpty: () => null,
  });
  assert(suppressed.includes('<table'));
  assert(!suppressed.includes(error.message) && !suppressed.includes('fallback empty'));
  const defaultError = render({ rows: null, state: { error, retry() {} } });
  assert(defaultError.includes('role="alert"') && defaultError.includes('>Retry</button>'));
  assert(!defaultError.includes('<table'));
  const staleWithoutBaseline = render({ rows: null, state: { stale: true } });
  assert(staleWithoutBaseline.includes('<table'));
  assert(staleWithoutBaseline.includes('Showing last known data.'));
  assert(staleWithoutBaseline.includes('No data available'));
  assert.deepEqual(accesses, [], 'Provider-free DataTable SSR touched browser/network globals');
}

try {
  const api = await load(specifier);
  assert.deepEqual(accesses, [], `${specifier} import touched browser/network globals`);
  if (kind === 'client') {
    setFetch(noNetwork);
    for (const name of ['DrasiClient', 'DrasiSSEClient', 'DrasiError',
      'accumulateResult', 'sse034ResultAdapter', 'createLegacyResultAdapter']) {
      assert.equal(typeof api[name], 'function', `Missing ${name} from client export`);
      assert.equal(api[name].name, name, `Public/debug client name changed: ${name}`);
    }
    const client = new api.DrasiClient(options);
    const sse = new api.DrasiSSEClient();
    assert.equal(client.isInitialized(), false);
    assert.equal(client.getConnectionStatus().connected, false);
    await client.disconnect();
    await sse.disconnect();
    assert.deepEqual(accesses, [], 'Client construction/disconnect performed browser/network work');

    const before = { stationId: 'old-room', celsius: 3 };
    const after = { stationId: 'new-room', celsius: 4 };
    const [delta] = api.sse034ResultAdapter({
      queryId: 'temperatures', timestamp: 1,
      results: [{ type: 'UPDATE', before, after, data: after }],
    }, { receivedAt: 2 });
    assert.deepEqual(delta, {
      kind: 'delta', queryId: 'temperatures', receivedAt: 2, sourceTimestamp: 1,
      changes: [{ kind: 'update', before, after }],
    });
    assert.deepEqual(api.accumulateResult([before], delta, queryOptions.getKey), [after]);
    assert.equal(typeof api.createLegacyResultAdapter(), 'function');
    assert.equal(sse.getQueryError('temperatures'), null);

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
    if (kind === 'components' || kind === 'root') checkProviderFreeTables(api);
    const hooks = kind === 'components' ? await load('@drasi/react/react') : api;
    function HeadlessConsumer() {
      const initial = hooks.useTableSort({ defaultSort: { column: 'priority', direction: 'desc' } });
      const controlled = hooks.useTableSort({ sort: null, defaultSort: { column: 'priority', direction: 'asc' } });
      assert.deepEqual(initial.sort, { column: 'priority', direction: 'desc' });
      assert.equal(controlled.sort, null);
      assert.equal(typeof controlled.setSort, 'function');
      assert.equal(typeof controlled.toggleSort, 'function');
      return React.createElement('span', null, 'provider-free sort controller');
    }
    assert(server.renderToString(React.createElement(HeadlessConsumer)).includes('provider-free sort controller'));
    assert.deepEqual(accesses, [], 'Headless sort SSR touched browser/network globals');
    if (kind === 'react') {
      for (const name of ['DataTable', 'QueryTable', 'queryTableState', 'CodeViewerDialog']) {
        assert.equal(api[name], undefined, `Hooks runtime exports presentation: ${name}`);
      }
    } else {
      const { DrasiError } = await load('@drasi/react/client');
      const retryQuery = () => {};
      const retryConnection = () => {};
      for (const errorScope of [null, 'query', 'connection']) {
        const query = Object.freeze({
          data: null, status: errorScope === 'connection' ? 'reconnecting' : 'resynchronizing',
          loading: false, stale: true, error: new DrasiError('SERVER_UNAVAILABLE'),
          errorScope, lastUpdate: null, retry: retryQuery,
        });
        const state = api.queryTableState(query, retryConnection);
        assert.equal(state.error, query.error);
        assert.equal(state.stale, query.stale);
        assert.equal(state.loading, query.loading);
        assert.equal(state.retry, errorScope === 'connection' ? retryConnection : retryQuery);
        assert.equal(state.retryLabel, errorScope === 'connection' ? 'Retry connection' : 'Retry query');
        assert.equal(state.staleMessage,
          `${errorScope === 'connection' ? 'Reconnecting' : 'Resynchronizing'}. Showing last known data.`);
      }
    }
    setFetch(noNetwork);
    function HookConsumer() {
      const result = hooks.useDrasiQuery('temperatures', queryOptions);
      const definition = hooks.useDrasiQueryDefinition('temperatures');
      assert.equal(hooks.useDrasiClient().initialized, false);
      assert.equal(hooks.useDrasiConnectionStatus().connected, false);
      assert.equal(hooks.useDrasiServerUiUrl(), null);
      assert.equal(result.data, null);
      assert.equal(result.status, 'initial-loading');
      assert.equal(result.stale, false);
      assert.equal(result.errorScope, null);
      assert.equal(result.lastUpdate, null);
      assert.equal(typeof result.retry, 'function');
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
          queryOptions,
          renderLoading(context) {
            assert.equal(context.query.status, 'initial-loading');
            assert.equal(context.query.errorScope, null);
            assert.equal(typeof context.retryConnection, 'function');
            assert.equal(context.state.retry, context.query.retry);
            assert.equal(context.rows, null);
            return React.createElement('span', null, 'query state slot');
          },
        }));
    const renderErrors = [];
    const originalError = console.error;
    let html;
    try {
      console.error = (...args) => { renderErrors.push(args); };
      html = server.renderToString(React.createElement(hooks.DrasiProvider, options, child));
    } finally {
      console.error = originalError;
    }
    assert.deepEqual(renderErrors, [], `${specifier} emitted server-rendering warnings/errors`);
    assert(html.includes('idle SSR consumer'));
    if (kind !== 'react') {
      assert(html.includes('Warehouse temperatures'));
      assert(html.includes('query state slot'));
      assert(!html.includes('View code') && !html.includes('Expand table'));
    }
    if (kind === 'root') {
      assert.equal(typeof api.DrasiClient, 'function');
      assert.equal(typeof api.QueryTable, 'function');
      assert.equal(typeof api.DataTable, 'function');
      assert.equal(typeof api.useTableSort, 'function');
      assert.equal(typeof api.accumulateResult, 'function');
      assert.equal(typeof api.sse034ResultAdapter, 'function');
      assert.equal(typeof api.createLegacyResultAdapter, 'function');
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
  const proof = kind === 'client' ? 'idle construction; browser-free Node REST'
    : kind === 'react' ? 'idle import; headless and provider SSR'
      : 'provider-free supplied-row SSR; scoped query adapter and live-table SSR';
  console.log(`Packed ${specifier} ${format}: ${proof}; explicit CSS resolves`);
} finally {
  for (const [name, descriptor] of originalGlobals) {
    if (descriptor) Object.defineProperty(globalThis, name, descriptor);
    else delete globalThis[name];
  }
}
