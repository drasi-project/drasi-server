// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { startExample, exampleRoot } from '../scripts/runtime.mjs';

const runtime = await startExample({
  ...process.env, P7_WEB_PORT: '0', P7_REST_PORT: '0', P7_SSE_PORT: '0', P7_FEED_PORT: '0',
});
const control = createServer((request, response) => {
  if (request.method !== 'POST' || !['/offline', '/online'].includes(request.url)) {
    response.writeHead(404).end();
    return;
  }
  runtime.setStreamsAvailable(request.url === '/online');
  response.writeHead(200).end('Only the owned SSE transport was changed; payloads are untouched.');
});
let child;
const interrupted = () => child?.kill('SIGINT');
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, interrupted);
try {
  await new Promise((accept, reject) => { control.once('error', reject); control.listen(0, '127.0.0.1', accept); });
  const address = control.address();
  assert(address && typeof address !== 'string');
  child = spawn(process.execPath, [
    join(exampleRoot, 'node_modules/@playwright/test/cli.js'), 'test', ...process.argv.slice(2),
  ], {
    cwd: exampleRoot, stdio: 'inherit',
    env: {
      ...process.env, P7_WEB_URL: runtime.web,
      P7_LIVE_ENDPOINTS: JSON.stringify({
        rest: runtime.rest, feed: runtime.feed,
        control: `http://127.0.0.1:${address.port}`,
      }),
    },
  });
  const code = await new Promise((accept, reject) => { child.once('error', reject); child.once('exit', accept); });
  assert.equal(code, 0, `Real-example browser gate failed; diagnostics: ${runtime.directory}`);
} finally {
  for (const signal of ['SIGINT', 'SIGTERM']) process.removeListener(signal, interrupted);
  control.closeAllConnections();
  await new Promise((accept, reject) => control.close(error => error ? reject(error) : accept()));
  await writeFile(join(runtime.directory, 'browser-reads.json'), JSON.stringify(runtime.requests, null, 2) + '\n');
  await runtime.close();
}
