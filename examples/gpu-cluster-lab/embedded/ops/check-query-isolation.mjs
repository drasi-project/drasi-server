import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { queryIds, rowKey, validateRow } from '../../shared/ui/src/rows.ts';

const [image, recording, destination, option, ...extra] = process.argv.slice(2);
assert.ok(image && recording && destination,
  'Usage: node --experimental-strip-types ops/check-query-isolation.mjs <image-id> <recording.ndjson> <report-directory> [--paced]');
assert.ok(extra.length === 0 && (option === undefined || option === '--paced'), 'Unknown diagnostic option');
const paced = option === '--paced';
assert.match(image, /^sha256:[a-f0-9]{64}$/, 'Pin the diagnostic image by immutable ID');
assert.equal(execFileSync('docker', ['image', 'inspect', image, '--format', '{{.Id}}'],
  { encoding: 'utf8' }).trim(), image, 'Use the runnable image ID returned by docker image inspect');
const corpus = resolve(recording), directory = resolve(destination);
await mkdir(directory, { recursive: true });
const manifest = { image, corpus, corpus_sha256: createHash('sha256').update(await readFile(corpus)).digest('hex'),
  paced, started_at: new Date().toISOString(), runs: [] };
await writeFile(`${directory}/manifest.json`, JSON.stringify(manifest, null, 2));
const queries = paced ? ['all'] : ['ui-workloads', 'ui-placements', 'scheduling-inputs', 'simulation-inputs',
  ...queryIds.filter(id => !['ui-workloads', 'ui-placements'].includes(id)), 'plan-output', 'runtime-context',
  'input-clusters', 'input-policies', 'input-data', 'input-gpus', 'input-settings', 'input-workloads', 'input-plan', 'all'];
const results = new Map();
const failures = [];
const canonical = value => Array.isArray(value)
  ? value.map(canonical).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)))
  : value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])])) : value;
const undoClockShift = (value, shift) => Array.isArray(value) ? value.map(item => undoClockShift(item, shift))
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key, item]) => [
    key, ['time_ms', 'report_time_ms', 'acknowledged_at_ms'].includes(key) && typeof item === 'number'
      ? item - shift
      : key === 'payload' && typeof item === 'string'
        ? JSON.stringify(undoClockShift(JSON.parse(item), shift)) : undoClockShift(item, shift),
  ])) : value;
const comparableSnapshots = report => canonical(undoClockShift(report.snapshots, report.clock_shift_ms ?? 0));
for (const provider of ['bare', 'pipeline-memory']) {
  for (const query of queries) {
    const filename = `${provider}-${query}.json`;
    const started = performance.now();
    const child = spawn('docker', ['run', '--rm', '--read-only', '--network', 'none',
      '--user', `${process.getuid()}:${process.getgid()}`,
      '-v', `${corpus}:/corpus/recording.ndjson:ro`, '-v', `${directory}:/results`,
      image, '/corpus/recording.ndjson', provider, query, `/results/${filename}`, ...(paced ? ['--paced'] : [])],
    { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; });
    child.stderr.on('data', chunk => { stderr += chunk; });
    const code = await new Promise((resolveExit, reject) => {
      child.on('error', reject);
      child.on('exit', resolveExit);
    });
    await writeFile(`${directory}/${provider}-${query}.log`, `${stdout}\n${stderr}`);
    const run = { provider, query, exit_code: code, container_wall_ms: performance.now() - started };
    manifest.runs.push(run);
    await writeFile(`${directory}/manifest.json`, JSON.stringify(manifest, null, 2));
    let report;
    try {
      report = JSON.parse(await readFile(`${directory}/${filename}`, 'utf8'));
    } catch (error) {
      failures.push(`${provider}/${query}: no valid report: ${error.message}; ${stderr}`);
      console.error(failures.at(-1));
      continue;
    }
    if (code !== 0) failures.push(`${provider}/${query}: exit ${code}; ${stderr}`);
    for (const snapshot of report.snapshots) {
      if (!queryIds.includes(snapshot.query)) continue;
      const keys = new Set();
      try {
        for (const row of snapshot.rows) {
          validateRow(snapshot.query, row);
          const key = rowKey(snapshot.query, row);
          assert.ok(!keys.has(key), `duplicate identity ${key}`);
          keys.add(key);
        }
      } catch (error) {
        failures.push(`${provider}/${query}/${snapshot.stage}: ${error.message}`);
      }
    }
    results.set(`${provider}/${query}`, report);
    for (const measured of report.queries) {
      console.log(JSON.stringify({
        provider, query: measured.query, combined: query === 'all',
        input_frames: measured.input.calls,
        transform_p50_ms: Number(measured.input.p50_ns) / 1e6,
        transform_p95_ms: Number(measured.input.p95_ns) / 1e6,
        transform_max_ms: Number(measured.input.max_ns) / 1e6,
        process_cpu_ms: Number(measured.process_cpu_ticks_during_calls) * 1000 / report.cpu_clock_ticks_per_second,
        correctness_failures: report.correctness_failures,
      }));
    }
  }
}
for (const query of queries) {
  const bare = results.get(`bare/${query}`), wrapped = results.get(`pipeline-memory/${query}`);
  if (!bare || !wrapped) continue;
  try {
    assert.deepEqual(comparableSnapshots(bare), comparableSnapshots(wrapped),
      `${query}: provider modes produced different rows`);
  } catch (error) { failures.push(error.message); }
}
for (const provider of ['bare', 'pipeline-memory']) {
  const combined = results.get(`${provider}/all`);
  if (!combined) continue;
  for (const query of queries.filter(query => query !== 'all')) {
    const single = results.get(`${provider}/${query}`);
    if (!single) continue;
    try {
      assert.deepEqual(canonical(single.snapshots), canonical(combined.snapshots.filter(s => s.query === query)),
        `${provider}/${query}: isolated and combined rows differ`);
    } catch (error) { failures.push(error.message); }
  }
}
await writeFile(`${directory}/validation.json`, JSON.stringify({ failures, complete: failures.length === 0 }, null, 2));
assert.deepEqual(failures, [], 'Isolation correctness/shape/provider comparisons must pass; timing alone is not success');
