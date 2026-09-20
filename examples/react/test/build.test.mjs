// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { assertExampleBudget, assertHooksGraph, entryGraph } from '../scripts/check-build.mjs';

const graph = () => ({
  'hooks.js': { entry: 'hooks.html', imports: ['shared.js'], dynamicImports: [], css: [], modules: [] },
  'shared.js': { entry: null, imports: [], dynamicImports: [], css: [], modules: [{ id: '@drasi/react/dist/react/index.js', sources: [] }] },
});
test('built dependency proof walks static, shared and lazy chunks', () => {
  const fixture = graph();
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
