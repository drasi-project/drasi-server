// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ALL_QUERIES } from '../src/services/queries.ts';

const [generator, plugins, ...extra] = process.argv.slice(2);
if (!generator || !plugins || extra.length) {
  throw new Error('Usage: node --import tsx scripts/export-native-recipe.ts <native-postgres binary> <plugins>');
}
const app = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const recipe = JSON.parse(readFileSync(join(app, '../server/trading-native-recipe.json'), 'utf8'));
recipe.instances[0].queries = ALL_QUERIES.map(({ id, query, sources, joins }) => ({
  id, query, sources, joins, queryLanguage: 'Cypher', autoStart: true,
}));
const temporary = mkdtempSync(join(tmpdir(), 'trading-native-recipe-'));
try {
  const input = join(temporary, 'recipe.json');
  const output = join(temporary, 'server.yaml');
  writeFileSync(input, JSON.stringify(recipe));
  execFileSync(resolve(generator), ['generate', resolve(plugins), input, output], { stdio: 'inherit' });
  const generated = JSON.parse(execFileSync(resolve(generator), ['inspect', output], { encoding: 'utf8' }));
  mkdirSync(join(app, 'public'), { recursive: true });
  writeFileSync(join(app, 'public/native-computation.json'),
    `${JSON.stringify(generated.instances[0].computation, null, 2)}\n`);
} finally {
  rmSync(temporary, { recursive: true });
}
