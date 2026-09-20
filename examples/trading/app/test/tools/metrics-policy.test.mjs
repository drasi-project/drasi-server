// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { assertBaseline, measurePackageModules, measureTradingAssets } from './metrics-policy.mjs';

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

const packageFiles = [
  { path: 'dist/index.js', bytes: 1000 },
  { path: 'dist/index.cjs', bytes: 1000 },
  { path: 'dist/index.d.ts', bytes: 500 },
  { path: 'styles.css', bytes: 100 },
];

function measurePackageFiles(files) {
  const bytes = new Map(files.map(file => [`package/${file.path}`, file.bytes]));
  return measurePackageModules(files.map(file => `package/${file.path}`), path => bytes.get(path));
}

test('counts every package entrypoint, nested chunk, declaration format and stylesheet', () => {
  assert.deepEqual(measurePackageFiles([
    ...packageFiles,
    { path: 'dist/client.mjs', bytes: 200 },
    { path: 'dist/chunks/shared.js', bytes: 300 },
    { path: 'dist/client.cjs', bytes: 400 },
    { path: 'dist/chunks/shared.cjs', bytes: 500 },
    { path: 'dist/index.d.cts', bytes: 600 },
    { path: 'dist/client.d.mts', bytes: 700 },
    { path: 'dist/chunks/shared.d.ts', bytes: 800 },
    { path: 'dist/theme.css', bytes: 200 },
    { path: 'dist/index.js.map', bytes: 10000 },
    { path: 'README.md', bytes: 10000 },
  ]), { packageEsm: 1500, packageCjs: 1900, packageTypes: 2600, packageCss: 300 });
});

test('remeasures both historical P1 and P2 declarations without changing product bytes', async () => {
  for (const [name, correctedTypes] of [
    ['baseline-metrics-v2.json', 37492],
    ['baseline-metrics-p2-v2.json', 42280],
  ]) {
    const historical = JSON.parse(await readFile(new URL(`../fixtures/${name}`, import.meta.url), 'utf8'));
    const sizes = historical.sizes;
    const measured = measurePackageFiles([
      { path: 'dist/index.js', bytes: sizes.packageEsm },
      { path: 'dist/index.cjs', bytes: sizes.packageCjs },
      { path: 'dist/index.d.ts', bytes: sizes.packageTypes },
      { path: 'dist/index.d.cts', bytes: sizes.packageTypes },
      { path: 'styles.css', bytes: sizes.packageCss },
    ]);
    assert.deepEqual(measured, {
      packageEsm: sizes.packageEsm, packageCjs: sizes.packageCjs,
      packageTypes: correctedTypes, packageCss: sizes.packageCss,
    });
    assert.equal(historical.schemaVersion, 2);
  }
});

test('rejects a secondary chunk exceeding the same 2% budget even when index is unchanged', () => {
  const expected = { ...baseline, sizes: measurePackageFiles(packageFiles) };
  const observed = {
    ...expected,
    sizes: measurePackageFiles([...packageFiles, { path: 'dist/chunks/added.js', bytes: 21 }]),
  };
  assert.throws(() => assertBaseline(observed, expected), /packageEsm grew more than 2%/);
});

test('rejects missing formats, duplicate files and invalid packed measurements', () => {
  assert.throws(() => measurePackageFiles(packageFiles.filter(file => !file.path.endsWith('.cjs'))), /Missing packed packageCjs/);
  assert.throws(() => measurePackageFiles([...packageFiles, packageFiles[0]]), /duplicate package path/);
  assert.throws(() => measurePackageFiles([...packageFiles, { path: 'dist/extra.js', bytes: -1 }]), /Invalid package byte count/);
});

test('counts nested Trading chunks and CSS without counting source maps as executable assets', async context => {
  const directory = await mkdtemp(join(tmpdir(), 'drasi-p1-asset-metrics-'));
  context.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, 'chunks'));
  const files = {
    'index.js': 'entry',
    'chunks/shared.mjs': 'shared',
    'chunks/compat.cjs': 'compat',
    'index.css': 'main',
    'chunks/theme.css': 'theme',
    'chunks/shared.mjs.map': 'not executable bytes',
  };
  for (const [name, content] of Object.entries(files)) {
    await writeFile(join(directory, name), content);
  }
  assert.deepEqual(await measureTradingAssets(directory), {
    tradingJs: 17,
    tradingJsGzip: ['entry', 'shared', 'compat'].reduce((bytes, content) => bytes + gzipSync(content).length, 0),
    tradingCss: 9,
    tradingCssGzip: ['main', 'theme'].reduce((bytes, content) => bytes + gzipSync(content).length, 0),
  });
});
