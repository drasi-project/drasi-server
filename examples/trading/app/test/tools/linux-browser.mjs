// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cp, mkdir, mkdtemp, readdir } from 'node:fs/promises';
import { join, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const image = 'mcr.microsoft.com/playwright:v1.56.1-noble@sha256:f1e7e01021efd65dd1a2c56064be399f3e4de00fd021ac561325f2bfbb2b837a';
const root = fileURLToPath(new URL('../../../../../', import.meta.url));
const app = join(root, 'examples/trading/app');
const script = 'examples/trading/app/test/tools/linux-browser.mjs';
const args = process.argv.slice(2);

function run(command, parameters, cwd) {
  const result = spawnSync(command, parameters, { cwd, stdio: 'inherit' });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} ${parameters.join(' ')} failed (${result.status})`);
}

if (args[0] !== '--inside') {
  await mkdir(join(app, '.test-runtime'), { recursive: true });
  const artifacts = await mkdtemp(join(app, '.test-runtime/linux-browser-'));
  console.log(`Linux browser artifacts: ${artifacts}`);
  run('docker', [
    'run', '--rm', '--init', '--platform', 'linux/amd64', '--shm-size=1g',
    '--mount', `type=bind,source=${root},target=/repo,readonly`,
    '--mount', `type=bind,source=${artifacts},target=/artifacts`,
    '--env', 'CI=true', image, 'node', `/repo/${script}`, '--inside', ...args,
  ]);
  if (args.includes('--update-snapshots')) {
    await cp(join(artifacts, '__screenshots__'), join(app, 'test/browser/__screenshots__'), { recursive: true });
    console.log('Updated Linux/amd64 visual baselines; review the image diff before committing.');
  }
} else {
  const packageDir = '/work/dev-tools/react';
  const consumer = '/work/consumer';
  const consumerApp = `${consumer}/examples/trading/app`;
  await mkdir(packageDir, { recursive: true });
  await cp('/repo/dev-tools/react', packageDir, {
    recursive: true,
    filter: path => !['node_modules', 'dist', 'coverage'].includes(basename(path)),
  });
  try {
    run('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund'], packageDir);
    run('npm', ['run', 'typecheck'], packageDir);
    run('npm', ['run', 'test:coverage'], packageDir);
    run('npm', ['run', 'build'], packageDir);
    run('npm', ['pack', '--pack-destination', '/artifacts'], packageDir);
    const archives = (await readdir('/artifacts')).filter(path => path.endsWith('.tgz'));
    assert.equal(archives.length, 1);
    run('node', [
      '/repo/examples/trading/app/test/tools/packed-consumer.mjs',
      `/artifacts/${archives[0]}`, consumer,
    ]);
    run('npm', ['run', 'typecheck'], consumerApp);
    run('npm', ['run', 'test:coverage'], consumerApp);
    run('npm', ['run', '--ignore-scripts', 'build'], consumerApp);
    run('npm', ['run', 'test:browser', '--', ...args.slice(1)], consumerApp);
    run('node', [
      '/repo/examples/trading/app/test/tools/check-baseline.mjs',
      `/artifacts/${archives[0]}`, consumerApp,
      join(packageDir, 'coverage/coverage-summary.json'),
    ]);
  } finally {
    const outputs = [
      [join(packageDir, 'coverage'), '/artifacts/package-coverage'],
      [join(consumerApp, 'coverage'), '/artifacts/app-coverage'],
      [join(consumerApp, 'dist'), '/artifacts/app-dist'],
      [join(consumerApp, 'test-results'), '/artifacts/test-results'],
      [join(consumerApp, 'playwright-report'), '/artifacts/playwright-report'],
      [join(consumerApp, 'test/browser/__screenshots__'), '/artifacts/__screenshots__'],
    ];
    for (const [source, target] of outputs) {
      try {
        await cp(source, target, { recursive: true });
      } catch (error) {
        if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
        console.log(`Not produced: ${source}`);
      }
    }
  }
}
