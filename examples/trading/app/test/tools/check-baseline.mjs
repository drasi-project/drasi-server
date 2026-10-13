// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { assertBaseline, measurePackageModules, measureTradingAssets } from './metrics-policy.mjs';

const [archiveArgument, appArgument, packageCoverageArgument, mode] = process.argv.slice(2);
assert(archiveArgument && appArgument && packageCoverageArgument,
  'Usage: node test/tools/check-baseline.mjs <package.tgz> <consumer-app> <package-coverage-summary.json> [--measure-only]');
const archive = resolve(archiveArgument);
const app = resolve(appArgument);
const counters = ['statements', 'branches', 'functions', 'lines'];

async function coverage(path) {
  const { total } = JSON.parse(await readFile(path, 'utf8'));
  return Object.fromEntries(counters.map(counter => {
    assert(Number.isFinite(total[counter]?.pct), `Missing coverage counter ${counter} in ${path}`);
    return [counter, total[counter].pct];
  }));
}

const packedPaths = execFileSync('tar', ['-tzf', archive], { encoding: 'utf8' }).trim().split('\n');

const sizes = {
  packageTarball: (await stat(archive)).size,
  ...measurePackageModules(packedPaths, path => execFileSync('tar', ['-xOf', archive, path]).length),
  ...await measureTradingAssets(join(app, 'dist/assets')),
};
const observed = {
  coverage: {
    package: await coverage(resolve(packageCoverageArgument)),
    trading: await coverage(join(app, 'coverage/coverage-summary.json')),
  },
  sizes,
};
await mkdir(join(app, 'test-results'), { recursive: true });
await writeFile(join(app, 'test-results/measured-baseline.json'), `${JSON.stringify(observed, null, 2)}\n`);
console.log(JSON.stringify(observed, null, 2));
if (mode !== '--measure-only') {
  const baseline = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assertBaseline(observed, baseline);
  if (baseline.approvedP5TradingJsGzipAllowance) {
    console.log('Includes the user-approved P5-only Trading JS gzip allowance: 74133 * 1.02 + 110 = 75725.66 bytes; all other limits are unchanged.');
  }
  console.log('Measured coverage and artifact sizes satisfy the P1 regression baseline (not final coverage targets).');
}
