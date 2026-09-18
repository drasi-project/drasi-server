// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { createServer, request as httpRequest, type ClientRequest, type ServerResponse } from 'node:http';
import { pipeline } from 'node:stream';

/** Transparent network-fault injector: never interprets or manufactures SSE data. */
export async function startSseProxy(target: string): Promise<{ url: string; close: () => Promise<void> }> {
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(target)) throw new Error('SSE test proxy requires an owned loopback endpoint');
  let online = true;
  const connections = new Map<ServerResponse, ClientRequest>();
  const disconnect = () => {
    for (const [response, upstream] of connections) {
      upstream.destroy();
      response.end();
    }
    connections.clear();
  };
  const server = createServer((request, response) => {
    response.setHeader('Access-Control-Allow-Origin', '*');
    const path = new URL(request.url ?? '/', 'http://127.0.0.1').pathname;
    if (request.method === 'POST' && (path === '/disconnect' || path === '/reconnect')) {
      online = path === '/reconnect';
      if (!online) disconnect();
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ online }));
      return;
    }
    if (path !== '/events' || request.method !== 'GET') {
      response.writeHead(404);
      response.end();
      return;
    }
    if (!online) {
      response.writeHead(503);
      response.end('Controlled test network outage');
      return;
    }
    const upstream = httpRequest(`${target}/events`, { headers: { Accept: 'text/event-stream' } }, stream => {
      response.writeHead(stream.statusCode ?? 502, {
        'Content-Type': stream.headers['content-type'] ?? 'text/event-stream',
        'Cache-Control': 'no-cache',
      });
      response.flushHeaders();
      pipeline(stream, response, error => {
        if (error && online) console.warn(`SSE proxy stream interrupted: ${error.message}`);
      });
    });
    connections.set(response, upstream);
    response.on('close', () => {
      upstream.destroy();
      connections.delete(response);
    });
    upstream.on('error', error => {
      if (!response.writableEnded) {
        console.warn(`SSE proxy upstream unavailable: ${error.message}`);
        if (!response.headersSent) response.writeHead(502);
        response.end();
      }
    });
    upstream.end();
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      server.off('error', reject);
      resolve();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('SSE test proxy did not bind a TCP port');
  return {
    url: `http://127.0.0.1:${address.port}`,
    close: async () => {
      disconnect();
      server.closeAllConnections();
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    },
  };
}
