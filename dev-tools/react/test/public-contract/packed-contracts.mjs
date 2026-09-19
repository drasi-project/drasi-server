// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { copyFile, lstat, mkdir, readFile, readdir, realpath, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const fixtures = fileURLToPath(new URL('.', import.meta.url));
const entrypoints = {
  root: '.',
  client: './client',
  react: './react',
  components: './components',
};
const forbiddenPeers = ['react', 'react-dom', '@types/react', '@types/react-dom'];
const componentNames = new Set([
  'QueryTable', 'QueryTableProps', 'ColumnDef', 'RowAction', 'SortConfig',
  'CodeViewerDialog', 'CodeViewerDialogProps', 'CodeIcon', 'ExpandIcon', 'CollapseIcon',
]);

function inside(root, file) {
  const path = relative(root, file);
  return path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

function run(command, args, cwd, timeout = 300_000) {
  const result = spawnSync(command, args, {
    cwd, stdio: 'inherit', timeout,
    env: { ...process.env, NODE_PATH: '', NODE_OPTIONS: '' },
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} ${args.join(' ')} failed (${result.status})`);
}

async function json(file) {
  return JSON.parse(await readFile(file, 'utf8'));
}

async function saveJson(file, value) {
  await writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}

async function filesUnder(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await filesUnder(path));
    else if (entry.isFile()) files.push(path);
    else if (entry.name !== '.bin') {
      // npm's executable links are not module/type-resolution inputs.
      assert.equal(dirname(path).endsWith(`${sep}.bin`), true, `Unexpected installed link: ${path}`);
    }
  }
  return files;
}

function moduleReferences(ts, source) {
  const references = [
    ...source.referencedFiles.map(reference => reference.fileName),
    ...source.typeReferenceDirectives.map(reference => reference.fileName),
  ];
  function visit(node) {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier) {
      references.push(node.moduleSpecifier.text);
    } else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
      references.push(node.argument.literal.text);
    } else if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
      references.push(node.moduleReference.expression.text);
    } else if (ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && ['require', '__require'].includes(node.expression.text)))) {
      assert(node.arguments.length && ts.isStringLiteralLike(node.arguments[0]),
        `Unverifiable dynamic module load in ${source.fileName}`);
      references.push(node.arguments[0].text);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return references;
}

function checkBoundary(ts, source, kind) {
  function visit(node) {
    if (kind !== 'root' && kind !== 'components' && ts.isIdentifier(node)) {
      assert(!componentNames.has(node.text), `${kind} graph contains component ${node.text}: ${source.fileName}`);
    }
    if (kind === 'client' && ts.isIdentifier(node)) {
      assert(!/^(React|ReactDOM|jsxRuntime|jsx_runtime|createPortal)$/.test(node.text),
        `Client graph contains React implementation/types: ${source.fileName}`);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  for (const specifier of moduleReferences(ts, source)) {
    assert(!/\.(?:css|scss|sass)(?:$|\?)/i.test(specifier), `Styles must be explicitly imported: ${source.fileName}`);
    assert(!/(?:^|[/\\])(?:src|examples|tutorials?|trading)(?:[/\\]|$)/i.test(specifier),
      `Published graph imports source/example code: ${specifier}`);
    if (kind === 'client') {
      assert(!/^(?:@types\/)?react(?:-dom)?(?:\/|$)/.test(specifier),
        `Client graph reaches a React peer: ${specifier}`);
    }
    if (kind === 'react' || kind === 'client') {
      assert(!/(?:^|[/\\])components(?:[/\\.]|$)/i.test(specifier),
        `${kind} graph reaches components: ${specifier}`);
      assert(!/^(?:@types\/)?react-dom(?:\/|$)/.test(specifier),
        `${kind} graph reaches ReactDOM: ${specifier}`);
    }
  }
}

async function runtimeGraph(ts, packageRoot, entry, kind) {
  const pending = [entry];
  const visited = new Set();
  const external = new Set();
  while (pending.length) {
    const file = await realpath(pending.pop());
    if (visited.has(file)) continue;
    visited.add(file);
    assert(inside(join(packageRoot, 'dist'), file), `Runtime escaped packed dist: ${file}`);
    assert(/\.(?:cjs|mjs|js)$/.test(file), `Runtime references non-JavaScript source: ${file}`);
    const source = ts.createSourceFile(file, await readFile(file, 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
    checkBoundary(ts, source, kind);
    for (const specifier of moduleReferences(ts, source)) {
      if (specifier.startsWith('.')) {
        pending.push(resolve(dirname(file), specifier));
      } else {
        assert(!isAbsolute(specifier) && !specifier.startsWith('file:'),
          `Runtime contains a machine-local import: ${specifier}`);
        external.add(specifier);
      }
    }
  }
  return {
    files: [...visited].map(file => relative(packageRoot, file)).sort(),
    external: [...external].sort(),
  };
}

async function checkArtifact(ts, packageRoot, manifest) {
  const graph = {};
  for (const [kind, subpath] of Object.entries(entrypoints)) {
    graph[kind] = {};
    for (const [condition, extension] of [['import', '.js'], ['require', '.cjs']]) {
      const entry = manifest.exports[subpath]?.[condition];
      assert(entry && typeof entry === 'object', `Missing conditional ${condition} export for ${subpath}`);
      assert(entry.default?.startsWith('./dist/') && entry.default.endsWith(extension),
        `Expected built ${condition} entry for ${subpath}`);
      assert(entry.types?.startsWith('./dist/') &&
        entry.types.endsWith(condition === 'import' ? '.d.ts' : '.d.cts'),
      `Expected dual-format declarations for ${subpath}`);
      assert(existsSync(join(packageRoot, entry.types)), `Missing published declaration ${entry.types}`);
      graph[kind][condition] = await runtimeGraph(ts, packageRoot, join(packageRoot, entry.default), kind);
    }
  }
  for (const file of await filesUnder(packageRoot)) {
    const declaration = /\.d\.(?:ts|mts|cts)$/.test(file);
    if (!declaration && /\.[cm]?js$/.test(file)) {
      const source = ts.createSourceFile(file, await readFile(file, 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
      checkBoundary(ts, source, 'root');
      for (const specifier of moduleReferences(ts, source)) {
        assert(!isAbsolute(specifier) && !specifier.startsWith('file:'),
          `JavaScript contains a machine-local import: ${specifier}`);
        if (specifier.startsWith('.')) {
          assert(inside(join(packageRoot, 'dist'), resolve(dirname(file), specifier)),
            `JavaScript imports outside packed dist: ${specifier}`);
          assert(/\.[cm]?js$/.test(specifier), `JavaScript imports non-built source: ${specifier}`);
        }
      }
      continue;
    }
    if (!/\.(?:ts|tsx|mts|cts)$/.test(file)) continue;
    assert(declaration, `Package contains implementation source: ${file}`);
    const source = ts.createSourceFile(file, await readFile(file, 'utf8'), ts.ScriptTarget.Latest, true);
    function visit(node) {
      assert.notEqual(node.kind, ts.SyntaxKind.AnyKeyword, `Published declaration contains ambient any: ${file}`);
      ts.forEachChild(node, visit);
    }
    visit(source);
    checkBoundary(ts, source, 'root');
    for (const specifier of moduleReferences(ts, source)) {
      assert(!isAbsolute(specifier) && !specifier.startsWith('file:'),
        `Declaration contains a machine-local import: ${specifier}`);
    }
  }
  return graph;
}

async function checkPeerIsolation(directory, lock) {
  const require = createRequire(join(directory, 'package.json'));
  for (const name of forbiddenPeers) {
    for (const search of require.resolve.paths(name) ?? []) {
      assert(!existsSync(join(search, name)), `Client-only install can reach ${name} in ${search}`);
    }
    assert.throws(() => require.resolve(`${name}/package.json`),
      error => error.code === 'MODULE_NOT_FOUND',
      `Client-only install can resolve ${name}`);
  }
  const installed = await filesUnder(join(directory, 'node_modules'));
  for (const file of installed.filter(path => path.endsWith(`${sep}package.json`))) {
    const manifest = await json(file);
    assert(!forbiddenPeers.includes(manifest.name), `Hidden React package in client-only install: ${file}`);
  }
  assert.equal((await lstat(join(directory, 'node_modules/@drasi/react'))).isSymbolicLink(), false);
  for (const [path, entry] of Object.entries(lock.packages)) {
    if (/node_modules\/(?:react(?:-dom)?|@types\/react(?:-dom)?)$/.test(path)) {
      assert.equal(entry.peer, true, `React is a non-peer dependency in the client-only lock: ${path}`);
    }
  }
  return installed.length;
}

async function createClientOnly(artifact, directory, appLock) {
  await mkdir(directory);
  const manifest = {
    name: 'drasi-packed-client-contract', version: '1.0.0', private: true, type: 'module',
    dependencies: {
      '@drasi/react': `file:${artifact}`,
      typescript: appLock.packages['node_modules/typescript'].version,
    },
  };
  await saveJson(join(directory, 'package.json'), manifest);
  // Seed the complete known lock so npm prunes/reclassifies dependencies rather
  // than selecting new peer or transitive versions from registry ranges.
  await saveJson(join(directory, 'package-lock.json'), {
    ...appLock, name: manifest.name, version: manifest.version,
    packages: { ...appLock.packages, '': manifest },
  });
  run('npm', ['install', '--package-lock-only', '--omit=peer', '--ignore-scripts', '--no-audit', '--no-fund'], directory);
  const lock = await json(join(directory, 'package-lock.json'));
  for (const [path, entry] of Object.entries(lock.packages)) {
    if (!path.startsWith('node_modules/')) continue;
    const original = appLock.packages[path];
    assert(original, `Client-only install selected an unpinned dependency: ${path}`);
    assert.equal(entry.version, original.version, `Client-only install changed locked version: ${path}`);
    assert.equal(entry.integrity, original.integrity, `Client-only install changed locked integrity: ${path}`);
    assert(!entry.link, `Client-only lock contains a source link: ${path}`);
  }
  run('npm', ['ci', '--omit=peer', '--ignore-scripts', '--no-audit', '--no-fund'], directory);
  const installedFiles = await checkPeerIsolation(directory, lock);
  return { omittedPeers: forbiddenPeers, installedFiles };
}

async function compileFixture(directory, output, fixture, mode, manifest) {
  const require = createRequire(join(directory, 'package.json'));
  const ts = require('typescript');
  const packageRoot = join(directory, 'node_modules/@drasi/react');
  const extension = { esm: 'mts', cjs: 'cts', bundler: 'ts' }[mode];
  const file = join(directory, `contract-${fixture}-${mode}.${extension}`);
  // Templates are not .ts files in the repository: only an installed tarball,
  // never package self-resolution or a source path alias, may compile them.
  await copyFile(join(fixtures, `${fixture}.fixture.txt`), file);
  const text = await readFile(file, 'utf8');
  assert(text.includes('@ts-expect-error'), `Missing negative assertions in ${fixture}`);
  const options = {
    noEmit: true, strict: true, skipLibCheck: false, types: [],
    target: ts.ScriptTarget.ES2022,
    lib: ['lib.es2022.d.ts', 'lib.dom.d.ts', 'lib.dom.iterable.d.ts'],
    module: mode === 'bundler' ? ts.ModuleKind.ESNext : ts.ModuleKind.NodeNext,
    moduleResolution: mode === 'bundler' ? ts.ModuleResolutionKind.Bundler : ts.ModuleResolutionKind.NodeNext,
    traceResolution: true,
  };
  const trace = [];
  const host = ts.createCompilerHost(options);
  host.trace = message => trace.push(message);
  const program = ts.createProgram([file], options, host);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  await writeFile(join(output, `${fixture}-${mode}-resolution.log`), `${trace.join('\n')}\n`);
  assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: path => path,
    getCurrentDirectory: () => directory,
    getNewLine: () => '\n',
  }));
  const packageFiles = [];
  const loadedFiles = [];
  for (const source of program.getSourceFiles()) {
    const path = await realpath(source.fileName);
    assert(inside(directory, path), `Compiler escaped packed consumer (source alias or ambient types): ${path}`);
    loadedFiles.push(relative(directory, path));
    if (fixture === 'client') {
      assert(!/[/\\]node_modules[/\\](?:@types[/\\])?react(?:-dom)?(?:[/\\]|$)/.test(path),
        `Client declaration graph resolved React: ${path}`);
    }
    if (inside(packageRoot, path)) {
      assert(/\.d\.(?:ts|mts|cts)$/.test(path), `Compiler reached package implementation source: ${path}`);
      checkBoundary(ts, source, fixture === 'hooks' ? 'react' : fixture);
      packageFiles.push(path);
    }
  }
  const expectedKinds = fixture === 'client' ? ['client'] : fixture === 'hooks' ? ['react', 'client'] : ['root', 'components'];
  const condition = mode === 'cjs' ? 'require' : 'import';
  for (const kind of expectedKinds) {
    const target = resolve(packageRoot, manifest.exports[entrypoints[kind]][condition].types);
    assert(packageFiles.includes(target), `${fixture}/${mode} did not resolve ${kind} through its ${condition} types export`);
  }
  return {
    compiler: ts.version, extension, condition,
    expectedErrors: (text.match(/@ts-expect-error/g) ?? []).length,
    declarations: packageFiles.map(path => relative(packageRoot, path)).sort(),
    loadedFiles: loadedFiles.sort(),
  };
}

/** Called only after Trading has installed the tarball, never a workspace link. */
export async function checkPackedPublicContract({ artifact, destination, app, lock }) {
  const output = join(destination, 'public-contract');
  await mkdir(output);
  const require = createRequire(join(app, 'package.json'));
  const packageRoot = join(app, 'node_modules/@drasi/react');
  const manifest = await json(join(packageRoot, 'package.json'));
  assert.equal(require('react/package.json').version, '18.3.1', 'Trading React baseline changed');
  assert.equal(require('react-dom/package.json').version, '18.3.1', 'Trading ReactDOM baseline changed');
  const proof = {
    node: process.version, react: '18.3.1',
    runtime: await checkArtifact(require('typescript'), packageRoot, manifest),
    types: {},
  };
  const clientOnly = join(output, 'client-only');
  proof.clientIsolation = await createClientOnly(artifact, clientOnly, lock);
  for (const [directory, kinds] of [[clientOnly, ['client']], [app, ['react', 'components', 'root']]]) {
    const runner = join(directory, 'contract-runtime.mjs');
    await copyFile(join(fixtures, 'runtime.mjs'), runner);
    for (const kind of kinds) {
      for (const format of ['esm', 'cjs']) {
        run(process.execPath, [runner, kind, format], directory, 30_000);
      }
    }
    const fixtureNames = directory === clientOnly ? ['client'] : ['hooks', 'components'];
    for (const fixture of fixtureNames) {
      proof.types[fixture] = {};
      for (const mode of ['esm', 'cjs', 'bundler']) {
        proof.types[fixture][mode] = await compileFixture(directory, output, fixture, mode, manifest);
      }
    }
  }
  await saveJson(join(output, 'proofs.json'), proof);
  console.log(`Packed public contracts passed (React 18.3.1; ${process.version}): ${output}`);
}
