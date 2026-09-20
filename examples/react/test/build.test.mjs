// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { assertExampleBudget, assertHooksGraph, entryGraph } from '../scripts/check-build.mjs';
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
