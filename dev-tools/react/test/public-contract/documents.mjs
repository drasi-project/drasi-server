// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';

export const documentationFiles = [
  'README.md', 'CHANGELOG.md',
  'docs/getting-started.md', 'docs/reference.md', 'docs/connection.md',
  'docs/migration.md', 'docs/testing.md',
];

export async function loadDocumentation(packageRoot) {
  const guides = (await readdir(join(packageRoot, 'docs'), { recursive: true }))
    .map(file => `docs/${file}`).sort();
  assert.deepEqual(guides, documentationFiles.filter(file => file.startsWith('docs/')).sort(),
    'Every shipped guide must be included in the executable documentation contract');
  return Object.fromEntries(await Promise.all(documentationFiles.map(async file =>
    [file, await readFile(join(packageRoot, file), 'utf8')],
  )));
}
