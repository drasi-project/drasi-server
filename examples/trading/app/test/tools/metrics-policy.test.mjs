// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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

test('measures all P3 entrypoints and shared chunks including both declaration formats, but not maps', () => {
  const files = {
    'package/dist/index.js': 2, 'package/dist/client/index.js': 3, 'package/dist/chunk-client.js': 500,
    'package/dist/index.cjs': 4, 'package/dist/react/index.cjs': 5, 'package/dist/chunk-react.cjs': 600,
    'package/dist/index.d.ts': 6, 'package/dist/client/index.d.ts': 7, 'package/dist/types-shared.d.ts': 700,
    'package/dist/index.d.cts': 1000, 'package/dist/index.js.map': 1000, 'package/README.md': 1000,
    'package/styles.css': 13,
  };
  assert.deepEqual(measurePackageModules(Object.keys(files), path => files[path]), {
    packageEsm: 505, packageCjs: 609, packageTypes: 1713, packageCss: 13,
  });
});

test('does not accept an artifact missing any mandatory runtime or declaration format', () => {
  for (const missing of ['.js', '.cjs', '.d.ts', '.css']) {
    const files = ['.js', '.cjs', '.d.ts', '.css'].filter(suffix => suffix !== missing).map(suffix => `package/dist/index${suffix}`);
    assert.throws(() => measurePackageModules(files, () => 1), /Missing packed/);
  }
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

test('retains the P3 schema-2 record while counting its separately shipped CommonJS declarations', async () => {
  const historical = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p3-v2.json', import.meta.url), 'utf8'));
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(historical.schemaVersion, 2);
  assert.equal(historical.sizes.packageTypes, 28121);
  assert.equal(current.p3MeasurementChange.esmDeclarations, historical.sizes.packageTypes);
  assert.equal(current.p3MeasurementChange.commonJsDeclarations, 28135);
  const sizes = measurePackageFiles([
    ...packageFiles.filter(file => !file.path.endsWith('.d.ts')),
    { path: 'dist/types.d.ts', bytes: current.p3MeasurementChange.esmDeclarations },
    { path: 'dist/types.d.cts', bytes: current.p3MeasurementChange.commonJsDeclarations },
  ]);
  assert.equal(sizes.packageTypes, 56256);
  assert.deepEqual(current.coverage, historical.coverage);
});

test('retains the P4 feature budget and schema-2 history while counting both actual declaration graphs', async () => {
  const historical = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p4-v2.json', import.meta.url), 'utf8'));
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(historical.schemaVersion, 2);
  assert.equal(historical.sizes.packageTypes, 34875);
  assert.equal(current.p4MeasurementChange.esmDeclarations, historical.sizes.packageTypes);
  assert.equal(current.p4MeasurementChange.commonJsDeclarations, 34892);
  const sizes = measurePackageFiles([
    ...packageFiles.filter(file => !file.path.endsWith('.d.ts')),
    { path: 'dist/types.d.ts', bytes: current.p4MeasurementChange.esmDeclarations },
    { path: 'dist/types.d.cts', bytes: current.p4MeasurementChange.commonJsDeclarations },
  ]);
  assert.equal(sizes.packageTypes, 69767);
  assert.equal(current.artifactChange.p4Sizes.packageTypes, sizes.packageTypes);
  assert.deepEqual(current.coverage, historical.coverage);
  assert.deepEqual(current.artifactChange.p4Sizes, { ...historical.sizes, packageTypes: sizes.packageTypes });
});

test('retains P5 feature bytes separately from counting its already-shipped CommonJS declarations', async () => {
  const historical = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p5-v2.json', import.meta.url), 'utf8'));
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(historical.schemaVersion, 2);
  assert.equal(current.previousP5Measurement, 'baseline-metrics-p5-v2.json');
  assert.equal(historical.sizes.packageTypes, 37834);
  assert.equal(current.p5MeasurementChange.esmDeclarations, historical.sizes.packageTypes);
  assert.equal(current.p5MeasurementChange.commonJsDeclarations, 37851);
  const sizes = measurePackageFiles([
    ...packageFiles.filter(file => !file.path.endsWith('.d.ts')),
    { path: 'dist/types.d.ts', bytes: current.p5MeasurementChange.esmDeclarations },
    { path: 'dist/types.d.cts', bytes: current.p5MeasurementChange.commonJsDeclarations },
  ]);
  assert.equal(sizes.packageTypes, 75685);
  assert.deepEqual(current.coverage, historical.coverage);
  assert.deepEqual(current.artifactChange.p5Sizes, { ...historical.sizes, packageTypes: sizes.packageTypes });
});

test('retains P6 feature bytes while counting both already-shipped declaration formats', async () => {
  const historical = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p6-v2.json', import.meta.url), 'utf8'));
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(historical.schemaVersion, 2);
  assert.equal(current.previousP6Measurement, 'baseline-metrics-p6-v2.json');
  assert.equal(historical.sizes.packageTypes, 41244);
  assert.equal(current.p6MeasurementChange.esmDeclarations, historical.sizes.packageTypes);
  assert.equal(current.p6MeasurementChange.commonJsDeclarations, 41261);
  const sizes = measurePackageFiles([
    ...packageFiles.filter(file => !file.path.endsWith('.d.ts')),
    { path: 'dist/types.d.ts', bytes: current.p6MeasurementChange.esmDeclarations },
    { path: 'dist/types.d.cts', bytes: current.p6MeasurementChange.commonJsDeclarations },
  ]);
  assert.equal(sizes.packageTypes, 82505);
  assert.deepEqual(current.coverage, historical.coverage);
  assert.deepEqual(current.sizes, { ...historical.sizes, packageTypes: sizes.packageTypes });
});

test('keeps all six original schema-2 records byte-identical', async () => {
  const hashes = {
    'baseline-metrics-v2.json': '11770739c8a262ebe3890be470f54dede21a0780675188e77aa4019429203bb3',
    'baseline-metrics-p2-v2.json': '48a764c44412023de17241b366364e753b696781ecbe094e5c72dbc97e8abc38',
    'baseline-metrics-p3-v2.json': '72413b925b27dbbac6c6e9a24a482813ce20bf58c6e5fd012bb75b6e59166dfa',
    'baseline-metrics-p4-v2.json': '4cd852a8dce5700130b73db15b94b0e41ef884e6231b004f509c8e768e8eeb73',
    'baseline-metrics-p5-v2.json': 'e48ad1d7d1fdd2b6410ac4b54e7bc841dd43c371a4558c3c64dc1fcd235337b2',
    'baseline-metrics-p6-v2.json': '8ff7a240f83d55c926517afcd9865c9181eb40451e3fb476e211aab086834389',
  };
  for (const [name, expected] of Object.entries(hashes)) {
    const bytes = await readFile(new URL(`../fixtures/${name}`, import.meta.url));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), expected, name);
    assert.equal(JSON.parse(bytes.toString('utf8')).schemaVersion, 2);
  }
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
