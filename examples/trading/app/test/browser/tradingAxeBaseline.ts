// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { AxeResults } from 'axe-core';
import type { JSHandle, Locator, Page } from '@playwright/test';
import baseline from '../fixtures/trading-axe-baseline.json' with { type: 'json' };

export type AuditBrowser = 'chromium' | 'firefox' | 'webkit';

export interface NodeObservation {
  identity: { tag: string; text: string; inputType: string | null; placeholder: string | null };
  style: {
    foreground: string;
    background: string;
    fontFamily: string;
    fontSize: string;
    fontWeight: string;
    lineHeight: string;
  };
  selectors: string[];
}

export interface ObservedViolation {
  rule: string;
  impact: string | null | undefined;
  target: unknown;
  html: string;
  metrics: unknown;
  observation: NodeObservation | null;
}

export const tradingContrastBaseline = baseline;

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** A non-regression comparison, not a zero-violation or conformance result. */
export function compareTradingAudit(browser: AuditBrowser, state: string, violations: readonly ObservedViolation[]) {
  const candidates = baseline.fingerprints.flatMap(fingerprint =>
    fingerprint.contexts.filter(context => context.browser === browser && context.state === state)
      .map(context => ({ fingerprint, context })),
  );
  if (!candidates.length) throw new Error(`No measured predecessor boundary for ${browser}/${state}`);
  const retained: { baselineId: string; predecessorRatio: number; violation: ObservedViolation }[] = [];
  const regressions: { reason: string; violation: ObservedViolation }[] = [];
  const used = new Set<string>();
  for (const violation of violations) {
    const actual = violation.observation;
    const metrics = violation.metrics;
    const matched = candidates.find(({ fingerprint, context }) => {
      if (!actual || !record(metrics) || violation.rule !== fingerprint.rule || violation.impact !== fingerprint.impact) return false;
      if (!actual.selectors.includes(fingerprint.selector)) return false;
      if (Object.entries(fingerprint.identity).some(([key, value]) => Reflect.get(actual.identity, key) !== value)) return false;
      if (Object.entries(context.style).some(([key, value]) => Reflect.get(actual.style, key) !== value)) return false;
      return Object.entries(fingerprint.metrics).every(([key, value]) =>
        key === 'contrastRatio'
          ? typeof metrics[key] === 'number' && Number.isFinite(metrics[key]) && metrics[key] >= fingerprint.metrics.contrastRatio
          : metrics[key] === value,
      );
    });
    if (!matched) {
      regressions.push({ reason: 'New/worsened or unrecognized element, state, rule, impact, colors, typography or contrast ratio', violation });
    } else if (used.has(matched.fingerprint.id)) {
      regressions.push({ reason: 'An additional element repeats a previously measured fingerprint', violation });
    } else {
      used.add(matched.fingerprint.id);
      retained.push({ baselineId: matched.fingerprint.id, predecessorRatio: matched.fingerprint.metrics.contrastRatio, violation });
    }
  }
  return {
    policy: 'No new/worsened Trading accessibility violations under preserved legacy colors; not a zero-axe, WCAG or human AT pass',
    predecessor: baseline.predecessor,
    matrixSha256: baseline.matrixSha256,
    browser,
    state,
    observedViolationCount: violations.length,
    retainedViolationCount: retained.length,
    retained,
    regressions,
  };
}

/** Capture actual rendered metadata before axe runs, including transient Copied feedback. */
export async function captureTradingNodes(scope: Locator): Promise<JSHandle<Map<Element, NodeObservation>>> {
  const selectors = [...new Set(baseline.fingerprints.map(fingerprint => fingerprint.selector))];
  return scope.evaluateHandle((root, selectors) => {
    const entries: [Element, NodeObservation][] = [root, ...root.querySelectorAll('*')].map(element => {
      const style = getComputedStyle(element);
      return [element, {
        identity: {
          tag: element.tagName.toLowerCase(),
          text: element.textContent?.replace(/\s+/g, ' ').trim() ?? '',
          inputType: element instanceof HTMLInputElement ? element.type : null,
          placeholder: element.getAttribute('placeholder'),
        },
        style: {
          foreground: style.color, background: style.backgroundColor,
          fontFamily: style.fontFamily, fontSize: style.fontSize,
          fontWeight: style.fontWeight, lineHeight: style.lineHeight,
        },
        selectors: selectors.filter(selector => element.matches(selector)),
      }];
    });
    return new Map(entries);
  }, selectors);
}

export async function observeTradingViolations(
  page: Page, result: AxeResults, snapshot: JSHandle<Map<Element, NodeObservation>>,
): Promise<ObservedViolation[]> {
  const observed: ObservedViolation[] = [];
  for (const violation of result.violations) {
    for (const node of violation.nodes) {
      const selector = node.target.length === 1 && typeof node.target[0] === 'string' ? node.target[0] : undefined;
      let observation: NodeObservation | null = null;
      if (selector) {
        const locator = page.locator(selector);
        if (await locator.count() === 1) {
          const element = await locator.elementHandle();
          if (element) {
            try {
              observation = await snapshot.evaluate((samples, target) => samples.get(target) ?? null, element);
            } finally {
              await element.dispose();
            }
          }
        }
      }
      const metrics: unknown = [...node.any, ...node.all, ...node.none].find(check => check.id === 'color-contrast')?.data;
      observed.push({ rule: violation.id, impact: violation.impact, target: node.target, html: node.html, metrics, observation });
    }
  }
  return observed;
}
