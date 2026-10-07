// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

/** Count every shipped entrypoint/shared chunk and both declaration formats. */
export function measurePackageModules(paths, readBytes) {
  const sizes = { packageEsm: 0, packageCjs: 0, packageTypes: 0, packageCss: 0 };
  const names = new Set();
  for (const path of paths) {
    assert(typeof path === 'string' && path.startsWith('package/') && !names.has(path),
      `Invalid or duplicate package path: ${path}`);
    names.add(path);
    let metric;
    if (/^package\/dist\/.*\.d\.(?:ts|mts|cts)$/.test(path)) metric = 'packageTypes';
    else if (/^package\/dist\/.*\.(?:js|mjs)$/.test(path)) metric = 'packageEsm';
    else if (/^package\/dist\/.*\.cjs$/.test(path)) metric = 'packageCjs';
    else if (/\.css$/.test(path)) metric = 'packageCss';
    if (metric) {
      const bytes = readBytes(path);
      assert(Number.isInteger(bytes) && bytes >= 0, `Invalid package byte count: ${path}`);
      sizes[metric] += bytes;
    }
  }
  for (const [metric, bytes] of Object.entries(sizes)) {
    assert(bytes > 0, `Missing packed ${metric} artifacts`);
  }
  return sizes;
}

export async function measureTradingAssets(directory) {
  const sizes = { tradingJs: 0, tradingJsGzip: 0, tradingCss: 0, tradingCssGzip: 0 };
  for (const asset of await readdir(directory, { recursive: true })) {
    if (!/\.(?:[cm]?js|css)$/.test(asset)) continue;
    const body = await readFile(join(directory, asset));
    const key = asset.endsWith('.css') ? 'tradingCss' : 'tradingJs';
    sizes[key] += body.length;
    sizes[`${key}Gzip`] += gzipSync(body).length;
  }
  assert(sizes.tradingJs > 0 && sizes.tradingCss > 0, 'Built Trading assets are missing');
  return sizes;
}

function approvedP5TradingGzipAllowance(baseline) {
  const approval = baseline.approvedP5TradingJsGzipAllowance;
  if (approval === undefined) return 0;
  const recordedSizes = '5e8ad7efb5c8f9dc59ad0308844c5643bdaf2e5d81d7f5fd24fdefaee249ba15';
  const sizes = Object.entries(baseline.sizes).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
  const fingerprint = createHash('sha256').update(JSON.stringify(sizes)).digest('hex');
  assert(approval !== null && typeof approval === 'object' &&
    approval.bytes === 110 && approval.baselineSizesSha256 === recordedSizes &&
    fingerprint === recordedSizes && baseline.sizes.tradingJsGzip === 74133 &&
    baseline.artifactChange?.issue === 164 && baseline.artifactChange?.part === 'A / P5' &&
    baseline.p5MeasurementChange?.originalP5Head === '569b1d26e558b838d3a572bf7e2e5fbd6280eeaf',
  'The approved P5 Trading gzip allowance applies only to its original recorded baseline; remove it for a distinct baseline.');
  return 110;
}

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
  const p5Allowance = approvedP5TradingGzipAllowance(baseline);
  for (const [asset, size] of Object.entries(observed.sizes)) {
    assert(Number.isInteger(size) && size >= 0 && Number.isInteger(baseline.sizes[asset]), `Invalid ${asset} byte count`);
    const extra = asset === 'tradingJsGzip' ? p5Allowance : 0;
    assert(size <= baseline.sizes[asset] * 1.02 + extra,
      extra ? `${asset} exceeds the approved P5 cap: 74133 * 1.02 + 110 = 75725.66.`
        : `${asset} grew more than 2%. Explain the change and review the artifact baseline.`);
  }
}
