// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, copyFile, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { STOCKS, POSITIONS, ORDERS, FIXED_TIME } from '../fixtures/synthetic/trading.ts';
import { verifyRuntimeVersion } from './runtimeVersion.ts';
import { startSseProxy } from './sseProxy.ts';

const app = fileURLToPath(new URL('../../', import.meta.url));
const trading = fileURLToPath(new URL('../../../', import.meta.url));
const python = process.env.P1_PYTHON ?? 'python3';
const sourceRoot = resolve(process.env.P1_SOURCE_ROOT ?? fileURLToPath(new URL('../../../../../', import.meta.url)));
assert([undefined, 'checkout', 'image'].includes(process.env.P1_RUNTIME), 'P1_RUNTIME must be checkout or image');
const native = process.env.P1_RUNTIME !== 'image';
const checkoutBinary = native
  ? resolve(process.env.P1_SERVER_BIN ?? fileURLToPath(new URL('../../../../../target/debug/drasi-server', import.meta.url)))
  : undefined;
if (native) {
  assert(process.env.P1_SERVER_CARGO_LOCK, 'P1_SERVER_CARGO_LOCK is required to record the exact native dependency lock');
  assert(/^[a-f0-9]{40}$/.test(process.env.P1_SERVER_REVISION ?? ''), 'P1_SERVER_REVISION must be the exact compiled checkout commit');
}
const platform = process.env.P1_DOCKER_PLATFORM ?? (process.arch === 'arm64' ? 'linux/arm64' : 'linux/amd64');
assert(['linux/arm64', 'linux/amd64'].includes(platform), 'BLOCKED: supported runtime platforms are linux/arm64 and linux/amd64');
assert(process.getuid && process.getgid, 'BLOCKED: the live harness requires a POSIX Docker host');
const user = `${process.getuid()}:${process.getgid()}`;
const architecture = platform.split('/')[1];
const lockPath = process.env.P1_PLUGIN_LOCK
  ? resolve(process.env.P1_PLUGIN_LOCK)
  : fileURLToPath(new URL(native && process.platform === 'darwin'
    ? `plugins-darwin-${process.arch}.lock` : `plugins-${architecture}.lock`, import.meta.url));
const pins = JSON.parse(await readFile(new URL('runtime-pins.json', import.meta.url), 'utf8')) as {
  serverVersion: string; serverImage: string; serverCommit: string; postgresImage: string;
};
assert(/^ghcr\.io\/drasi-project\/drasi-server@sha256:[a-f0-9]{64}$/.test(pins.serverImage));
assert(/^postgres:14-alpine@sha256:[a-f0-9]{64}$/.test(pins.postgresImage));

function command(executable: string, args: string[], options: { env?: NodeJS.ProcessEnv; timeout?: number } = {}): string {
  const result = spawnSync(executable, args, {
    encoding: 'utf8', timeout: 120_000, maxBuffer: 16 * 1024 * 1024, ...options,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${executable} ${args.join(' ')} failed:\n${result.stderr}\n${result.stdout}`);
  return result.stdout.trim();
}

await access(join(app, 'dist/index.html'));
if (checkoutBinary) await access(checkoutBinary);
command('docker', ['info', '--format', '{{.ServerVersion}}']);
command(python, ['-c', [
  'import sys, tomllib',
  'from importlib.metadata import version',
  'requirements = [line.strip().split("==") for line in open(sys.argv[1]) if "==" in line]',
  'assert all(version(name) == expected for name, expected in requirements), "Use the pinned isolated test/live/requirements.txt environment"',
].join('\n'), join(app, 'test/live/requirements.txt')]);
const sourceProvenance = native ? JSON.parse(command(python, [
  join(app, 'test/live/source_provenance.py'),
  '--source-root', sourceRoot,
  '--cargo-lock', process.env.P1_SERVER_CARGO_LOCK!,
  '--revision', process.env.P1_SERVER_REVISION!,
  '--plugin-lock', lockPath,
])) : undefined;
const pluginPins: Record<string, { filename: string; file_hash: string; version: string }> =
  sourceProvenance?.pluginPins ?? JSON.parse(command(python, [
  '-c', 'import json, sys, tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], "rb"))["plugins"]))', lockPath,
]));
assert.equal(Object.keys(pluginPins).length, 5);
const checkoutDependencies = native ? JSON.parse(command(python, [
  '-c',
  'import json, sys, tomllib; lock=tomllib.load(open(sys.argv[1], "rb")); print(json.dumps({p["name"]:p["version"] for p in lock["package"] if p["name"] in ["drasi-server","drasi-lib","drasi-core","drasi-query-ast","drasi-query-cypher","drasi-query-gql","drasi-index-rocksdb","drasi-plugin-sdk","drasi-host-sdk","drasi-ffi-primitives"]}))',
  process.env.P1_SERVER_CARGO_LOCK!,
])) : undefined;
const checkoutVersion = checkoutBinary ? command(checkoutBinary, ['--version']) : undefined;
const runtimeVersion = native ? verifyRuntimeVersion(checkoutVersion!, {
  server: checkoutDependencies['drasi-server'],
  sdk: sourceProvenance.selectedDependencies['drasi-plugin-sdk'].version,
}) : pins.serverVersion;
await mkdir(join(app, '.test-runtime'), { recursive: true });
const runDir = await mkdtemp(join(app, '.test-runtime/live-'));
await mkdir(join(runDir, 'plugins'));
await mkdir(join(runDir, 'tmp'));
await copyFile(lockPath, join(runDir, 'plugins/plugins.lock'));
if (sourceProvenance) {
  await writeFile(join(runDir, 'source-provenance.json'), JSON.stringify(sourceProvenance, null, 2) + '\n');
}
await writeFile(join(runDir, 'runtime.json'), JSON.stringify({
  imageBaseline: pins, platform: native ? `${process.platform}/${process.arch}` : platform, pluginPins,
  mode: native ? 'checkout' : 'pinned-image',
  ...(checkoutBinary ? {
    checkoutRevision: process.env.P1_SERVER_REVISION,
    checkoutVersion,
    checkoutDependencies,
    sourceProvenance,
    checkoutCargoLockSha256: createHash('sha256').update(await readFile(process.env.P1_SERVER_CARGO_LOCK!)).digest('hex'),
    checkoutBinarySha256: createHash('sha256').update(await readFile(checkoutBinary)).digest('hex'),
  } : {}),
}, null, 2) + '\n');
console.log(`Real-server diagnostics: ${runDir}`);

const children: ChildProcess[] = [];
const criticalChildren = new Set<ChildProcess>();
const logs: ReturnType<typeof createWriteStream>[] = [];
const containers: Array<{ id: string; name: string }> = [];
let network: string | undefined;
let proxy: Awaited<ReturnType<typeof startSseProxy>> | undefined;
let interrupted = false;
let failure: unknown;
const cleanupErrors: unknown[] = [];
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => {
    interrupted = true;
    for (const child of children) child.kill('SIGINT');
  });
}

function container(name: string, args: string[], env?: NodeJS.ProcessEnv): string {
  const id = command('docker', [
    'run', '--detach', '--name', `drasi-p1-${name}-${randomUUID()}`, ...args,
  ], { env });
  assert(/^[a-f0-9]{64}$/.test(id), 'Docker did not return an owned container ID');
  containers.push({ id, name });
  return id;
}

function endpoint(id: string, port: number): string {
  const mapping = command('docker', ['port', id, `${port}/tcp`]);
  assert(/^127\.0\.0\.1:\d+$/.test(mapping), `Unexpected published binding ${mapping}`);
  return `http://${mapping}`;
}

async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  assert(address && typeof address !== 'string');
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return address.port;
}

function start(executable: string, args: string[], name: string, env: NodeJS.ProcessEnv, cwd = runDir, critical = true): ChildProcess {
  const output = createWriteStream(join(runDir, `${name}.log`));
  logs.push(output);
  const child = spawn(executable, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout?.pipe(output, { end: false });
  child.stderr?.pipe(output, { end: false });
  child.on('error', error => output.write(`${error.stack}\n`));
  children.push(child);
  if (critical) criticalChildren.add(child);
  return child;
}

async function ready(name: string, predicate: () => Promise<boolean>, milliseconds = 60_000): Promise<void> {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) {
    assert(!interrupted, 'Smoke interrupted');
    for (const child of criticalChildren) {
      assert(child.exitCode === null && child.signalCode === null, `${name}: owned service ${child.pid} exited; see ${runDir}`);
    }
    if (await predicate()) return;
    await delay(200);
  }
  throw new Error(`BLOCKED: ${name} not ready after ${milliseconds} ms; see ${runDir}`);
}

async function healthy(url: string): Promise<boolean> {
  try {
    return (await fetch(url, { signal: AbortSignal.timeout(1500) })).ok;
  } catch (error) {
    if (error instanceof DOMException && error.name === 'TimeoutError') return false;
    if (error instanceof TypeError && error.cause && typeof error.cause === 'object' &&
        'code' in error.cause && ['ECONNREFUSED', 'ECONNRESET', 'UND_ERR_SOCKET'].includes(String(error.cause.code))) return false;
    throw error;
  }
}

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGINT');
  const closed = new Promise<void>(resolve => child.once('exit', () => resolve()));
  await Promise.race([closed, delay(5000, undefined, { ref: false })]);
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGKILL');
    await closed;
  }
}

try {
  const adminPassword = randomUUID();
  const appPassword = randomUUID();
  const env = {
    ...process.env, PYTHONDONTWRITEBYTECODE: '1',
    TMPDIR: join(runDir, 'tmp'), P1_POSTGRES_APP_PASSWORD: appPassword,
  };
  const networkName = `drasi-p1-${randomUUID()}`;
  network = command('docker', ['network', 'create', networkName]);
  assert(/^[a-f0-9]{64}$/.test(network));
  const postgres = container('postgres', [
    '--network', network, '--network-alias', 'postgres',
    '--env', 'POSTGRES_PASSWORD', '--env', 'POSTGRES_DB=trading_demo',
    '--publish', '127.0.0.1::5432',
    '--mount', `type=bind,source=${join(trading, 'database/init.sql')},target=/docker-entrypoint-initdb.d/init.sql,readonly`,
    pins.postgresImage, 'postgres', '-c', 'wal_level=logical',
    '-c', 'max_replication_slots=10', '-c', 'max_wal_senders=10',
  ], { ...env, POSTGRES_PASSWORD: adminPassword });
  const postgresPort = new URL(endpoint(postgres, 5432)).port;
  await ready('PostgreSQL schema', async () => {
    const result = spawnSync('docker', [
      'exec', postgres, 'psql', '-U', 'postgres', '-d', 'trading_demo',
      '-Atc', "SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('stocks','portfolio','watchlist','limit_orders') AND current_setting('listen_addresses') <> ''",
    ], { encoding: 'utf8', timeout: 5000 });
    if (result.error) throw result.error;
    return result.status === 0 && result.stdout.trim() === '4';
  });
  const seedFile = join(runDir, 'seed.json');
  await writeFile(seedFile, JSON.stringify({ stocks: STOCKS, positions: POSITIONS, orders: ORDERS, watchlist: ['AAPL', 'MSFT'] }));
  command(python, [join(app, 'test/live/seed.py'), seedFile], { env: {
    ...env, P1_POSTGRES_PORT: postgresPort, P1_POSTGRES_PASSWORD: adminPassword,
  } });
  await writeFile(join(runDir, 'prices.jsonl'), [
    { kind: 'Header', start_time: FIXED_TIME, description: 'P1 deterministic Trading prices' },
    ...STOCKS.map(stock => ({
      kind: 'Node', id: `price_${stock.symbol}`, labels: ['stock_prices'],
      properties: { symbol: stock.symbol, price: stock.price, previous_close: stock.previousClose, volume: stock.volume, timestamp: FIXED_TIME },
    })),
  ].map(row => JSON.stringify(row)).join('\n') + '\n');
  const restPort = native ? await freePort() : 8080;
  const pricePort = native ? await freePort() : 9100;
  const reactionPort = native ? await freePort() : 8281;
  const postgresConfig = {
    kind: 'postgres', autoStart: true, host: native ? '127.0.0.1' : 'postgres', port: native ? Number(postgresPort) : 5432,
    database: 'trading_demo', user: 'drasi_user', password: '${P1_POSTGRES_APP_PASSWORD}',
    sslMode: 'prefer', bootstrapProvider: { kind: 'postgres' },
  };
  const config = {
    apiVersion: 'drasi.io/v1', id: 'trading-server', host: native ? '127.0.0.1' : '0.0.0.0', port: restPort,
    logLevel: 'info', persistConfig: false, autoInstallPlugins: false, enableUi: false,
    verifyPlugins: true, pluginRegistry: 'ghcr.io/drasi-project',
    plugins: Object.keys(pluginPins).map(ref => ({ ref })),
    sources: [
      { kind: 'http', id: 'price-feed', autoStart: true, host: native ? '127.0.0.1' : '0.0.0.0', port: pricePort, timeoutMs: 10000,
        bootstrapProvider: { kind: 'scriptfile', filePaths: [native ? join(runDir, 'prices.jsonl') : '/smoke/prices.jsonl'] } },
      { ...postgresConfig, id: 'postgres-stocks', tables: ['stocks', 'portfolio', 'watchlist'],
        slotName: 'drasi_trading_slot', publicationName: 'drasi_trading_pub',
        tableKeys: ['stocks', 'portfolio', 'watchlist'].map(table => ({ table, keyColumns: ['id'] })) },
      { ...postgresConfig, id: 'postgres-broker', tables: ['limit_orders'],
        slotName: 'drasi_broker_slot', publicationName: 'drasi_broker_pub',
        tableKeys: [{ table: 'limit_orders', keyColumns: ['id'] }] },
    ],
    queries: [], reactions: [],
  };
  await writeFile(join(runDir, 'server.yaml'), JSON.stringify(config, null, 2) + '\n');
  const dockerOptions = [
    '--platform', platform, '--user', user, '--workdir', '/smoke',
    '--env', 'TMPDIR=/smoke/tmp', '--env', 'P1_POSTGRES_APP_PASSWORD',
    '--mount', `type=bind,source=${runDir},target=/smoke`,
  ];
  if (checkoutBinary) {
    const installer = start(python, [
      join(sourceRoot, 'scripts/install_plugins.py'),
      '--group', 'trading', '--server-bin', checkoutBinary,
      '--plugins-dir', join(runDir, 'plugins'),
    ], 'plugin-install', env, runDir, false);
    await ready('signed, locked native plugin installation', async () => {
      if (installer.exitCode === null && installer.signalCode === null) return false;
      assert.equal(installer.exitCode, 0, 'Pinned native plugin installation failed; see plugin-install.log');
      return true;
    }, 300_000);
  } else {
    const installer = container('plugin-install', [
      ...dockerOptions, pins.serverImage, '--config', '/smoke/server.yaml',
      '--plugins-dir', '/smoke/plugins', 'plugin', 'install', '--from-config', '--locked',
    ], env);
    await ready('signed, locked plugin installation', async () => {
      const state = JSON.parse(command('docker', ['inspect', '--format', '{{json .State}}', installer])) as { Running: boolean; ExitCode: number };
      if (state.Running) return false;
      assert.equal(state.ExitCode, 0, 'Pinned plugin installation failed; see plugin-install.log');
      return true;
    }, 300_000);
  }
  for (const pin of Object.values(pluginPins)) {
    assert(basename(pin.filename) === pin.filename && /^[a-f0-9]{64}$/.test(pin.file_hash));
    assert.equal(createHash('sha256').update(await readFile(join(runDir, 'plugins', pin.filename))).digest('hex'),
      pin.file_hash, `Pinned binary hash mismatch: ${pin.filename}`);
  }
  let server: string | undefined;
  if (checkoutBinary) {
    start(checkoutBinary, [
      '--config', join(runDir, 'server.yaml'), '--plugins-dir', join(runDir, 'plugins'),
    ], 'drasi-server', env);
  } else {
    server = container('drasi-server', [
      ...dockerOptions, '--network', network, '--stop-signal', 'SIGINT',
      '--publish', '127.0.0.1::8080', '--publish', '127.0.0.1::8281', '--publish', '127.0.0.1::9100',
      pins.serverImage, '--config', '/smoke/server.yaml', '--plugins-dir', '/smoke/plugins',
    ], env);
  }
  const rest = server ? endpoint(server, 8080) : `http://127.0.0.1:${restPort}`;
  await ready('Drasi health', async () => {
    if (server) assert.equal(command('docker', ['inspect', '--format', '{{.State.Running}}', server]), 'true', 'Drasi container exited; see drasi-server.log');
    return healthy(`${rest}/health`);
  });
  await ready('all three real Trading sources', async () => {
    const response = await fetch(`${rest}/api/v1/sources`, { signal: AbortSignal.timeout(1500) });
    assert(response.ok, `Cannot inspect Trading sources: ${response.status}`);
    const json = await response.json();
    const sources = json.data ?? json;
    assert(Array.isArray(sources), 'Unexpected source-list contract');
    return ['price-feed', 'postgres-stocks', 'postgres-broker'].every(id =>
      sources.some((source: { id: string; status?: string }) => source.id === id && source.status?.toLowerCase() === 'running'));
  });
  const loadedResponse = await fetch(`${rest}/api/v1/plugins`, { signal: AbortSignal.timeout(5000) });
  assert(loadedResponse.ok, `Cannot inspect loaded plugin ABI: ${loadedResponse.status}`);
  const loadedPlugins = await loadedResponse.json();
  const loadedPath = join(runDir, 'loaded-plugins.json');
  await writeFile(loadedPath, JSON.stringify(loadedPlugins, null, 2) + '\n');
  command(python, ['-c', [
    'import json, sys, tomllib',
    'sys.path.insert(0, sys.argv[1])',
    'from install_plugins import validate_loaded_plugins',
    'response = json.load(open(sys.argv[2]))',
    'pins = tomllib.load(open(sys.argv[3], "rb"))["plugins"]',
    'validate_loaded_plugins(response["plugins"], pins)',
  ].join('\n'), join(sourceRoot, 'scripts'), loadedPath, lockPath]);
  const apiPort = await freePort();
  start(python, ['-m', 'flask', '--app', join(trading, 'mock-generator/trading_api.py'), 'run',
    '--host', '127.0.0.1', '--port', String(apiPort)], 'trading-api', {
    ...env, POSTGRES_HOST: '127.0.0.1', POSTGRES_PORT: postgresPort,
    POSTGRES_DB: 'trading_demo', POSTGRES_USER: 'drasi_user', POSTGRES_PASSWORD: appPassword,
  });
  const api = `http://127.0.0.1:${apiPort}`;
  await ready('Trading API/database health', () => healthy(`${api}/health`));
  proxy = await startSseProxy(server ? endpoint(server, 8281) : `http://127.0.0.1:${reactionPort}`);
  const endpoints = {
    rest, events: proxy.url, control: proxy.url, api,
    price: server ? endpoint(server, 9100) : `http://127.0.0.1:${pricePort}`,
    ...(native ? { reactionPort } : {}),
  };
  await writeFile(join(runDir, 'endpoints.json'), JSON.stringify(endpoints, null, 2) + '\n');
  const runner = start(process.execPath, [join(app, 'node_modules/@playwright/test/cli.js'),
    'test', '--config', 'test/live/playwright.config.ts'], 'playwright', {
    ...env, P1_LIVE_ENDPOINTS: JSON.stringify(endpoints), P1_WEB_PORT: String(await freePort()),
  }, app);
  const result = await Promise.race([
    new Promise<number | null>((resolve, reject) => { runner.once('error', reject); runner.once('exit', resolve); }),
    delay(180_000, undefined, { ref: false }).then(() => { throw new Error('Real browser gate exceeded 180 seconds'); }),
  ]);
  console.log(await readFile(join(runDir, 'playwright.log'), 'utf8'));
  assert.equal(result, 0, 'Real-server smoke failed (never a passing skip)');
  if (native) command('bash', [join(sourceRoot, 'scripts/prepare-build.sh')]);
  console.log(`Real-server smoke passed with ${runtimeVersion} / ${native ? 'checkout' : platform}.`);
} catch (error) {
  failure = error;
} finally {
  if (proxy) {
    try { await proxy.close(); } catch (error) { cleanupErrors.push(error); }
  }
  for (const child of [...children].reverse()) {
    try { await stop(child); } catch (error) { cleanupErrors.push(error); }
  }
  for (const { id, name } of [...containers].reverse()) {
    try { command('docker', ['stop', '--signal', 'SIGINT', '--timeout', '15', id]); } catch (error) { cleanupErrors.push(error); }
    try {
      const result = spawnSync('docker', ['logs', id], { encoding: 'utf8', timeout: 10_000 });
      if (result.error) throw result.error;
      assert.equal(result.status, 0);
      await writeFile(join(runDir, `${name}.log`), result.stdout + result.stderr);
    } catch (error) { cleanupErrors.push(error); }
    try { command('docker', ['rm', '--force', '--volumes', id]); } catch (error) { cleanupErrors.push(error); }
  }
  if (network) {
    try { command('docker', ['network', 'rm', network]); } catch (error) { cleanupErrors.push(error); }
  }
  for (const log of logs) log.end();
}
if (failure || cleanupErrors.length) {
  throw new AggregateError([...(failure ? [failure] : []), ...cleanupErrors], `Live gate failed; inspect ${runDir} and test-results/live`);
}
