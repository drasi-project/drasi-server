// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { createServer, request as httpRequest } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

export async function serveExample({ dist, rest = null, events = null, port = 5373 }) {
  for (const target of [rest, events]) {
    assert(target === null || /^http:\/\/127\.0\.0\.1:\d+$/.test(target), 'Example proxy targets must be owned loopback services');
  }
  const streams = new Map();
  let streamsAvailable = true;
  const requests = [];
  const root = resolve(dist);
  const server = createServer((request, response) => {
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('X-Frame-Options', 'DENY');
    response.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'");
    void handle(request, response).catch(error => {
      console.error('Example request failed:', error);
      if (!response.headersSent) response.writeHead(500, { 'Content-Type': 'text/plain' });
      response.end('Example request failed; see the local server log.');
    });
  });
  async function handle(request, response) {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1');
    if (request.method !== 'GET') {
      response.writeHead(405, { Allow: 'GET' }).end('This browser example is read-only.');
      return;
    }
    if (url.pathname === '/events' || url.pathname.startsWith('/api/v1/instances/cold-chain/')) {
      requests.push({ method: request.method, path: url.pathname + url.search });
      const target = url.pathname === '/events' ? events : rest;
      if (!target || (url.pathname === '/events' && !streamsAvailable)) {
        response.writeHead(503, { 'Content-Type': 'text/plain' }).end(
          target ? 'Controlled example transport outage.' : 'Static showcase only. Run npm start for real data.');
        return;
      }
      const upstream = httpRequest(`${target}${url.pathname}${url.search}`, { headers: { Accept: request.headers.accept ?? '*/*' } }, incoming => {
        response.writeHead(incoming.statusCode ?? 502, {
          'Content-Type': incoming.headers['content-type'] ?? 'application/json',
          'Cache-Control': 'no-store',
        });
        response.flushHeaders();
        incoming.pipe(response);
        incoming.on('error', error => {
          console.error('Example upstream stream failed:', error.code ?? error.name);
          response.destroy();
        });
      });
      streams.set(response, { upstream, events: url.pathname === '/events' });
      response.on('close', () => { upstream.destroy(); streams.delete(response); });
      upstream.on('error', error => {
        if (response.destroyed) return;
        console.error('Example upstream unavailable:', error.code ?? error.name);
        if (!response.headersSent) response.writeHead(502, { 'Content-Type': 'text/plain' });
        response.end('Example backend unavailable. Start it with the documented configuration.');
      });
      upstream.end();
      return;
    }
    let pathname;
    try { pathname = decodeURIComponent(url.pathname); }
    catch { response.writeHead(400).end('Invalid asset path.'); return; }
    const path = resolve(root, `.${pathname === '/' ? '/index.html' : pathname}`);
    const extension = Object.keys(types).find(suffix => path.endsWith(suffix));
    if (pathname.includes('\0') || pathname.includes('\\') || !path.startsWith(root + sep) || !extension) {
      response.writeHead(404).end('Example asset not found.');
      return;
    }
    try {
      const content = await readFile(path);
      response.writeHead(200, { 'Content-Type': types[extension] }).end(content);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      response.writeHead(404).end('Example asset not found. Run npm run build first.');
    }
  }
  await new Promise((accept, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => { server.off('error', reject); accept(); });
  });
  const address = server.address();
  assert(address && typeof address !== 'string');
  return {
    url: `http://127.0.0.1:${address.port}`,
    requests,
    setStreamsAvailable(available) {
      streamsAvailable = available;
      if (!available) for (const [response, stream] of streams) {
        if (stream.events) { stream.upstream.destroy(); response.end(); }
      }
    },
    async close() {
      for (const [response, { upstream }] of streams) { upstream.destroy(); response.end(); }
      streams.clear();
      server.closeAllConnections();
      await new Promise((accept, reject) => server.close(error => error ? reject(error) : accept()));
    },
  };
}
