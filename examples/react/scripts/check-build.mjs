// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join, resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { assertExampleModule } from './module-identity.mjs';

export function entryGraph(graph, entry) {
  const start = Object.keys(graph).find(file => graph[file].entry === entry);
  assert(start, `Missing built entry ${entry}`);
  const files = new Set();
  function visit(file) {
    if (files.has(file)) return;
    assert(graph[file], `Missing emitted chunk ${file}`);
    files.add(file);
    for (const imported of [...graph[file].imports, ...graph[file].dynamicImports]) visit(imported);
  }
  visit(start);
  return [...files].sort();
}

export function assertHooksGraph(graph) {
  const files = entryGraph(graph, 'hooks.html');
  for (const file of files) {
    const chunk = graph[file];
    assert.deepEqual(chunk.css, [], `Hooks entry unexpectedly loads CSS: ${file}`);
    for (const module of chunk.modules) {
      assert(!/(?:^|\/)trading(?:\/|$)|@radix-ui|scroll-into-view|react-remove-scroll|react-focus|ModalLayer|components\/|\.(?:css|scss|sass)(?:\?|$)/i.test(module.id),
        `Hooks entry contains presentation/domain module ${module.id}`);
      assertExampleModule(module.id);
      assert(!module.sources.some(source => /\/components\//.test(source)),
        `Hooks entry retains component implementation through ${module.id}`);
      assert(!/dev-tools\/react\/src/.test(module.id), 'Package source alias detected');
    }
  }
  return files;
}

export function assertExampleBudget(evidence, baseline) {
  assert.deepEqual(Object.keys(evidence.entries).sort(), Object.keys(baseline.entries).sort(), 'Example entry set changed');
  const scopes = { total: baseline.total, ...baseline.entries };
  for (const [scope, expected] of Object.entries(scopes)) {
    const actual = scope === 'total' ? evidence.total : evidence.entries[scope];
    for (const metric of ['js', 'jsGzip', 'css', 'cssGzip']) {
      assert(Number.isInteger(actual[metric]) && actual[metric] >= 0 && Number.isInteger(expected[metric]),
        `Missing example ${scope} ${metric} measurement`);
      assert(actual[metric] <= expected[metric] * 1.02,
        `Example ${scope} ${metric} exceeds the 2% growth budget; explain and review the measured feature cost`);
    }
  }
}

export async function checkBuild(directory) {
  const graph = JSON.parse(await readFile(join(directory, 'build-graph.json'), 'utf8'));
  for (const chunk of Object.values(graph)) for (const module of chunk.modules) {
    assertExampleModule(module.id);
    assert(!module.sources.some(source => /(?:^|\/)examples\/trading\//.test(source)),
      `Package chunk retains Trading source: ${module.id}`);
  }
  const hooks = assertHooksGraph(graph);
  const html = await readFile(join(directory, 'hooks.html'), 'utf8');
  assert(!/stylesheet|\.css\b/.test(html), 'Hooks HTML unexpectedly loads a stylesheet');
  const allFiles = (await readdir(join(directory, 'assets'))).filter(file => /\.(js|css)$/.test(file)).sort();
  const assets = {};
  for (const name of allFiles) {
    const content = await readFile(join(directory, 'assets', name));
    assets[`assets/${name}`] = {
      bytes: content.length, gzip: gzipSync(content).length,
      sha256: createHash('sha256').update(content).digest('hex'),
    };
  }
  assert.deepEqual(Object.keys(graph).sort(), Object.keys(assets).filter(file => file.endsWith('.js')).sort(),
    'Every emitted entry/shared/lazy JS chunk must be accounted for');
  const entries = {};
  for (const entry of ['index.html', 'hooks.html', 'showcase.html']) {
    const js = entryGraph(graph, entry);
    const css = [...new Set(js.flatMap(file => graph[file].css))].sort();
    entries[entry] = { jsFiles: js, cssFiles: css, ...totals([...js, ...css], assets) };
  }
  const evidence = {
    node: process.version, assets, entries, total: totals(Object.keys(assets), assets),
    hooksPresentationFree: true, hooksChunks: hooks,
    note: 'Per-entry totals include all reachable shared and lazy chunks; workspace totals count each emitted asset once.',
  };
  await writeFile(join(directory, 'build-evidence.json'), JSON.stringify(evidence, null, 2) + '\n');
  console.log(JSON.stringify(evidence, null, 2));
  const baseline = JSON.parse(await readFile(new URL('../test/baseline-metrics.json', import.meta.url), 'utf8'));
  assertExampleBudget(evidence, baseline);
  return evidence;
}

function totals(files, assets) {
  return files.reduce((sum, file) => {
    assert(assets[file], `Missing measured asset ${file}`);
    const kind = file.endsWith('.js') ? 'js' : 'css';
    sum[kind] += assets[file].bytes;
    sum[`${kind}Gzip`] += assets[file].gzip;
    return sum;
  }, { js: 0, jsGzip: 0, css: 0, cssGzip: 0 });
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  await checkBuild(fileURLToPath(new URL('../dist/', import.meta.url)));
}
