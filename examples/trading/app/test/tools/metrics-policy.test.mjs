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
  assert.deepEqual(current.artifactChange.p6Sizes, { ...historical.sizes, packageTypes: sizes.packageTypes });
});

test('retains the P7 feature budget and exact history with both declaration formats accounted for', async () => {
  const historical = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p7-v2.json', import.meta.url), 'utf8'));
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(historical.schemaVersion, 2);
  assert.equal(current.previousP7Measurement, 'baseline-metrics-p7-v2.json');
  assert.equal(historical.sizes.packageTarball, 217782);
  assert.equal(current.p7MeasurementChange.esmDeclarations, historical.sizes.packageTypes);
  assert.equal(current.p7MeasurementChange.commonJsDeclarations, 41261);
  const sizes = measurePackageFiles([
    ...packageFiles.filter(file => !file.path.endsWith('.d.ts')),
    { path: 'dist/types.d.ts', bytes: current.p7MeasurementChange.esmDeclarations },
    { path: 'dist/types.d.cts', bytes: current.p7MeasurementChange.commonJsDeclarations },
  ]);
  assert.equal(sizes.packageTypes, 82505);
  assert.deepEqual(current.coverage, historical.coverage);
  assert.deepEqual(current.artifactChange.p7Sizes, { ...historical.sizes, packageTypes: sizes.packageTypes });
});

test('keeps all seven original schema-2 records byte-identical', async () => {
  const hashes = {
    'baseline-metrics-v2.json': '11770739c8a262ebe3890be470f54dede21a0780675188e77aa4019429203bb3',
    'baseline-metrics-p2-v2.json': '48a764c44412023de17241b366364e753b696781ecbe094e5c72dbc97e8abc38',
    'baseline-metrics-p3-v2.json': '72413b925b27dbbac6c6e9a24a482813ce20bf58c6e5fd012bb75b6e59166dfa',
    'baseline-metrics-p4-v2.json': '4cd852a8dce5700130b73db15b94b0e41ef884e6231b004f509c8e768e8eeb73',
    'baseline-metrics-p5-v2.json': 'e48ad1d7d1fdd2b6410ac4b54e7bc841dd43c371a4558c3c64dc1fcd235337b2',
    'baseline-metrics-p6-v2.json': '8ff7a240f83d55c926517afcd9865c9181eb40451e3fb476e211aab086834389',
    'baseline-metrics-p7-v2.json': '11ed156b053d2c37fa0805a29201249c42d65a040310018d0d88c603fc4f5171',
  };
  for (const [name, expected] of Object.entries(hashes)) {
    const bytes = await readFile(new URL(`../fixtures/${name}`, import.meta.url));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), expected, name);
    assert.equal(JSON.parse(bytes.toString('utf8')).schemaVersion, 2);
  }
});

test('advances only the explicitly approved documentation archive baseline and retains its exact prior record', async () => {
  const priorBytes = await readFile(new URL('../fixtures/baseline-metrics-p7-pre-guides-v3.json', import.meta.url));
  assert.equal(createHash('sha256').update(priorBytes).digest('hex'),
    '3a9db73a1a0dacc2614858f2bb0df9f73657fc45c109407e1d890a4f8b239b24');
  const prior = JSON.parse(priorBytes);
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  const approval = JSON.parse(await readFile(new URL('../fixtures/p7-consumer-baseline-approval.json', import.meta.url), 'utf8'));
  assert.deepEqual(current.consumerDocumentationChange, {
    approval: 'p7-consumer-baseline-approval.json',
    previousBaseline: 'baseline-metrics-p7-pre-guides-v3.json',
  });
  const restored = structuredClone(current);
  delete restored.consumerDocumentationChange;
  restored.sizes.packageTarball = prior.sizes.packageTarball;
  assert.deepEqual(restored, prior, 'No other baseline, coverage, metric scope or historical field may advance');
  assert.equal(current.sizes.packageTarball, 225320);
  assert.equal(approval.authorization.decision, 'Approve these scoped documentation/example baselines (Recommended)');
  assert.equal(approval.packageArchive.previousArtifactBytes, 222072);
  assert.equal(approval.packageArchive.previousCapBytes, 222137.64);
  assert.equal(approval.packageArchive.baselineBytes, current.sizes.packageTarball);
  assert.equal(approval.packageArchive.archiveEntries, 65);
  assert.equal(approval.packageArchive.futureGrowthPercent, 2);
  assert.equal(approval.packageArchive.addedFiles.length, 5);
  assert.throws(() => assertBaseline({ coverage: prior.coverage, sizes: current.sizes }, prior), /packageTarball grew more than 2%/);
  assertBaseline({ coverage: current.coverage, sizes: { ...current.sizes, packageTarball: 229826 } }, current);
  assert.throws(() => assertBaseline({
    coverage: current.coverage, sizes: { ...current.sizes, packageTarball: 229827 },
  }, current), /packageTarball grew more than 2%/);
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

async function p5Baseline() {
  return JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p5-quality-v3.json', import.meta.url), 'utf8'));
}

test('preserves the reviewed P5 quality approval as byte-identical historical evidence', async () => {
  const bytes = await readFile(new URL('../fixtures/baseline-metrics-p5-quality-v3.json', import.meta.url));
  assert.equal(createHash('sha256').update(bytes).digest('hex'),
    '5f87e41b72410779d0af2ef7564af95108ee1e0b90972d63b9425ebaa472102d');
});

test('keeps the distinct active P7 baseline at 2% and rejects carrying over the historical P5 allowance', async () => {
  const expected = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  assert.equal(expected.artifactChange.part, 'P7');
  assert.equal(expected.approvedP5TradingJsGzipAllowance, undefined);
  for (const [metric, bytes] of Object.entries(expected.sizes)) {
    const sizes = { ...expected.sizes, [metric]: Math.floor(bytes * 1.02) };
    assertBaseline({ coverage: expected.coverage, sizes }, expected);
    assert.throws(() => assertBaseline({
      coverage: expected.coverage, sizes: { ...sizes, [metric]: sizes[metric] + 1 },
    }, expected), new RegExp(`${metric} grew more than 2%`));
  }
  expected.approvedP5TradingJsGzipAllowance = (await p5Baseline()).approvedP5TradingJsGzipAllowance;
  assert.throws(() => assertBaseline({ coverage: expected.coverage, sizes: expected.sizes }, expected),
    /applies only to its original recorded baseline/);
});

test('uses the actual-user-approved P5 allowance exactly once: 75725 passes, 75726 fails', async () => {
  const expected = await p5Baseline();
  const unchanged = structuredClone(expected);
  const observed = { coverage: structuredClone(expected.coverage), sizes: { ...expected.sizes, tradingJsGzip: 75725 } };
  assertBaseline(observed, expected);
  assertBaseline(observed, expected);
  assert.deepEqual(expected, unchanged, 'Successful measurements must not become a compounded baseline');
  assert.equal(expected.sizes.tradingJsGzip, 74133);
  assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, tradingJsGzip: 75726 } }, expected),
    /tradingJsGzip exceeds the approved P5 cap/);
});

test('retains the original 2% boundary when the explicit P5 approval is absent', async () => {
  const expected = await p5Baseline();
  delete expected.approvedP5TradingJsGzipAllowance;
  const observed = { coverage: expected.coverage, sizes: { ...expected.sizes, tradingJsGzip: 75615 } };
  assertBaseline(observed, expected);
  assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, tradingJsGzip: 75616 } }, expected), /more than 2%/);
  assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, tradingJsGzip: 75725 } }, expected), /more than 2%/);
});

test('never applies the P5 gzip allowance to another artifact or to coverage', async () => {
  const expected = await p5Baseline();
  for (const [metric, bytes] of Object.entries(expected.sizes)) {
    if (metric === 'tradingJsGzip') continue;
    const observed = {
      coverage: expected.coverage,
      sizes: { ...expected.sizes, [metric]: Math.floor(bytes * 1.02) + 1 },
    };
    assert.throws(() => assertBaseline(observed, expected), new RegExp(`${metric} grew more than 2%`));
  }
  const observed = { coverage: structuredClone(expected.coverage), sizes: { ...expected.sizes, tradingJsGzip: 75725 } };
  observed.coverage.package.lines -= 0.01;
  assert.throws(() => assertBaseline(observed, expected), /package lines coverage regressed/);
});

test('rejects allowance reuse for a changed size baseline, another layer or an altered approval', async () => {
  const mutations = [
    record => { record.sizes.tradingJsGzip = 75725; },
    record => { record.sizes.packageTarball += 1; },
    record => { record.artifactChange.part = 'B / P6'; },
    record => { record.artifactChange.issue = 165; },
    record => { record.p5MeasurementChange.originalP5Head = 'not-the-approved-record'; },
    record => { record.approvedP5TradingJsGzipAllowance.bytes = 111; },
    record => { record.approvedP5TradingJsGzipAllowance.baselineSizesSha256 = 'another-baseline'; },
    record => { record.approvedP5TradingJsGzipAllowance = null; },
    record => {
      record.sizes.tradingJsGzip = 75725;
      const sorted = Object.entries(record.sizes).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
      record.approvedP5TradingJsGzipAllowance.baselineSizesSha256 =
        createHash('sha256').update(JSON.stringify(sorted)).digest('hex');
    },
  ];
  for (const mutate of mutations) {
    const expected = await p5Baseline();
    mutate(expected);
    assert.throws(() => assertBaseline({ coverage: expected.coverage, sizes: expected.sizes }, expected),
      /applies only to its original recorded baseline/);
  }
});
