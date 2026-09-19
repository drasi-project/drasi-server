// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { expect, type Locator, type Page, type TestInfo } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {
  captureTradingNodes, compareTradingAudit, observeTradingViolations, type AuditBrowser,
} from './tradingAxeBaseline';

export async function audit(
  page: Page, testInfo: TestInfo, name: string, soft = false, activeModal?: Locator,
  tradingState?: { state: string; browser: AuditBrowser },
): Promise<void> {
  const builder = new AxeBuilder({ page });
  let selector: string | undefined;
  if (activeModal) {
    await expect(activeModal).toBeVisible();
    await expect(activeModal).toHaveAttribute('aria-modal', 'true');
    selector = await activeModal.evaluate(element => {
      if (!element.id || element.closest('[aria-hidden="true"], [inert]')) {
        throw new Error('Scoped axe audit requires an active, named Modal root');
      }
      return `#${CSS.escape(element.id)}`;
    });
    builder.include(selector);
  }
  const snapshot = tradingState ? await captureTradingNodes(activeModal ?? page.locator('body')) : null;
  try {
    const result = await builder.analyze();
    await testInfo.attach(`${name}-axe-scope.json`, {
      body: JSON.stringify({ scope: activeModal ? 'active-modal' : 'full-document', selector }),
      contentType: 'application/json',
    });
    await testInfo.attach(`${name}-axe.json`, {
      body: JSON.stringify(result, null, 2), contentType: 'application/json',
    });
    const assertion = soft ? expect.soft : expect;
    if (tradingState && snapshot) {
      const observed = await observeTradingViolations(page, result, snapshot);
      const comparison = compareTradingAudit(tradingState.browser, tradingState.state, observed);
      await testInfo.attach(`${name}-non-regression.json`, {
        body: JSON.stringify({
          ...comparison, incompleteCount: result.incomplete.length,
          incompleteReviewRequired: result.incomplete.length > 0,
          rawAxeReport: `${name}-axe.json`,
        }, null, 2),
        contentType: 'application/json',
      });
      testInfo.annotations.push({
        type: 'preserved-trading-accessibility-findings',
        description: `${comparison.retainedViolationCount} observed violations retained; ${result.incomplete.length} incomplete rules require review. Non-regression only, not zero axe/WCAG/human AT.`,
      });
      console.log(`Trading a11y non-regression ${tradingState.browser}/${tradingState.state}: ${comparison.retainedViolationCount} retained violations, ${comparison.regressions.length} regressions, ${result.incomplete.length} incomplete rules; not contrast-clean.`);
      assertion(comparison.regressions, `${name}: reject new/worsened findings; all raw violations and incomplete checks remain attached`).toEqual([]);
    } else {
      const details = result.violations.map(violation => ({
        id: violation.id, impact: violation.impact, description: violation.description,
        nodes: violation.nodes.map(node => ({ target: node.target, html: node.html, summary: node.failureSummary })),
      }));
      assertion(details, `${name}: full axe results, including incomplete checks, are attached`).toEqual([]);
    }
  } finally {
    await snapshot?.dispose();
  }
}

export async function settleTradingAudit(page: Page, scope: Locator): Promise<void> {
  await page.mouse.move(0, 0);
  await expect.poll(() => scope.evaluate(element => element.getAnimations({ subtree: true })
    .filter(animation => animation.playState === 'running' &&
      Number.isFinite(animation.effect?.getComputedTiming().endTime)).length)).toBe(0);
}
