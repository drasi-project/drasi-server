// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { renderServerConfig } from '../scripts/config.mjs';

const template = await readFile(new URL('../config/server.yaml', import.meta.url), 'utf8');
const ports = { restPort: 15380, feedPort: 15381, ssePort: 15382 };

test('declarative setup installs concrete numeric ports without modifying query or bootstrap fields', () => {
  const result = renderServerConfig(template, ports);
  assert(!result.includes('${'));
  for (const port of Object.values(ports)) {
    assert(result.includes(`port: ${port}\n`));
    assert(!result.includes(`port: "${port}"`));
  }
  assert(result.includes('filePaths: ["./readings.jsonl"]'));
  assert.equal(result.match(/queryLanguage: Cypher/g)?.length, 2);
  assert(result.includes('id: cold-chain'));
});

test('rendering rejects invalid ports, missing/repeated markers and unresolved configuration', () => {
  for (const ssePort of [0, 65536, -1, 1.5, '15382', NaN]) {
    assert.throws(() => renderServerConfig(template, { ...ports, ssePort }), /Invalid allocated/);
  }
  assert.throws(() => renderServerConfig(template.replace('"${EXAMPLE_SSE_PORT}"', '1234'), ports), /exactly one/);
  assert.throws(() => renderServerConfig(template + '\nport: "${EXAMPLE_SSE_PORT}"', ports), /exactly one/);
  assert.throws(() => renderServerConfig(template + '\nextra: "${UNEXPECTED}"', ports), /unresolved/);
});
