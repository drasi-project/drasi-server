// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';

export function assertBaseline(observed, baseline) {
  for (const project of ['package', 'trading']) {
    for (const counter of ['statements', 'branches', 'functions', 'lines']) {
      const actual = observed.coverage[project][counter];
      const expected = baseline.coverage[project][counter];
      assert(Number.isFinite(actual) && Number.isFinite(expected), `Missing ${project} ${counter} coverage`);
      assert(actual >= expected,
        `${project} ${counter} coverage regressed. Explain and review baseline changes; do not exclude product code.`);
    }
  }
  assert.deepEqual(Object.keys(observed.sizes).sort(), Object.keys(baseline.sizes).sort(), 'Artifact metric set changed');
  for (const [asset, size] of Object.entries(observed.sizes)) {
    assert(Number.isInteger(size) && size >= 0 && Number.isInteger(baseline.sizes[asset]), `Invalid ${asset} byte count`);
    assert(size <= baseline.sizes[asset] * 1.02,
      `${asset} grew more than 2%. Explain the change and review the artifact baseline.`);
  }
}
