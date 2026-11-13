// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';
import { assertExampleBudget, assertHooksGraph, entryGraph, measureExampleAssets } from '../scripts/check-build.mjs';
import { assertExampleModule, moduleIdentity } from '../scripts/module-identity.mjs';

const graph = () => ({
  'hooks.js': { entry: 'hooks.html', imports: ['shared.js'], dynamicImports: [], css: [], modules: [] },
  'shared.js': { entry: null, imports: [], dynamicImports: [], css: [], modules: [{ id: '@drasi/react/dist/react/index.js', sources: [] }] },
});
test('built dependency proof normalizes actual virtual IDs and walks static, shared and lazy chunks', () => {
  const root = '/__w/_temp/trading-consumer/dev-tools/react/examples';
  const virtual = moduleIdentity(`\0${root}/node_modules/react/jsx-runtime.js?commonjs-module`, root);
  assert.deepEqual(virtual, { id: 'node_modules/react/jsx-runtime.js?commonjs-module', sourceMap: null });
  assertExampleModule(virtual.id);
  assertExampleModule(moduleIdentity('\0commonjsHelpers.js', root).id);
  const library = moduleIdentity(`${root}/node_modules/@drasi/react/dist/chunk-client.js`, root);
  assert.deepEqual(library, {
    id: '@drasi/react/dist/chunk-client.js',
    sourceMap: `${root}/node_modules/@drasi/react/dist/chunk-client.js.map`,
  });
  assertExampleModule(library.id);
  for (const raw of [
    `${root}/../../../examples/trading/app/src/App.tsx`,
    `\0${root}/../../../examples/trading/app/src/App.tsx?commonjs-module`,
    `${root}/../src/react/index.ts`,
    `${root}/node_modules/@drasi/react/src/react/index.ts`,
    '\0unknown-helper.js',
  ]) {
    assert.throws(() => assertExampleModule(moduleIdentity(raw, root).id), /outside its source\/installed artifacts/);
  }
  const fixture = graph();
  fixture['shared.js'].modules.push({ id: virtual.id, sources: [] });
  fixture['shared.js'].dynamicImports = ['lazy.js'];
  fixture['lazy.js'] = { entry: null, imports: [], dynamicImports: [], css: [], modules: [] };
  assert.deepEqual(entryGraph(fixture, 'hooks.html'), ['hooks.js', 'lazy.js', 'shared.js']);
  assert.deepEqual(assertHooksGraph(fixture), ['hooks.js', 'lazy.js', 'shared.js']);
});
test('proof rejects component code hidden in a shared or lazy package chunk', () => {
  const fixture = graph();
  fixture['shared.js'].modules[0].sources = ['../src/components/Modal.tsx'];
  assert.throws(() => assertHooksGraph(fixture), /component implementation/);
});

test('the convenience-table entry accounts for shared and lazy presentation code without widening hooks', () => {
  const fixture = graph();
  fixture['query-table.js'] = {
    entry: 'query-table.html', imports: ['shared.js', 'table.js'], dynamicImports: [], css: [], modules: [],
  };
  fixture['table.js'] = {
    entry: null, imports: [], dynamicImports: ['modal.js'], css: ['table.css'],
    modules: [{ id: '@drasi/react/dist/components/index.js', sources: ['../src/components/QueryTable.tsx'] }],
  };
  fixture['modal.js'] = { entry: null, imports: [], dynamicImports: [], css: [], modules: [] };
  assert.deepEqual(entryGraph(fixture, 'query-table.html'), ['modal.js', 'query-table.js', 'shared.js', 'table.js']);
  assert.deepEqual(assertHooksGraph(fixture), ['hooks.js', 'shared.js']);
  delete fixture['modal.js'];
  assert.throws(() => entryGraph(fixture, 'query-table.html'), /Missing emitted chunk modal\.js/);
});
test('proof rejects component CSS, missing chunks and Trading or primitive imports', () => {
  const css = graph();
  css['shared.js'].css = ['assets/styles.css'];
  assert.throws(() => assertHooksGraph(css), /CSS/);
  const missing = graph();
  missing['hooks.js'].imports = ['missing.js'];
  assert.throws(() => assertHooksGraph(missing), /Missing emitted chunk/);
  for (const id of ['../trading/App.tsx', 'node_modules/@radix-ui/react-dialog/index.js']) {
    const fixture = graph();
    fixture['shared.js'].modules[0].id = id;
    assert.throws(() => assertHooksGraph(fixture), /presentation\/domain/);
  }
});

test('example budgets count every entry and enforce exactly the inherited 2% growth ceiling', () => {
  const metrics = { js: 100, jsGzip: 50, css: 0, cssGzip: 0 };
  const baseline = { total: metrics, entries: { 'hooks.html': metrics } };
  const boundary = { js: 102, jsGzip: 51, css: 0, cssGzip: 0 };
  assert.doesNotThrow(() => assertExampleBudget({ total: boundary, entries: { 'hooks.html': boundary } }, baseline));
  assert.throws(() => assertExampleBudget({ total: { ...boundary, js: 103 }, entries: { 'hooks.html': boundary } }, baseline), /growth budget/);
  assert.throws(() => assertExampleBudget({ total: boundary, entries: {} }, baseline), /entry set/);
  assert.throws(() => assertExampleBudget({ total: boundary, entries: { 'hooks.html': { ...boundary, css: 1 } } }, baseline), /growth budget/);
  assert.throws(() => assertExampleBudget({ total: { ...boundary, js: undefined }, entries: { 'hooks.html': boundary } }, baseline), /Missing example/);
});

test('approved example advances preserve both predecessor records and the exact current measurement', async () => {
  const bytes = await readFile(new URL('baseline-metrics-original.json', import.meta.url));
  assert.equal(createHash('sha256').update(bytes).digest('hex'),
    'affe56d1808dfb2d60d7e6e5a9c79d8ac9ad54381416c4ef2a8d7944bb017fdd');
  const original = JSON.parse(bytes);
  const predecessorBytes = await readFile(new URL('baseline-metrics-pre-graph-contracts.json', import.meta.url));
  assert.equal(createHash('sha256').update(predecessorBytes).digest('hex'),
    'a7b32950525f4c07d318918754e04bd98f723daad18e0146a2d1634d9bc06c40');
  const predecessor = JSON.parse(predecessorBytes);
  const restored = structuredClone(predecessor);
  delete restored.entries['query-table.html'];
  assert.deepEqual(restored, original);
  assert.deepEqual(predecessor.entries['query-table.html'],
    { js: 230748, jsGzip: 75560, css: 7462, cssGzip: 1981 });

  const baseline = JSON.parse(await readFile(new URL('baseline-metrics.json', import.meta.url), 'utf8'));
  assert.deepEqual(baseline.advance, {
    previousBaseline: 'baseline-metrics-pre-graph-contracts.json',
    reason: 'Measure the already-shipped explicit instance/output selection and unified graph-result contracts. Preserve the predecessor unchanged and retain the same 2% future-growth policy.',
  });
  assert.deepEqual(baseline.total, { js: 240834, jsGzip: 79642, css: 7462, cssGzip: 1981 });
  assert.deepEqual(baseline.entries, {
    'index.html': { js: 234190, jsGzip: 76487, css: 7462, cssGzip: 1981 },
    'hooks.html': { js: 183756, jsGzip: 58449, css: 0, cssGzip: 0 },
    'showcase.html': { js: 234921, jsGzip: 76574, css: 7462, cssGzip: 1981 },
    'query-table.html': { js: 234987, jsGzip: 76789, css: 7462, cssGzip: 1981 },
  });
  for (const [name, expected] of Object.entries({ total: baseline.total, ...baseline.entries })) {
    for (const metric of ['js', 'jsGzip', 'css', 'cssGzip']) {
      const evidence = structuredClone(baseline);
      const scope = name === 'total' ? evidence.total : evidence.entries[name];
      scope[metric] = Math.floor(expected[metric] * 1.02);
      assert.doesNotThrow(() => assertExampleBudget(evidence, baseline));
      scope[metric] += 1;
      assert.throws(() => assertExampleBudget(evidence, baseline), /growth budget/);
    }
  }
  const uncounted = structuredClone(baseline);
  uncounted.entries['unreviewed.html'] = baseline.entries['query-table.html'];
  assert.throws(() => assertExampleBudget(uncounted, baseline), /entry set changed/);
  assert.equal(baseline.entries['hooks.html'].css, 0);
});

test('example inventory includes root/nested JS, MJS, CJS and CSS but not source maps', async context => {
  const directory = await mkdtemp(join(tmpdir(), 'drasi-example-assets-'));
  context.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, 'assets/nested'), { recursive: true });
  const files = {
    'entry.js': 'entry',
    'assets/nested/shared.mjs': 'shared',
    'assets/nested/compat.cjs': 'compat',
    'root.css': 'root style',
    'assets/nested/theme.css': 'theme',
    'assets/nested/shared.mjs.map': 'map',
  };
  for (const [file, content] of Object.entries(files)) await writeFile(join(directory, file), content);
  const measured = await measureExampleAssets(directory);
  assert.deepEqual(Object.keys(measured).sort(), Object.keys(files).filter(file => !file.endsWith('.map')).sort());
  for (const [file, value] of Object.entries(measured)) {
    assert.equal(value.bytes, Buffer.byteLength(files[file]));
    assert.equal(value.gzip, gzipSync(files[file]).length);
    assert.match(value.sha256, /^[a-f0-9]{64}$/);
  }
  const fixture = graph();
  fixture['hooks.js'].imports = ['assets/nested/shared.mjs'];
  fixture['assets/nested/shared.mjs'] = { ...fixture['shared.js'], dynamicImports: ['assets/nested/compat.cjs'] };
  fixture['assets/nested/compat.cjs'] = { ...fixture['shared.js'] };
  assert.deepEqual(entryGraph(fixture, 'hooks.html'), ['assets/nested/compat.cjs', 'assets/nested/shared.mjs', 'hooks.js']);
});
