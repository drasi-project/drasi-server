// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { gunzipSync } from 'node:zlib';

const archive = JSON.parse(gunzipSync(readFileSync(new URL('../fixtures/p6-predecessor-axe-evidence.json.gz', import.meta.url))));
const baseline = JSON.parse(readFileSync(new URL('../fixtures/trading-axe-baseline.json', import.meta.url)));
const files = new Map(archive.files.map(file => [file.path, file]));
const matrix = JSON.parse(files.get('colour-approval-matrix.json').content);
const states = JSON.parse(files.get('states/inventory.json').content);
const pairs = [...states.pairs, ...matrix.additionalPairs];

test('retains the full measured reports, incomplete checks and exact predecessor provenance', () => {
  assert.equal(archive.predecessor, '569b1d26e558b838d3a572bf7e2e5fbd6280eeaf');
  assert.equal(files.size, archive.files.length);
  for (const file of archive.files) {
    assert.equal(createHash('sha256').update(file.content).digest('hex'), file.sha256, file.path);
  }
  const reports = archive.files.filter(file => file.path.endsWith('-axe.json'));
  assert.equal(reports.length, 98);
  for (const file of reports) {
    const report = JSON.parse(file.content);
    assert.equal(report.testEngine.name, 'axe-core');
    assert.equal(report.testEngine.version, '4.10.3');
    assert(Array.isArray(report.violations) && Array.isArray(report.incomplete) && Array.isArray(report.passes));
  }
  assert.equal(matrix.strictPairedStates, 47);
  assert.equal(matrix.naturalPostclickObservations, 2);
  assert.equal(matrix.inventory.length, 19);
  assert.deepEqual(matrix.unresolved, []);
});

test('every regression fingerprint is exactly bounded by recorded element/state/browser evidence', () => {
  assert.equal(baseline.predecessor, archive.predecessor);
  assert.equal(baseline.matrixSha256, files.get('colour-approval-matrix.json').sha256);
  assert.equal(baseline.fingerprints.length, 19);
  assert.equal(new Set(baseline.fingerprints.map(entry => entry.id)).size, 19);
  assert.equal(baseline.fingerprints.reduce((count, entry) => count + entry.contexts.length, 0), 86);
  for (const fingerprint of baseline.fingerprints) {
    assert.equal(fingerprint.rule, 'color-contrast');
    assert.equal(fingerprint.impact, 'serious');
    assert(fingerprint.selector.length > 0);
    for (const context of fingerprint.contexts) {
      const pair = pairs.find(entry => entry.browser === context.browser && entry.state === context.state);
      assert(pair, `${fingerprint.id} ${context.browser}/${context.state}`);
      const finding = pair.findings.find(entry =>
        entry.classification === 'verified-unchanged-predecessor-finding' &&
        JSON.stringify(entry.identity) === JSON.stringify(fingerprint.identity) &&
        JSON.stringify(entry.data) === JSON.stringify(fingerprint.metrics));
      assert(finding, `${fingerprint.id} has no identical measured finding`);
      assert.deepEqual(finding.current.style, context.style);
      assert.deepEqual(finding.predecessor.data, fingerprint.metrics);
      assert.deepEqual(finding.predecessor.style, context.style);
    }
  }
});
