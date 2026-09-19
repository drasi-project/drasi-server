// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assertBaseline } from './metrics-policy.mjs';

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
