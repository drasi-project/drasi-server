// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, cp, lstat, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { createRequire } from 'node:module';

export function extractInstallRecipes(markdown) {
  const recipes = new Map();
  const pattern = /^```sh\r?\n# @drasi-install: ([a-z-]+)(?: \(Node ([\d.]+), npm ([\d.]+)\))?\r?\n([\s\S]*?)^```[ \t]*$/gm;
  for (const match of markdown.matchAll(pattern)) {
    const [, name, node, npm, code] = match;
    assert(['copied-local', 'tarball'].includes(name), `Unknown install recipe ${name}`);
    assert(!recipes.has(name), `Duplicate install recipe ${name}`);
    assert.deepEqual([node, npm], name === 'copied-local' ? ['24.19.0', '11.17.0'] : [undefined, undefined],
      `Install support scope changed for ${name}; validate it before making a new claim`);
    const lines = code.trim().split(/\r?\n/);
    const variable = name === 'copied-local' ? 'DRASI_PACKAGE_DIR' : 'DRASI_PACKAGE_TARBALL';
    assert.equal(lines.length, 2, `Expected a path assignment and one install command in ${name}`);
    assert(new RegExp(`^${variable}="/absolute/path/[A-Za-z0-9./-]+"$`).test(lines[0]),
      `Unsafe install path placeholder in ${name}`);
    const args = lines[1].split(' ');
    assert.deepEqual(args, [
      'npm', 'install', '--save-exact', '--ignore-scripts',
      ...(name === 'copied-local' ? ['--install-links'] : []),
      `"$${variable}"`, 'react@18.3.1', 'react-dom@18.3.1',
    ], `Unsupported or unsafe literal install command in ${name}`);
    recipes.set(name, { name, variable, node, npm, args: args.slice(1), code });
  }
  assert.deepEqual([...recipes.keys()].sort(), ['copied-local', 'tarball'], 'Both documented supported installs are required');
  return recipes;
}

function execute(executable, args, cwd) {
  const result = spawnSync(executable, args, {
    cwd, encoding: 'utf8', timeout: 300_000, maxBuffer: 8 * 1024 * 1024,
    env: { ...process.env, NODE_PATH: '', NODE_OPTIONS: '', NODE_ENV: 'development' },
  });
  if (result.error) throw result.error;
  return result;
}

function run(executable, args, cwd) {
  const result = execute(executable, args, cwd);
  assert.equal(result.status, 0, `${executable} ${args.join(' ')} failed:\n${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}

async function builtFiles(directory) {
  const files = (await readdir(directory, { recursive: true }))
    .filter(file => file.startsWith('dist/') || file === 'styles.css' || file === 'package.json').sort();
  const values = {};
  for (const file of files) {
    const path = join(directory, file);
    if (!(await lstat(path)).isFile()) continue;
    const bytes = await readFile(path);
    values[file] = { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
  }
  assert(Object.keys(values).length > 0, 'Missing built local package');
  return values;
}

export async function checkInstallRecipes({ artifact, packageRoot, output, readme }) {
  const recipes = extractInstallRecipes(readme);
  const directory = join(output, 'installation');
  await mkdir(directory);
  const localPackage = join(directory, 'package');
  await cp(packageRoot, localPackage, { recursive: true, filter: file => basename(file) !== 'node_modules' });
  // This is a real local development dependency graph, using the author's lock.
  // Sources are absent, so an accidental lifecycle rebuild cannot rescue a pass.
  await copyFile(new URL('../../package-lock.json', import.meta.url), join(localPackage, 'package-lock.json'));
  const original = await builtFiles(localPackage);
  const timestamps = await Promise.all(Object.keys(original).map(async file => (await stat(join(localPackage, file))).mtimeMs));
  const sourceInstallLog = run('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund'], localPackage);
  await writeFile(join(directory, 'package-install.log'), sourceInstallLog + '\n');
  const packageRequire = createRequire(join(localPackage, 'package.json'));
  assert.equal(packageRequire('react/package.json').version, '18.3.1');
  const proof = {
    node: process.version, npm: run('npm', ['--version'], directory),
    react: '18.3.1', builtFiles: original, recipes: {}, unsupportedLinkControl: null,
  };
  const consumers = [];
  try {
    for (const recipe of recipes.values()) {
      const consumer = join(directory, recipe.name);
      await mkdir(consumer);
      consumers.push(consumer);
      await writeFile(join(consumer, 'package.json'), JSON.stringify({
        name: `drasi-install-${recipe.name}`, version: '0.0.0', private: true,
      }, null, 2) + '\n');
      const value = recipe.name === 'copied-local' ? localPackage : artifact;
      const args = recipe.args.map(arg => arg === `"$${recipe.variable}"` ? value : arg);
      if (recipe.node && (process.version !== `v${recipe.node}` || proof.npm !== recipe.npm)) {
        assert.equal(process.version, 'v22.20.0', 'Validate an unmeasured install environment before extending support');
        assert.equal(proof.npm, '10.9.3', 'Do not generalize the measured npm 10.9.3 limitation');
        const rejected = execute('npm', [...args, '--no-audit', '--no-fund'], consumer);
        const diagnostic = `${rejected.stdout}\n${rejected.stderr}`;
        assert.notEqual(rejected.status, 0, 'The documented unsupported npm mode changed; investigate before changing support');
        assert.match(diagnostic, /command sh -c npm run build/);
        assert.match(diagnostic, /No input files/);
        await writeFile(join(consumer, 'unsupported-install.log'), diagnostic);
        proof.recipes[recipe.name] = {
          status: 'unsupported mode: executed expected-negative, not a supported install or skip',
          literal: recipe.code, npmArgs: args, exitStatus: rejected.status,
          reason: 'Measured npm 10.9.3 invokes prepare despite --ignore-scripts; source-free fixture rejects rebuilding.',
        };
        continue;
      }
      const log = run('npm', [...args, '--no-audit', '--no-fund'], consumer);
      await writeFile(join(consumer, 'install.log'), log + '\n');
      const installed = join(consumer, 'node_modules/@drasi/react');
      assert.equal((await lstat(installed)).isSymbolicLink(), false, 'Supported recipe must install a copy, not a symlink');
      assert.deepEqual(await builtFiles(installed), original, 'Installed recipe changed or rebuilt package files');
      const runner = join(consumer, 'render.cjs');
      await copyFile(new URL('install-render.cjs', import.meta.url), runner);
      const renders = [];
      for (const format of ['cjs', 'esm']) {
        renders.push(JSON.parse(run(process.execPath, [runner, format, 'single-react'], consumer)));
      }
      if (recipe.name === 'copied-local') {
        run('npm', ['ci', '--ignore-scripts', '--install-links', '--no-audit', '--no-fund'], consumer);
        assert.deepEqual(await builtFiles(installed), original);
        renders.push(JSON.parse(run(process.execPath, [runner, 'cjs', 'single-react'], consumer)));
      }
      proof.recipes[recipe.name] = { status: 'supported install passed', literal: recipe.code, npmArgs: args, renders, builtFilesIdentical: true };
    }
    const unsupported = join(directory, 'unsupported-default-link');
    await mkdir(unsupported);
    consumers.push(unsupported);
    await writeFile(join(unsupported, 'package.json'), JSON.stringify({
      name: 'drasi-unsupported-file-link', version: '0.0.0', private: true,
      dependencies: { '@drasi/react': 'file:../package', react: '18.3.1', 'react-dom': '18.3.1' },
    }, null, 2) + '\n');
    const linkArgs = ['install', '--ignore-scripts', '--install-links=false', '--no-audit', '--no-fund'];
    if (process.version === 'v22.20.0' && proof.npm === '10.9.3') {
      const rejected = execute('npm', linkArgs, unsupported);
      const diagnostic = `${rejected.stdout}\n${rejected.stderr}`;
      assert.notEqual(rejected.status, 0);
      assert.match(diagnostic, /command sh -c npm run build/);
      assert.match(diagnostic, /No input files/);
      await writeFile(join(unsupported, 'unsupported-install.log'), diagnostic);
      proof.unsupportedLinkControl = {
        expectation: 'unsupported npm 10.9.3 directory installation',
        phase: 'prepare invocation rejected by source-free fixture', exitStatus: rejected.status,
        rendered: false,
      };
    } else {
      run('npm', linkArgs, unsupported);
      assert.equal((await lstat(join(unsupported, 'node_modules/@drasi/react'))).isSymbolicLink(), true);
      const runner = join(unsupported, 'render.cjs');
      await copyFile(new URL('install-render.cjs', import.meta.url), runner);
      proof.unsupportedLinkControl = JSON.parse(run(process.execPath, [runner, 'cjs', 'duplicate-react'], unsupported));
    }
    assert.deepEqual(await builtFiles(localPackage), original);
    assert.deepEqual(await Promise.all(Object.keys(original).map(async file => (await stat(join(localPackage, file))).mtimeMs)),
      timestamps, 'Consumer installation unexpectedly rebuilt the local package');
    await writeFile(join(directory, 'proofs.json'), JSON.stringify(proof, null, 2) + '\n');
    console.log(`Fresh README installs: tarball ESM/CJS single-React passed; copied-local: ${proof.recipes['copied-local'].status}; ${Object.keys(original).length} built files unchanged; executed unsupported control: ${proof.unsupportedLinkControl.expectation}.`);
    return proof;
  } finally {
    for (const consumer of consumers) await rm(join(consumer, 'node_modules'), { recursive: true, force: true });
    await rm(join(localPackage, 'node_modules'), { recursive: true, force: true });
  }
}
