// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { readFile, realpath, rm } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { exampleRoot } from './runtime.mjs';

const argument = process.argv[2];
assert(argument, 'Usage: npm run clean:run -- <exact completed live-run directory>');
const root = await realpath(resolve(exampleRoot, '.runtime'));
const directory = await realpath(resolve(argument));
assert.equal(dirname(directory), root, 'Only a direct child of this example .runtime directory may be removed');
assert(/^live-[a-zA-Z0-9]+$/.test(basename(directory)), 'Expected an exact generated live-run directory');
const cleanup = JSON.parse(await readFile(resolve(directory, 'cleanup.json'), 'utf8'));
assert(cleanup.owner === 'drasi-react-cold-storage' && cleanup.stopped === true, 'Refusing an unowned or unfinished run');
await rm(directory, { recursive: true });
console.log(`Removed only the completed example run: ${directory}`);
