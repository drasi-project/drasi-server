// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { extractInstallRecipes } from './install-recipes.mjs';
import { loadDocumentation } from './documents.mjs';

const markdown = Object.values(await loadDocumentation(fileURLToPath(new URL('../../', import.meta.url)))).join('\n');

test('extracts the two literal primary install recipes and measured React peers', () => {
  const recipes = extractInstallRecipes(markdown);
  assert.equal(recipes.size, 2);
  assert(recipes.get('copied-local').args.includes('--install-links'));
  assert.equal(recipes.get('copied-local').node, '24.19.0');
  assert.equal(recipes.get('copied-local').npm, '11.17.0');
  assert(recipes.get('tarball').args.includes('--ignore-scripts'));
  assert.equal(extractInstallRecipes(markdown.replaceAll('\n', '\r\n')).size, 2);
});

test('rejects missing, duplicate, symlink-default and lifecycle-enabled install recipes', () => {
  const recipes = extractInstallRecipes(markdown);
  const copied = `\`\`\`sh\n# @drasi-install: copied-local (Node 24.19.0, npm 11.17.0)\n${recipes.get('copied-local').code}\n\`\`\``;
  for (const invalid of [
    markdown.replace('# @drasi-install: copied-local', '# removed marker'),
    markdown + '\n' + copied,
    markdown.replace('--ignore-scripts --install-links "$DRASI_PACKAGE_DIR"', '--ignore-scripts "$DRASI_PACKAGE_DIR"'),
    markdown.replace('--ignore-scripts --install-links "$DRASI_PACKAGE_DIR"', '--install-links "$DRASI_PACKAGE_DIR"'),
    markdown.replace('react@18.3.1 react-dom@18.3.1', 'react@19 react-dom@19'),
    markdown.replace('npm install --save-exact --ignore-scripts --install-links', 'npm link'),
    markdown.replace('# @drasi-install: copied-local (Node 24.19.0, npm 11.17.0)',
      '# @drasi-install: copied-local (Node 22.20.0, npm 10.9.3)'),
  ]) assert.throws(() => extractInstallRecipes(invalid));
});

test('rejects unsafe or additional shell operations rather than evaluating documentation', () => {
  assert.throws(() => extractInstallRecipes(markdown.replace('DRASI_PACKAGE_DIR="/absolute/path/to/drasi-server/dev-tools/react"',
    'DRASI_PACKAGE_DIR="$(touch unsafe)"')));
  assert.throws(() => extractInstallRecipes(markdown.replace('react@18.3.1 react-dom@18.3.1',
    'react@18.3.1 react-dom@18.3.1 && echo unsafe')));
});
