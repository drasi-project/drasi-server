// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { join } from 'node:path';
import { exampleRoot } from '../scripts/runtime.mjs';
import { serveExample } from '../scripts/web.mjs';

const preview = await serveExample({
  dist: join(exampleRoot, 'dist'), port: Number(process.env.P7_WEB_PORT ?? 15373),
});
console.log(`Static simulation preview: ${preview.url}/showcase.html (no backend)`);
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    void preview.close().catch(error => { console.error(error); process.exitCode = 1; });
  });
}
