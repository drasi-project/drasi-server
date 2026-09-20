// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cp, lstat, mkdir, readFile } from 'node:fs/promises';
import { resolve, basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkPackedPublicContract } from '../../../../../dev-tools/react/test/public-contract/packed-contracts.mjs';

const [artifactArgument, destinationArgument] = process.argv.slice(2);
assert(artifactArgument && destinationArgument, 'Usage: node test/tools/packed-consumer.mjs <package.tgz> <new-empty-directory>');
const artifact = resolve(artifactArgument);
const destination = resolve(destinationArgument);
const trading = fileURLToPath(new URL('../../../', import.meta.url));
const app = join(destination, 'examples/trading/app');
const examples = fileURLToPath(new URL('../../../../react/', import.meta.url));
const exampleApp = join(destination, 'examples/react');

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8', stdio: 'inherit' });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} ${args.join(' ')} failed (${result.status})`);
}

// Refuse reuse: stale source/node_modules must not make this consumer pass.
await mkdir(destination);
const excluded = new Set(['node_modules', 'dist', 'coverage', '.test-runtime', '.runtime', 'test-results', 'playwright-report', 'logs']);
await cp(trading, join(destination, 'examples/trading'), {
  recursive: true,
  filter: path => !excluded.has(basename(path)),
});
await cp(examples, exampleApp, {
  recursive: true,
  filter: path => !excluded.has(basename(path)),
});
const listing = spawnSync('tar', ['-tzf', artifact], { encoding: 'utf8' });
if (listing.error) throw listing.error;
assert.equal(listing.status, 0, listing.stderr);
const entries = listing.stdout.trim().split('\n');
assert(entries.every(entry => entry.startsWith('package/') && !entry.split('/').includes('..')), 'Unsafe package archive path');
assert(!entries.some(entry => entry.startsWith('package/src/')), 'Package unexpectedly contains source');
// Resolve only the artifact in this disposable consumer before npm ci. Some npm
// versions run file-directory prepare even with --ignore-scripts; no source
// directory exists here, so accidental rebuilding cannot rescue the test.
async function installArtifact(consumer) {
  const originalLock = JSON.parse(await readFile(join(consumer, 'package-lock.json'), 'utf8'));
  run('npm', ['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund', artifact], consumer);
  const artifactLock = JSON.parse(await readFile(join(consumer, 'package-lock.json'), 'utf8'));
  for (const [path, entry] of Object.entries(originalLock.packages)) {
    if (path.startsWith('node_modules/') && path !== 'node_modules/@drasi/react' && !entry.link) {
      assert.equal(artifactLock.packages[path]?.version, entry.version, `Tarball substitution changed locked dependency ${path}`);
      assert.equal(artifactLock.packages[path]?.integrity, entry.integrity, `Tarball substitution changed integrity for ${path}`);
    }
  }
  run('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund'], consumer);
  assert.equal((await lstat(join(consumer, 'node_modules/@drasi/react'))).isSymbolicLink(), false, 'Consumer still uses a local link');
  const manifest = JSON.parse(await readFile(join(consumer, 'node_modules/@drasi/react/package.json'), 'utf8'));
  assert.equal(manifest.private, true);
  return artifactLock;
}
const artifactLock = await installArtifact(app);
await checkPackedPublicContract({ artifact, destination, app, lock: artifactLock });
await installArtifact(exampleApp);
run('npm', ['run', 'typecheck'], exampleApp);
run('npm', ['test'], exampleApp);
run('npm', ['run', '--ignore-scripts', 'build'], exampleApp);
console.log(`Clean tarball consumer ready: ${app}`);
console.log(`Independent example consumer ready: ${exampleApp}`);
