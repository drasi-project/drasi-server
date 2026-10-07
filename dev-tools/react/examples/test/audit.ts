// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import AxeBuilder from '@axe-core/playwright';
import { expect, type Page, type TestInfo } from '@playwright/test';

export async function audit(page: Page, info: TestInfo, name: string, scope?: string) {
  let builder = new AxeBuilder({ page });
  if (scope) builder = builder.include(scope);
  const result = await builder.analyze();
  await info.attach(`axe-${name}`, {
    body: JSON.stringify({
      scope: scope ?? 'whole document', browser: info.project.name,
      version: page.context().browser()?.version(), url: page.url(), result,
      humanReview: 'Not performed; automation is not assistive-technology acceptance.',
    }, null, 2),
    contentType: 'application/json',
  });
  expect(result.violations, `Full-rule axe: ${name}; incomplete results retained in attachment`).toEqual([]);
}
