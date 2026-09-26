// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import test from 'node:test';
import { extractReadmeExamples, requiredReadmeExamples } from './readme-examples.mjs';

function fence(name, code = 'export {};', language = name.endsWith('.tsx') ? 'tsx' : 'ts') {
  return `\`\`\`${language}\n// @drasi-docs: ${name}\n${code}\n\`\`\`\n`;
}

const required = requiredReadmeExamples.map(name => fence(name)).join('\n');

test('extracts only first-line-marked fences, preserving README code and line numbers', () => {
  const preamble = '# Usage\n\n```ts\nnot valid TypeScript and deliberately unmarked\n```\n\n';
  const examples = extractReadmeExamples(preamble + required);
  assert.deepEqual(examples.map(example => example.name), requiredReadmeExamples);
  assert.equal(examples[0].code, '// @drasi-docs: quickstart.tsx\nexport {};\n');
  assert.equal(examples[0].line, 8);
});

test('accepts additional safe marked examples and CRLF README files', () => {
  const examples = extractReadmeExamples((required + fence('additional.ts')).replaceAll('\n', '\r\n'));
  assert.equal(examples.length, requiredReadmeExamples.length + 1);
});

test('does not extract fences quoted inside an unmarked wider fence with metadata', () => {
  const quoted = `\`\`\`\`text title="not runnable"\n${fence('client.ts')}\`\`\`\`\n`;
  assert.equal(extractReadmeExamples(quoted + required).length, requiredReadmeExamples.length);
});

test('requires every named runnable example, including the composition recipes', () => {
  for (const name of requiredReadmeExamples) {
    const incomplete = requiredReadmeExamples.filter(example => example !== name).map(example => fence(example)).join('\n');
    assert.throws(() => extractReadmeExamples(incomplete),
      error => error.message.includes(`Missing marked README examples: ${name}`));
  }
});

test('rejects duplicate names instead of silently overwriting an example', () => {
  assert.throws(() => extractReadmeExamples(required + fence('client.ts')), /Duplicate.*client\.ts/);
});

test('a marker after the first code line does not opt a fence in', () => {
  const misplaced = required.replace('// @drasi-docs: quickstart.tsx\n', '\n// @drasi-docs: quickstart.tsx\n');
  assert.throws(() => extractReadmeExamples(misplaced), /Missing marked README examples: quickstart\.tsx/);
});

test('rejects path traversal, non-TypeScript languages and unterminated marked fences', () => {
  assert.throws(() => extractReadmeExamples(required + fence('../outside.ts')), /Invalid README example filename/);
  assert.throws(() => extractReadmeExamples(required + fence('shell.ts', 'export {};', 'sh')), /must use a ts\/typescript fence/);
  assert.throws(() => extractReadmeExamples(required + '```ts\n// @drasi-docs: unfinished.ts\n'), /Unclosed marked README fence/);
});

test('rejects diagnostic suppression in runnable documentation', () => {
  for (const directive of ['@ts-ignore', '@ts-expect-error', '@ts-nocheck']) {
    assert.throws(() => extractReadmeExamples(required + fence('unchecked.ts', `// ${directive}\nexport {};`)),
      /suppresses compiler checks/);
  }
});
