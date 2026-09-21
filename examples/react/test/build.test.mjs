// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
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
  const root = '/__w/_temp/trading-consumer/examples/react';
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
    `${root}/../trading/app/src/App.tsx`,
    `\0${root}/../trading/app/src/App.tsx?commonjs-module`,
    `${root}/../../dev-tools/react/src/react/index.ts`,
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
