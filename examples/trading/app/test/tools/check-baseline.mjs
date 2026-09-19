// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { assertBaseline } from './metrics-policy.mjs';

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

function packedBytes(path) {
  return execFileSync('tar', ['-xOf', archive, `package/${path}`]).length;
}

const assets = await readdir(join(app, 'dist/assets'));
const sizes = {
  packageTarball: (await stat(archive)).size,
  packageEsm: packedBytes('dist/index.js'),
  packageCjs: packedBytes('dist/index.cjs'),
  packageTypes: packedBytes('dist/index.d.ts'),
  packageCss: packedBytes('styles.css'),
  tradingJs: 0, tradingJsGzip: 0, tradingCss: 0, tradingCssGzip: 0,
};
for (const asset of assets) {
  if (!/\.(js|css)$/.test(asset)) continue;
  const body = await readFile(join(app, 'dist/assets', asset));
  const key = asset.endsWith('.js') ? 'tradingJs' : 'tradingCss';
  sizes[key] += body.length;
  sizes[`${key}Gzip`] += gzipSync(body).length;
}
assert(sizes.tradingJs > 0 && sizes.tradingCss > 0, 'Built Trading assets are missing');
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
  console.log('Measured coverage and artifact sizes satisfy the P1 regression baseline (not final coverage targets).');
}
