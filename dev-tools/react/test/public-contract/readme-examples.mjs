// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';

export const requiredReadmeExamples = [
  'quickstart.tsx', 'query-options.ts', 'client.ts', 'auth.ts', 'controlled.tsx',
  'data-table.tsx', 'sort-uncontrolled.tsx', 'sort-controlled.tsx',
  'query-states.tsx', 'composed-table.tsx',
];

/** Only an explicit first-line marker opts a README fence into compilation. */
export function extractReadmeExamples(markdown) {
  const examples = new Map();
  const lines = markdown.split(/\r?\n/);
  let fence = null;
  function collect(closed) {
    const first = fence.lines[0] ?? '';
    if (!first.startsWith('// @drasi-docs:')) return;
    assert(closed, `Unclosed marked README fence at line ${fence.line}`);
    const match = /^\/\/ @drasi-docs: ([a-z][a-z0-9-]*\.(?:ts|tsx))$/.exec(first);
    assert(match, `Invalid README example filename/marker at line ${fence.line}`);
    const name = match[1];
    const languages = name.endsWith('.tsx') ? ['tsx'] : ['ts', 'typescript'];
    assert(languages.includes(fence.language), `README ${name} must use a ${languages.join('/')} fence`);
    assert(!examples.has(name), `Duplicate marked README example: ${name}`);
    const code = `${fence.lines.join('\n')}\n`;
    assert(!/@ts-(?:ignore|expect-error|nocheck)\b/.test(code), `README ${name} suppresses compiler checks`);
    examples.set(name, { name, code, line: fence.line });
  }
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (fence) {
      const closing = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(line);
      if (closing && closing[1][0] === fence.delimiter[0] && closing[1].length >= fence.delimiter.length) {
        collect(true);
        fence = null;
      } else {
        fence.lines.push(line);
      }
    } else {
      const opening = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
      if (opening) {
        fence = {
          delimiter: opening[1], language: opening[2].trim().split(/[ \t]/)[0],
          line: index + 2, lines: [],
        };
      }
    }
  }
  if (fence) collect(false);
  const missing = requiredReadmeExamples.filter(name => !examples.has(name));
  assert.equal(missing.length, 0, `Missing marked README examples: ${missing.join(', ')}`);
  return [...examples.values()];
}
