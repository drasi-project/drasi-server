// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assertBaseline, measurePackageModules } from './metrics-policy.mjs';

const baseline = {
  coverage: {
    package: { statements: 61.81, branches: 64.87, functions: 75.58, lines: 61.81 },
    trading: { statements: 89.01, branches: 78.6, functions: 85.92, lines: 89.01 },
  },
  sizes: { packageTarball: 1000, tradingJs: 1000 },
};

test('accepts unchanged coverage and exactly 2% artifact growth', () => {
  assertBaseline({ ...baseline, sizes: { packageTarball: 1020, tradingJs: 1020 } }, baseline);
});

test('rejects a one-byte breach of the artifact budget', () => {
  assert.throws(() => assertBaseline({ ...baseline, sizes: { packageTarball: 1021, tradingJs: 1000 } }, baseline), /more than 2%/);
});

test('rejects even a small unexplained coverage drop', () => {
  const observed = structuredClone(baseline);
  observed.coverage.package.lines -= 0.01;
  assert.throws(() => assertBaseline(observed, baseline), /package lines coverage regressed/);
});

test('rejects missing mandatory metrics rather than passing a partial measurement', () => {
  const observed = structuredClone(baseline);
  delete observed.coverage.trading.functions;
  assert.throws(() => assertBaseline(observed, baseline), /Missing trading functions/);
  assert.throws(() => assertBaseline({ ...baseline, sizes: {} }, baseline), /Artifact metric set changed/);
});

test('measures all entrypoints and shared chunks without counting maps or dual declarations twice', () => {
  const files = {
    'package/dist/index.js': 2, 'package/dist/client/index.js': 3, 'package/dist/chunk-client.js': 500,
    'package/dist/index.cjs': 4, 'package/dist/react/index.cjs': 5, 'package/dist/chunk-react.cjs': 600,
    'package/dist/index.d.ts': 6, 'package/dist/client/index.d.ts': 7, 'package/dist/types-shared.d.ts': 700,
    'package/dist/index.d.cts': 1000, 'package/dist/index.js.map': 1000, 'package/README.md': 1000,
  };
  assert.deepEqual(measurePackageModules(Object.keys(files), path => files[path]), {
    packageEsm: 505, packageCjs: 609, packageTypes: 713,
  });
});

test('does not accept an artifact missing any mandatory runtime or declaration format', () => {
  for (const missing of ['.js', '.cjs', '.d.ts']) {
    const files = ['.js', '.cjs', '.d.ts'].filter(suffix => suffix !== missing).map(suffix => `package/dist/index${suffix}`);
    assert.throws(() => measurePackageModules(files, () => 1), /Missing packed/);
  }
});
