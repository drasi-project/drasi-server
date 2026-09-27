// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it } from 'vitest';
import {
  compareTradingAudit, tradingContrastBaseline, type AuditBrowser, type ObservedViolation,
} from '../browser/tradingAxeBaseline';

const browsers: readonly AuditBrowser[] = ['chromium', 'firefox', 'webkit'];
const cases = tradingContrastBaseline.fingerprints.flatMap(fingerprint =>
  browsers.flatMap(browser => fingerprint.contexts.filter(context => context.browser === browser)
    .map(context => ({ browser, context, fingerprint }))),
);

function observed(entry = cases[0]): ObservedViolation {
  return {
    rule: entry.fingerprint.rule, impact: entry.fingerprint.impact,
    target: [entry.fingerprint.selector], html: '<measured-node>',
    metrics: structuredClone(entry.fingerprint.metrics),
    observation: {
      identity: structuredClone(entry.fingerprint.identity),
      style: structuredClone(entry.context.style),
      selectors: [entry.fingerprint.selector],
    },
  };
}

describe('strict user-preserved Trading contrast boundary', () => {
  it.each(cases)('retains measured $fingerprint.id in $browser/$context.state without calling it zero axe', entry => {
    const report = compareTradingAudit(entry.browser, entry.context.state, [observed(entry)]);
    expect(report.regressions).toEqual([]);
    expect(report.retainedViolationCount).toBe(1);
    expect(report.observedViolationCount).toBe(1);
    expect(report.policy).toContain('not a zero-axe');
  });

  it.each([
    ['new rule', (finding: ObservedViolation) => { finding.rule = 'label'; }],
    ['changed impact', (finding: ObservedViolation) => { finding.impact = 'moderate'; }],
    ['missing target', (finding: ObservedViolation) => { finding.observation = null; }],
    ['new selector', (finding: ObservedViolation) => { if (finding.observation) finding.observation.selectors = ['.new']; }],
    ['new text', (finding: ObservedViolation) => { if (finding.observation) finding.observation.identity.text = 'new'; }],
    ['changed family', (finding: ObservedViolation) => { if (finding.observation) finding.observation.style.fontFamily = 'serif'; }],
    ['changed font weight', (finding: ObservedViolation) => { if (finding.observation) finding.observation.style.fontWeight = '100'; }],
    ['changed line height', (finding: ObservedViolation) => { if (finding.observation) finding.observation.style.lineHeight = '1px'; }],
    ['missing metrics', (finding: ObservedViolation) => { finding.metrics = null; }],
    ['lower ratio', (finding: ObservedViolation) => { finding.metrics = { ...cases[0].fingerprint.metrics, contrastRatio: 3.80 }; }],
    ['nonfinite ratio', (finding: ObservedViolation) => { finding.metrics = { ...cases[0].fingerprint.metrics, contrastRatio: Infinity }; }],
    ['changed color', (finding: ObservedViolation) => { finding.metrics = { ...cases[0].fingerprint.metrics, fgColor: '#ef4443' }; }],
    ['changed size', (finding: ObservedViolation) => { finding.metrics = { ...cases[0].fingerprint.metrics, fontSize: '8px' }; }],
  ] as const)('fails %s instead of excluding the contrast rule', (_label, change) => {
    const finding = observed();
    change(finding);
    expect(compareTradingAudit('chromium', 'dashboard', [finding]).regressions).toHaveLength(1);
  });

  it('fails newly duplicated bad elements', () => {
    const report = compareTradingAudit('chromium', 'dashboard', [observed(), observed()]);
    expect(report.retainedViolationCount).toBe(1);
    expect(report.regressions).toHaveLength(1);
  });

  it('does not extend a known element to another state or browser', () => {
    expect(compareTradingAudit('chromium', 'add-position', [observed()]).regressions).toHaveLength(1);
    const copied = cases.find(entry => entry.fingerprint.identity.text === 'Copied!');
    if (!copied) throw new Error('Missing measured clipboard fingerprint');
    expect(compareTradingAudit('firefox', 'inspector-react', [observed(copied)]).regressions).toHaveLength(1);
    expect(() => compareTradingAudit('chromium', 'unmeasured', [])).toThrow('No measured predecessor boundary');
  });

  it('permits disappearance of a violation while reporting the observed count honestly', () => {
    const report = compareTradingAudit('chromium', 'dashboard', []);
    expect(report.regressions).toEqual([]);
    expect(report.observedViolationCount).toBe(0);
    expect(report.retained).toEqual([]);
  });
});
