// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { checkDocumentationLinks, markdownAnchors, relativeMarkdownLinks } from './documentation-links.mjs';

test('matches actual historical headings, punctuation, duplicate slugs and stable aliases', () => {
  const anchors = markdownAnchors([
    '## P6 / #164 Part B migration', '## Repeated', '## Repeated',
    '<a id="p6-measured-evidence"></a>', '#### Historical P6 measured evidence',
    '```md', '## Not a real heading', '```',
  ].join('\n'));
  assert.deepEqual([...anchors], [
    'p6--164-part-b-migration', 'repeated', 'repeated-1',
    'historical-p6-measured-evidence', 'p6-measured-evidence',
  ]);
});

test('validates related repository links without interpreting example code or external URLs', () => {
  assert.deepEqual(relativeMarkdownLinks([
    '[Install](#installation-and-entrypoints)',
    '[History](../../examples/trading/TESTING.md#historical-p6-measured-evidence)',
    '[External](https://drasi.io)', '```tsx', '[Not prose](missing.md)', '```',
  ].join('\n')), ['#installation-and-entrypoints', '../../examples/trading/TESTING.md#historical-p6-measured-evidence']);
});

test('related README/changelog/example/evidence paths and anchors resolve', async () => {
  const proof = await checkDocumentationLinks();
  assert(proof.checked.length > 0);
  assert(proof.checked.some(link => link.target.endsWith('#historical-p6-measured-evidence')));
  assert(proof.checked.some(link => link.target.endsWith('#run-only-the-simulated-showcase')));
});

test('missing anchors or files fail instead of blessing stale evidence links', async () => {
  await assert.rejects(checkDocumentationLinks({
    'dev-tools/react/README.md': '[Missing](../../examples/trading/TESTING.md#missing-evidence)',
  }), /Missing documentation anchor/);
  await assert.rejects(checkDocumentationLinks({
    'dev-tools/react/README.md': '[Missing](missing-guide.md)',
  }), /ENOENT/);
});
