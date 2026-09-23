// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { startExample } from './runtime.mjs';

const runtime = await startExample();
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    void runtime.close().catch(error => { console.error(error); process.exitCode = 1; });
  });
}
