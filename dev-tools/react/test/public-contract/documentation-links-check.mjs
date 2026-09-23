// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkDocumentationLinks, checkInstalledDocumentationLinks, markdownAnchors, relativeMarkdownLinks } from './documentation-links.mjs';
import { documentationFiles, loadDocumentation } from './documents.mjs';

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

test('installed guide links cannot rely on missing or escaped repository files', async context => {
  const root = await mkdtemp(join(tmpdir(), 'drasi-installed-docs-'));
  context.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, 'docs'));
  for (const file of documentationFiles) await writeFile(join(root, file), '# Guide\n');
  let documents = await loadDocumentation(root);
  documents['README.md'] = '[Start](docs/getting-started.md#guide)';
  assert.equal((await checkInstalledDocumentationLinks(root, documents)).length, 1);
  documents['README.md'] = '[Missing local example](examples/README.md)';
  await assert.rejects(checkInstalledDocumentationLinks(root, documents), /ENOENT/);
  documents['README.md'] = '[Repository alias](../README.md)';
  await assert.rejects(checkInstalledDocumentationLinks(root, documents), /escapes package/);
  documents['README.md'] = '[Bad heading](docs/reference.md#missing)';
  await assert.rejects(checkInstalledDocumentationLinks(root, documents), /Missing installed documentation anchor/);
  await writeFile(join(root, 'docs/unchecked.md'), '# Unchecked\n');
  await assert.rejects(loadDocumentation(root), /Every shipped guide/);
});

test('repository-only example links still target a real canonical path and heading', async () => {
  await assert.rejects(checkDocumentationLinks({
    'dev-tools/react/README.md': '[Wrong example](https://github.com/drasi-project/drasi-server/blob/main/dev-tools/react/examples/README.md#not-a-heading)',
  }), /Missing documentation anchor/);
});
