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
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p7-pre-release-review.json', import.meta.url), 'utf8'));
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
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p7-pre-release-review.json', import.meta.url), 'utf8'));
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

async function historicalP5Baseline() {
  return JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p5-pre-review.json', import.meta.url), 'utf8'));
}

async function reviewedP5Baseline() {
  return JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p5-review.json', import.meta.url), 'utf8'));
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
  assert.equal(expected.reviewBudgetChange, undefined);
  for (const [metric, bytes] of Object.entries(expected.sizes)) {
    const sizes = { ...expected.sizes, [metric]: Math.floor(bytes * 1.02) };
    assertBaseline({ coverage: expected.coverage, sizes }, expected);
    assert.throws(() => assertBaseline({
      coverage: expected.coverage, sizes: { ...sizes, [metric]: sizes[metric] + 1 },
    }, expected), new RegExp(`${metric} grew more than 2%`));
  }
  expected.approvedP5TradingJsGzipAllowance = (await historicalP5Baseline()).approvedP5TradingJsGzipAllowance;
  assert.throws(() => assertBaseline({ coverage: expected.coverage, sizes: expected.sizes }, expected),
    /applies only to its original recorded baseline/);
});

test('preserves the complete P6 policy and accepted P5 review record without transferring approval', async () => {
  for (const [name, digest] of [
    ['baseline-metrics-p6-pre-review.json', '58fa504b7e24cdf40fa39e5e09573146aed4756f715d63e5cefaf38e9f01f45e'],
    ['baseline-metrics-p5-review.json', 'af310e14562e9d0ff52a47965aadb0cdce3e0a6716d96878322057ed70e1e416'],
    ['baseline-metrics-p6-review.json', '49d7ca4e8e83831431ef4cb8b2cf8c28e8763732a587cb11d93a81c390ee100a'],
    ['baseline-metrics-p7-pre-release-review.json', '47e8dd79d8d00636a6aaad72038bd0933d75d1fe3ec568d0fd36441744829a7e'],
  ]) {
    const bytes = await readFile(new URL(`../fixtures/${name}`, import.meta.url));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), digest, name);
  }
});

test('limits the P6 approval to its first archive and dual-declaration measurements', async () => {
  const originalBytes = await readFile(new URL('../fixtures/baseline-metrics-p6-pre-review.json', import.meta.url));
  const historical = JSON.parse(originalBytes);
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p6-review.json', import.meta.url), 'utf8'));
  const approved = { packageTarball: 208944, packageTypes: 84985 };
  assert.equal(current.reviewBudgetChange.previousBaselineSha256,
    createHash('sha256').update(originalBytes).digest('hex'));
  assert.equal(current.reviewBudgetChange.previousCommit, '1a8d1f504f86dd469643c0c4f471beca2de0513b');
  assert.equal(current.reviewBudgetChange.approvedArchiveBaseline, approved.packageTarball);
  assert.equal(current.reviewBudgetChange.approvedDeclarationBaseline, approved.packageTypes);
  assert.deepEqual(current.reviewBudgetChange.firstFailedMeasurement, approved);
  assert.equal(current.reviewBudgetChange.firstArtifactSha256,
    'f9e310996d069657b5c458a9c6e004571d669974f55abc12680ad4eb588ab5b9');
  assert.deepEqual(current.reviewBudgetChange.previousCaps, { packageTarball: 208936.8, packageTypes: 84155.1 });
  assert.deepEqual(current.reviewBudgetChange.previousOverages, { packageTarball: 7.2, packageTypes: 829.9 });
  assert.deepEqual(current.sizes, { ...historical.sizes, ...approved });
  const { reviewBudgetChange: _receipt, ...withoutReceipt } = current;
  assert.deepEqual({ ...withoutReceipt, sizes: historical.sizes }, historical);
  for (const [metric, bytes] of Object.entries(approved)) {
    assert.throws(() => assertBaseline({ ...historical, sizes: { ...historical.sizes, [metric]: bytes } }, historical),
      new RegExp(`${metric} grew more than 2%`));
  }
});

test('keeps exact P6 integer limits without compounding successful measurements', async () => {
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p6-review.json', import.meta.url), 'utf8'));
  const original = structuredClone(current);
  const observed = { ...current, sizes: { ...current.sizes, packageTarball: 213122, packageTypes: 86684 } };
  assertBaseline(observed, current);
  assertBaseline(observed, current);
  assert.deepEqual(current, original);
  for (const [metric, bytes] of Object.entries({ packageTarball: 213123, packageTypes: 86685 })) {
    assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, [metric]: bytes } }, current),
      new RegExp(`${metric} grew more than 2%`));
  }
  for (const [metric, bytes] of Object.entries(current.sizes)) {
    if (metric === 'packageTarball' || metric === 'packageTypes') continue;
    assert.throws(() => assertBaseline({
      ...current, sizes: { ...current.sizes, [metric]: Math.floor(bytes * 1.02) + 1 },
    }, current), new RegExp(`${metric} grew more than 2%`));
  }
  for (const project of ['package', 'trading']) {
    for (const counter of Object.keys(current.coverage[project])) {
      const reduced = structuredClone(current);
      reduced.coverage[project][counter] -= 0.01;
      assert.throws(() => assertBaseline(reduced, current), new RegExp(`${project} ${counter} coverage regressed`));
    }
  }
});

test('keeps earlier review approvals separate from the P6-only receipt', async () => {
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p6-review.json', import.meta.url), 'utf8'));
  assert.equal(current.artifactChange.part, 'B / P6');
  assert.match(current.reviewBudgetChange.scope, /^#209 \/ #164 Part B/);
  assert.match(current.reviewBudgetChange.reason, /does not apply to P7/);
  assert.equal(current.approvedP5TradingJsGzipAllowance, undefined);
  for (const name of ['p3', 'p4', 'p5']) {
    const earlier = JSON.parse(await readFile(new URL(`../fixtures/baseline-metrics-${name}-review.json`, import.meta.url), 'utf8'));
    assert.notEqual(earlier.reviewBudgetChange.scope, current.reviewBudgetChange.scope);
    assert.notEqual(earlier.sizes.packageTarball, current.sizes.packageTarball);
    assert.notEqual(earlier.sizes.packageTypes, current.sizes.packageTypes);
    assert.throws(() => assertBaseline({
      ...earlier, sizes: { ...earlier.sizes, packageTarball: current.sizes.packageTarball },
    }, earlier), /packageTarball grew more than 2%/);
  }
});

test('does not transfer reviewed predecessor archive or declaration limits into P7', async () => {
  const current = await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url));
  const original = await readFile(new URL('../fixtures/baseline-metrics-p7-pre-release-review.json', import.meta.url));
  const p7 = JSON.parse(current);
  const prior = JSON.parse(original);
  assert.deepEqual(p7, { ...prior, sizes: { ...prior.sizes, packageTypes: 84985 } });
  assert.equal(p7.artifactChange.part, 'P7');
  assert.equal(p7.sizes.packageTarball, 225320);
  assert.equal(p7.sizes.packageTypes, 84985);
  assert.equal(p7.reviewBudgetChange, undefined);
  for (const layer of ['p3', 'p4', 'p5', 'p6']) {
    const historical = JSON.parse(await readFile(new URL(`../fixtures/baseline-metrics-${layer}-review.json`, import.meta.url), 'utf8'));
    assert.notEqual(historical.artifactChange.part, p7.artifactChange.part);
    assert.notEqual(historical.sizes.packageTarball, p7.sizes.packageTarball);
  }
});

test('applies only the actual P7 declaration approval with unchanged archive and coverage limits', async () => {
  const originalBytes = await readFile(new URL('../fixtures/baseline-metrics-p7-pre-release-review.json', import.meta.url));
  const original = JSON.parse(originalBytes);
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics.json', import.meta.url), 'utf8'));
  const receipt = JSON.parse(await readFile(new URL('../fixtures/p7-released-declarations-approval.json', import.meta.url), 'utf8'));
  assert.equal(createHash('sha256').update(originalBytes).digest('hex'),
    '47e8dd79d8d00636a6aaad72038bd0933d75d1fe3ec568d0fd36441744829a7e');
  assert.equal(receipt.previousBaselineSha256, createHash('sha256').update(originalBytes).digest('hex'));
  assert.equal(receipt.actualUserDecision, 'Approve only #210’s declaration baseline and continue (Recommended)');
  assert.equal(receipt.approvedDeclarationBaseline, 84985);
  assert.deepEqual(receipt.firstDeclarationMeasurement, { esm: 42484, commonJs: 42501, total: 84985 });
  assert.equal(receipt.firstArtifactSha256, '897c1853d22bda0a56578be3bdf0acb5681b956068c1c51ee6f8d0124338f39c');
  assert.equal(receipt.firstArtifactBytes, 229361);
  assert.equal(receipt.firstArtifactEntries, 65);
  assert.equal(receipt.previousDeclarationCap, 84155.1);
  assert.equal(receipt.firstDeclarationOverage, 829.9);
  assert.deepEqual(current, { ...original, sizes: { ...original.sizes, packageTypes: 84985 } });
  assert.throws(() => assertBaseline({
    ...original, sizes: { ...original.sizes, packageTypes: 84985 },
  }, original), /packageTypes grew more than 2%/);
  const unchanged = structuredClone(current);
  const limit = { ...current, sizes: { ...current.sizes, packageTypes: 86684, packageTarball: 229826 } };
  assertBaseline(limit, current);
  assertBaseline(limit, current);
  assert.deepEqual(current, unchanged);
  for (const [metric, bytes] of Object.entries({ packageTypes: 86685, packageTarball: 229827 })) {
    assert.throws(() => assertBaseline({
      ...limit, sizes: { ...limit.sizes, [metric]: bytes },
    }, current), new RegExp(`${metric} grew more than 2%`));
  }
  for (const [metric, bytes] of Object.entries(original.sizes)) {
    if (metric === 'packageTypes') continue;
    assert.throws(() => assertBaseline({
      ...current, sizes: { ...current.sizes, [metric]: Math.floor(bytes * 1.02) + 1 },
    }, current), new RegExp(`${metric} grew more than 2%`));
  }
  for (const project of ['package', 'trading']) {
    for (const counter of Object.keys(original.coverage[project])) {
      const lower = structuredClone(current);
      lower.coverage[project][counter] -= 0.01;
      assert.throws(() => assertBaseline(lower, current), new RegExp(`${project} ${counter} coverage regressed`));
    }
  }
});

test('uses the actual-user-approved P5 allowance exactly once: 75725 passes, 75726 fails', async () => {
  const expected = await historicalP5Baseline();
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
  const expected = await historicalP5Baseline();
  delete expected.approvedP5TradingJsGzipAllowance;
  const observed = { coverage: expected.coverage, sizes: { ...expected.sizes, tradingJsGzip: 75615 } };
  assertBaseline(observed, expected);
  assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, tradingJsGzip: 75616 } }, expected), /more than 2%/);
  assert.throws(() => assertBaseline({ ...observed, sizes: { ...observed.sizes, tradingJsGzip: 75725 } }, expected), /more than 2%/);
});

test('never applies the P5 gzip allowance to another artifact or to coverage', async () => {
  const expected = await historicalP5Baseline();
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
    const expected = await historicalP5Baseline();
    mutate(expected);
    assert.throws(() => assertBaseline({ coverage: expected.coverage, sizes: expected.sizes }, expected),
      /applies only to its original recorded baseline/);
  }
});

test('applies the approved P3 archive baseline only and preserves the complete prior record', async () => {
  const originalBytes = await readFile(new URL('../fixtures/baseline-metrics-p3-pre-review.json', import.meta.url));
  const historical = JSON.parse(originalBytes);
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p3-review.json', import.meta.url), 'utf8'));
  assert.equal(createHash('sha256').update(originalBytes).digest('hex'),
    '95147dcfd0205cbf92a13ffdb23507aa68a2b2a65463e9267594be0719091bb9');
  assert.equal(current.reviewBudgetChange.previousBaselineSha256,
    createHash('sha256').update(originalBytes).digest('hex'));
  assert.deepEqual(current.sizes, { ...historical.sizes, packageTarball: 159591 });
  assert.equal(current.sizes.packageTypes, 56256);
  assert.equal(current.reviewBudgetChange.approvedDeclarationBaselineNotUsed, 57550);
  const { reviewBudgetChange: _receipt, ...withoutReceipt } = current;
  assert.deepEqual({ ...withoutReceipt, sizes: historical.sizes }, historical);
  assert.deepEqual(current.reviewBudgetChange.firstFailedMeasurement,
    { packageTarball: 159591, packageTypes: 57550 });
  assert.deepEqual(current.reviewBudgetChange.afterClarityMeasurement,
    { packageTarball: 159912, packageTypes: 57290 });
});

test('keeps the same 2% rule on the approved archive and the original declaration baseline', async () => {
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p3-review.json', import.meta.url), 'utf8'));
  assertBaseline({ ...current, sizes: { ...current.sizes, packageTarball: 162782, packageTypes: 57381 } }, current);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTarball: 162783 },
  }, current), /packageTarball grew more than 2%/);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTypes: 57382 },
  }, current), /packageTypes grew more than 2%/);
});

test('does not transfer the P3 review allowance into the historical P4 review budget', async () => {
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p4-review.json', import.meta.url), 'utf8'));
  const p3 = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p3-review.json', import.meta.url), 'utf8'));
  assert.match(current.reviewBudgetChange.scope, /^#207 \/ #163 Part B/);
  assert.match(p3.reviewBudgetChange.scope, /^#206 \/ #163 Part A/);
  assert.notEqual(current.reviewBudgetChange.approvedArchiveBaseline, p3.reviewBudgetChange.approvedArchiveBaseline);
  assert.equal(current.artifactChange.part, 'B / P4');
  assert.deepEqual(current.sizes, {
    packageTarball: 192535, packageEsm: 98585, packageCjs: 106000, packageTypes: 71207,
    packageCss: 10043, tradingJs: 250892, tradingJsGzip: 73795, tradingCss: 21034, tradingCssGzip: 5163,
  });
});

test('limits the scoped P4 approval to two size fields and preserves the full original baseline', async () => {
  const originalBytes = await readFile(new URL('../fixtures/baseline-metrics-p4-pre-review.json', import.meta.url));
  const historical = JSON.parse(originalBytes);
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p4-review.json', import.meta.url), 'utf8'));
  const expected = { packageTarball: 192535, packageTypes: 71207 };
  assert.equal(createHash('sha256').update(originalBytes).digest('hex'),
    '46d8db7eb3ab6af2b6fba1e951abc362696f666514e766a1e9115c0065874d5f');
  assert.equal(current.reviewBudgetChange.previousBaselineSha256,
    createHash('sha256').update(originalBytes).digest('hex'));
  assert.deepEqual(current.sizes, { ...historical.sizes, ...expected });
  assert.deepEqual(current.reviewBudgetChange.firstFailedMeasurement, expected);
  assert.equal(current.reviewBudgetChange.approvedArchiveBaseline, expected.packageTarball);
  assert.equal(current.reviewBudgetChange.approvedDeclarationBaseline, expected.packageTypes);
  const { reviewBudgetChange: _receipt, ...withoutReceipt } = current;
  assert.deepEqual({ ...withoutReceipt, sizes: historical.sizes }, historical);
  assert.throws(() => assertBaseline({ ...historical, sizes: { ...historical.sizes, ...expected } }, historical),
    /packageTarball grew more than 2%/);
});

test('keeps the normal 2% integer boundaries on the approved P4 archive and declarations', async () => {
  const current = JSON.parse(await readFile(new URL('../fixtures/baseline-metrics-p4-review.json', import.meta.url), 'utf8'));
  assertBaseline({ ...current, sizes: { ...current.sizes, packageTarball: 196385, packageTypes: 72631 } }, current);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTarball: 196386 },
  }, current), /packageTarball grew more than 2%/);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTypes: 72632 },
  }, current), /packageTypes grew more than 2%/);
});

test('does not transfer either earlier review approval into the historical P5 review budget', async () => {
  const current = await reviewedP5Baseline();
  assert.equal(current.artifactChange.part, 'A / P5');
  assert.match(current.reviewBudgetChange.scope, /^#208 \/ #164 Part A/);
  assert.equal(current.sizes.packageTarball, 175111);
  assert.equal(current.sizes.packageTypes, 75685);
  for (const name of ['p3', 'p4']) {
    const historical = JSON.parse(await readFile(new URL(`../fixtures/baseline-metrics-${name}-review.json`, import.meta.url), 'utf8'));
    assert.equal(historical.approvedP5TradingJsGzipAllowance, undefined);
    assert.notEqual(historical.reviewBudgetChange.approvedArchiveBaseline, current.sizes.packageTarball);
  }
});

test('preserves the complete original P5 record and advances only the approved archive baseline', async () => {
  const originalBytes = await readFile(new URL('../fixtures/baseline-metrics-p5-pre-review.json', import.meta.url));
  const historical = JSON.parse(originalBytes);
  const current = await reviewedP5Baseline();
  assert.equal(createHash('sha256').update(originalBytes).digest('hex'),
    '5f87e41b72410779d0af2ef7564af95108ee1e0b90972d63b9425ebaa472102d');
  assert.equal(current.reviewBudgetChange.previousBaselineSha256,
    createHash('sha256').update(originalBytes).digest('hex'));
  assert.deepEqual(current.sizes, { ...historical.sizes, packageTarball: 175111 });
  assert.equal(current.reviewBudgetChange.approvedArchiveBaseline, 175111);
  assert.deepEqual(current.reviewBudgetChange.firstFailedMeasurement, { packageTarball: 175111 });
  assert.equal(current.reviewBudgetChange.firstArtifactSha256,
    '1b82158387e6f197e489d7e4c22edcde838a0fc1df158812678d9880e8263794');
  assert.equal(current.reviewBudgetChange.previousArchiveCap, 172846.14);
  assert.equal(current.reviewBudgetChange.previousArchiveOverrun, 2264.86);
  assert.equal(current.approvedP5TradingJsGzipAllowance, undefined);
  const { reviewBudgetChange: _receipt, ...withoutReceipt } = current;
  assert.deepEqual({
    ...withoutReceipt, sizes: historical.sizes,
    approvedP5TradingJsGzipAllowance: historical.approvedP5TradingJsGzipAllowance,
  }, historical);
  assert.throws(() => assertBaseline({ ...historical, sizes: current.sizes }, historical),
    /packageTarball grew more than 2%/);
});

test('keeps exact P5 archive and unchanged declaration boundaries without compounding', async () => {
  const current = await reviewedP5Baseline();
  const original = structuredClone(current);
  const observed = { ...current, sizes: { ...current.sizes, packageTarball: 178613, packageTypes: 77198 } };
  assertBaseline(observed, current);
  assertBaseline(observed, current);
  assert.deepEqual(current, original);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTarball: 178614 },
  }, current), /packageTarball grew more than 2%/);
  assert.throws(() => assertBaseline({
    ...current, sizes: { ...current.sizes, packageTypes: 77199 },
  }, current), /packageTypes grew more than 2%/);
});

test('keeps the unused historical gzip allowance on its old vector, not the new archive baseline', async () => {
  const historical = await historicalP5Baseline();
  const current = await reviewedP5Baseline();
  assert.equal(historical.approvedP5TradingJsGzipAllowance.bytes, 110);
  assert.equal(historical.approvedP5TradingJsGzipAllowance.baselineSizesSha256,
    '5e8ad7efb5c8f9dc59ad0308844c5643bdaf2e5d81d7f5fd24fdefaee249ba15');
  assertBaseline({ ...current, sizes: { ...current.sizes, tradingJsGzip: 75615 } }, current);
  for (const size of [75616, 75725]) {
    assert.throws(() => assertBaseline({
      ...current, sizes: { ...current.sizes, tradingJsGzip: size },
    }, current), /tradingJsGzip grew more than 2%/);
  }
  const reused = { ...current, approvedP5TradingJsGzipAllowance: historical.approvedP5TradingJsGzipAllowance };
  assert.throws(() => assertBaseline(current, reused), /applies only to its original recorded baseline/);
  for (const [metric, size] of Object.entries(current.sizes)) {
    if (metric === 'packageTarball') continue;
    assert.throws(() => assertBaseline({
      ...current, sizes: { ...current.sizes, [metric]: Math.floor(size * 1.02) + 1 },
    }, current), new RegExp(`${metric} grew more than 2%`));
  }
});
