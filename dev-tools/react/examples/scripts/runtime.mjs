// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, copyFile, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { DrasiClient } from '@drasi/react/client';
import { renderServerConfig, verifyRuntimeVersion } from './config.mjs';
import { serveExample } from './web.mjs';

export const exampleRoot = fileURLToPath(new URL('../', import.meta.url));

function command(program, args, cwd, timeout = 120_000) {
  const result = spawnSync(program, args, { cwd, encoding: 'utf8', timeout, maxBuffer: 16 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} failed:\n${result.stderr}\n${result.stdout}`);
  return result.stdout.trim();
}

async function portNumber(value, fallback) {
  const port = Number(value ?? fallback);
  assert(Number.isInteger(port) && (port === 0 || (port >= 1024 && port <= 65535)), 'Invalid example port');
  const listener = createServer();
  await new Promise((accept, reject) => { listener.once('error', reject); listener.listen(port, '127.0.0.1', accept); });
  const address = listener.address();
  assert(address && typeof address !== 'string');
  await new Promise((accept, reject) => listener.close(error => error ? reject(error) : accept()));
  return address.port;
}

async function get(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(1500) });
  if (!response.ok) throw new Error(`Example readiness request failed (${response.status})`);
  return response.json();
}

export async function startExample(env = process.env) {
  const source = resolve(env.P7_SOURCE_ROOT ?? fileURLToPath(new URL('../../../../', import.meta.url)));
  const binary = join(source, 'target/debug/drasi-server');
  await access(binary);
  await access(join(exampleRoot, 'dist/index.html'));
  assert.equal(command('bash', [join(source, 'scripts/prepare-build.sh')], source), 'registry',
    'This example uses the reviewed registry-SDK setup, not an unverified local plugin build');
  const sourceProvenance = JSON.parse(command('python3', [
    join(exampleRoot, 'scripts/verify-plugins.py'), source, '--source',
  ], source));
  const binaryVersion = command(binary, ['--version'], source);
  verifyRuntimeVersion(binaryVersion, {
    server: sourceProvenance.serverVersion,
    sdk: sourceProvenance.selectedDependencies['drasi-plugin-sdk'].version,
  });
  const restPort = await portNumber(env.P7_REST_PORT, 0);
  const feedPort = await portNumber(env.P7_FEED_PORT, 0);
  const ssePort = await portNumber(env.P7_SSE_PORT, 0);
  const webPort = await portNumber(env.P7_WEB_PORT, 5373);
  assert.equal(new Set([restPort, feedPort, ssePort, webPort]).size, 4, 'Example ports must be distinct');
  await mkdir(join(exampleRoot, '.runtime'), { recursive: true });
  const directory = await mkdtemp(join(exampleRoot, '.runtime/live-'));
  await copyFile(join(exampleRoot, 'config/readings.jsonl'), join(directory, 'readings.jsonl'));
  const template = await readFile(join(exampleRoot, 'config/server.yaml'), 'utf8');
  const config = renderServerConfig(template, { restPort, feedPort, ssePort });
  await writeFile(join(directory, 'server.yaml'), config);
  const rest = `http://127.0.0.1:${restPort}`;
  const feed = `http://127.0.0.1:${feedPort}`;
  const events = `http://127.0.0.1:${ssePort}`;
  let child;
  let web;
  let spawnError;
  let stopped = false;
  const log = createWriteStream(join(directory, 'server.log'));
  async function close() {
    if (stopped) return;
    stopped = true;
    const failures = [];
    if (web) {
      try { await web.close(); } catch (error) { failures.push(error); }
    }
    if (child && child.exitCode === null && child.signalCode === null) {
      const exited = new Promise(accept => child.once('exit', accept));
      child.kill('SIGINT');
      await Promise.race([exited, delay(5000, undefined, { ref: false })]);
      if (child.exitCode === null && child.signalCode === null) { child.kill('SIGKILL'); await exited; }
    }
    await new Promise(accept => log.end(accept));
    await writeFile(join(directory, 'cleanup.json'), JSON.stringify({
      owner: 'drasi-react-cold-storage', stopped: failures.length === 0, serverPid: child?.pid,
      serviceUrls: { rest, feed, events, web: web?.url }, retainedData: join(directory, 'data'),
    }, null, 2) + '\n');
    if (failures.length) throw new AggregateError(failures, 'Example service cleanup failed');
  }
  try {
    const installation = command('python3', [
      join(source, 'scripts/install_plugins.py'), '--server-bin', binary,
      '--plugins-dir', join(directory, 'plugins'),
    ], source, 300_000);
    await writeFile(join(directory, 'plugin-install.log'), installation + '\n');
    child = spawn(binary, [
      '--config', join(directory, 'server.yaml'),
      '--plugins-dir', join(directory, 'plugins'),
    ], {
      cwd: directory,
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    child.stdout.pipe(log, { end: false });
    child.stderr.pipe(log, { end: false });
    child.on('error', error => { spawnError = error; });
    const snapshots = {};
    const deadline = Date.now() + 60_000;
    let ready = false;
    while (Date.now() < deadline) {
      if (spawnError) throw spawnError;
      assert(child.exitCode === null && child.signalCode === null, `Owned server exited; see ${directory}/server.log`);
      try {
        for (const query of ['north-room', 'south-room']) {
          snapshots[query] = await get(`${rest}/api/v1/instances/cold-chain/queries/${query}/results`);
        }
        ready = Object.values(snapshots).every(body => body.success === true && body.data?.length === 1);
        if (ready) break;
      } catch (error) {
        const unavailable = error instanceof TypeError && ['ECONNREFUSED', 'ECONNRESET', 'UND_ERR_SOCKET'].includes(error.cause?.code);
        if (!unavailable && !(error instanceof DOMException && error.name === 'TimeoutError')) throw error;
      }
      await delay(100);
    }
    assert(ready, `Pre-created queries did not become ready in 60 seconds; see ${directory}/server.log`);
    const validator = new DrasiClient({
      serverUrl: rest, instanceId: 'cold-chain', queryIds: ['north-room', 'south-room'],
      reaction: { id: 'cold-chain-events', endpoint: `${events}/events` },
    });
    try { await validator.validateResources(); } finally { await validator.disconnect(); }
    const plugins = await get(`${rest}/api/v1/plugins`);
    assert(Array.isArray(plugins.plugins), 'Invalid loaded-plugin response');
    await writeFile(join(directory, 'loaded-plugins.json'), JSON.stringify(plugins, null, 2) + '\n');
    command('python3', [join(exampleRoot, 'scripts/verify-plugins.py'), source, join(directory, 'loaded-plugins.json')], source);
    const provenance = {
      sourceRevision: sourceProvenance.serverRevision,
      coreRevision: sourceProvenance.engineGit,
      cargoLockSha256: sourceProvenance.serverCargoLockSha256,
      manifestSha256: sourceProvenance.serverManifestSha256,
      binarySha256: createHash('sha256').update(await readFile(binary)).digest('hex'),
      binaryVersion, sourceProvenance,
      configTemplateSha256: createHash('sha256').update(template).digest('hex'),
      configSha256: createHash('sha256').update(config).digest('hex'),
      seedSha256: createHash('sha256').update(await readFile(join(directory, 'readings.jsonl'))).digest('hex'),
      node: process.version, sourceRoot: source, instanceId: 'cold-chain', snapshots,
    };
    await writeFile(join(directory, 'provenance.json'), JSON.stringify(provenance, null, 2) + '\n');
    web = await serveExample({ dist: join(exampleRoot, 'dist'), rest, events, port: webPort });
    await writeFile(join(directory, 'ready.json'), JSON.stringify({ web: web.url, rest, feed, events }, null, 2) + '\n');
    console.log(`Example ready: ${web.url}\nQueryTable: ${web.url}/query-table.html\nHooks: ${web.url}/hooks.html\nSimulated states: ${web.url}/showcase.html\nFeed: ${feed}\nFrom dev-tools/react/examples: npm run feed -- ${feed} warm\nOwned diagnostics: ${directory}`);
    return {
      web: web.url, rest, feed, events, directory, close, requests: web.requests,
      setStreamsAvailable: web.setStreamsAvailable,
    };
  } catch (error) {
    await close();
    throw error;
  }
}
