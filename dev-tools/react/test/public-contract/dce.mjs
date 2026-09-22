// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { build } from 'vite';
import ts from 'typescript';

// This runner is copied into the installed consumer, never executed through a source alias.
const directory = await mkdtemp(join(process.cwd(), '.contract-dce-'));
try {
  const entry = join(directory, 'consumer.mjs');
  await writeFile(entry, "export { DataTable, queryTableState } from '@drasi/react/components';\n");
  const result = await build({
    configFile: false,
    root: process.cwd(),
    logLevel: 'silent',
    build: {
      write: false,
      minify: false,
      lib: { entry, formats: ['es'] },
      rollupOptions: { external: ['react', 'react-dom', 'react/jsx-runtime', 'clsx'] },
    },
  });
  const outputs = Array.isArray(result) ? result : [result];
  assert.equal(outputs.length, 1);
  assert('output' in outputs[0]);
  const chunk = outputs[0].output.find(item => item.type === 'chunk' && item.isEntry);
  assert(chunk?.type === 'chunk');
  assert.deepEqual([...chunk.exports].sort(), ['DataTable', 'queryTableState']);
  const identifiers = new Set();
  const retainedNames = new Set();
  const source = ts.createSourceFile('consumer.js', chunk.code, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  function visit(node) {
    if (ts.isIdentifier(node)) identifiers.add(node.text);
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === '__name') {
      for (const argument of node.arguments) {
        if (ts.isStringLiteral(argument)) retainedNames.add(argument.text);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert(identifiers.has('DataTable') && identifiers.has('queryTableState'), 'Used controls were not retained');
  assert(!identifiers.has('QueryTable') && !retainedNames.has('QueryTable'),
    'Unused QueryTable implementation is retained by the installed consumer');
  console.log('Packed ESM consumer retains DataTable/queryTableState and eliminates unused QueryTable.');
} finally {
  await rm(directory, { recursive: true, force: true });
}
