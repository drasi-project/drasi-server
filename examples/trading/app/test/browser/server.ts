// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { SyntheticTrading, type FixtureRow } from '../fixtures/synthetic/trading.ts';

const dist = resolve(fileURLToPath(new URL('../../dist/', import.meta.url)));
const consumerDist = resolve(fileURLToPath(new URL('../../.test-runtime/consumer-dist/', import.meta.url)));
const port = Number(process.env.P1_WEB_PORT ?? 15273);
if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error('Invalid P1_WEB_PORT');

let backend = new SyntheticTrading();
let online = true;
const streams = new Set<ServerResponse>();
const failures: string[] = [];

function connectBackend(): void {
  backend.onBatch = batch => {
    for (const stream of streams) stream.write(`data: ${JSON.stringify(batch)}\n\n`);
  };
}
connectBackend();

async function readJson(request: IncomingMessage): Promise<FixtureRow> {
  let body = '';
  for await (const chunk of request) {
    body += chunk;
    if (body.length > 64 * 1024) throw new Error('Fixture body exceeds 64 KiB');
  }
  if (!body) return {};
  const value: unknown = JSON.parse(body);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected JSON object');
  return value as FixtureRow;
}

function json(response: ServerResponse, status: number, body: unknown): void {
  response.writeHead(status, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify(body));
}

const server = createServer((request, response) => {
  response.setHeader('Access-Control-Allow-Origin', '*');
  response.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  response.setHeader('Access-Control-Allow-Headers', 'Content-Type');
  void handle(request, response).catch(error => {
    failures.push(String(error));
    console.error(error);
    if (!response.headersSent) json(response, 500, { error: 'Fixture server failed; see log' });
    else response.destroy();
  });
});

async function handle(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const url = new URL(request.url ?? '/', 'http://127.0.0.1');
  const method = request.method ?? 'GET';
  if (method === 'OPTIONS') {
    response.writeHead(204);
    response.end();
    return;
  }
  if (url.pathname === '/__fixture/health') {
    await readFile(resolve(dist, 'index.html'));
    json(response, 200, { ready: true });
    return;
  }
  if (process.env.P6_COMPONENTS_ONLY === '1' &&
      url.pathname !== '/__components' && !url.pathname.startsWith('/__components/')) {
    json(response, 403, { error: 'Component-only preview: open /__components/; Trading and API routes are disabled' });
    return;
  }
  if (process.env.P1_REAL_SERVER_ONLY === '1' &&
      (url.pathname.startsWith('/api/') || url.pathname.startsWith('/__fixture/') ||
       url.pathname === '/events' || url.pathname === '/health')) {
    throw new Error('Synthetic endpoints are disabled in the real-server gate');
  }
  if (url.pathname === '/__fixture/state') {
    json(response, 200, { requests: backend.requests, failures, connections: streams.size });
    return;
  }
  if (url.pathname.startsWith('/__fixture/') && method === 'POST') {
    const body = await readJson(request);
    switch (url.pathname) {
      case '/__fixture/reset':
        for (const stream of streams) stream.end();
        streams.clear();
        backend = new SyntheticTrading();
        online = true;
        failures.length = 0;
        connectBackend();
        break;
      case '/__fixture/price':
        if (typeof body.symbol !== 'string' || typeof body.price !== 'number') throw new Error('Invalid price fixture');
        if (body.volume !== undefined && typeof body.volume !== 'number') throw new Error('Invalid volume fixture');
        backend.changePrice(body.symbol, body.price, body.volume);
        break;
      case '/__fixture/disconnect':
        online = false;
        for (const stream of streams) stream.end();
        streams.clear();
        break;
      case '/__fixture/reconnect':
        online = true;
        break;
      case '/__fixture/partial':
        backend.queries.delete('portfolio-query');
        backend.queryStatuses.set('watchlist-query', 'Stopped');
        backend.reactionStatus = 'Stopped';
        break;
      default: throw new Error(`Unknown fixture control ${url.pathname}`);
    }
    json(response, 200, { success: true });
    return;
  }
  if (url.pathname === '/events') {
    if (!online) {
      json(response, 503, { error: 'Controlled fixture outage' });
      return;
    }
    response.writeHead(200, {
      'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    });
    response.write(': synthetic transport, not a recorded server contract\n\n');
    streams.add(response);
    response.on('close', () => streams.delete(response));
    return;
  }
  if (url.pathname === '/health' || url.pathname.startsWith('/api/')) {
    const result = backend.handle({ method, path: url.pathname, body: await readJson(request) });
    json(response, result.status, result.body);
    return;
  }
  let pathname: string;
  try {
    pathname = decodeURIComponent(url.pathname);
  } catch {
    json(response, 400, { error: 'Invalid static asset path' });
    return;
  }
  if (pathname.includes('\0') || pathname.includes('\\')) {
    json(response, 400, { error: 'Invalid static asset path' });
    return;
  }
  const isConsumer = pathname === '/__components' || pathname.startsWith('/__components/');
  const root = isConsumer ? consumerDist : dist;
  const relative = isConsumer ? pathname.slice('/__components'.length) : pathname;
  const path = resolve(root, `.${relative === '/' || relative === '' ? '/index.html' : relative}`);
  if (!path.startsWith(`${root}${sep}`)) {
    json(response, 400, { error: 'Invalid static asset path' });
    return;
  }
  const contentTypes: Record<string, string> = {
    html: 'text/html', js: 'text/javascript', css: 'text/css', svg: 'image/svg+xml',
  };
  try {
    const content = await readFile(path);
    response.writeHead(200, { 'Content-Type': contentTypes[path.split('.').at(-1)!] ?? 'application/octet-stream' });
    response.end(content);
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
      json(response, 404, { error: 'Static asset not found' });
      return;
    }
    throw error;
  }
}

server.listen(port, '127.0.0.1', () => console.log(`Trading fixture server: http://127.0.0.1:${port}`));
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => {
    for (const stream of streams) stream.end();
    server.close();
    server.closeAllConnections();
  });
}
