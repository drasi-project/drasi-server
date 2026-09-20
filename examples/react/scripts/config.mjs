// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';

export function renderServerConfig(template, { restPort, feedPort, ssePort }) {
  for (const [name, port] of Object.entries({
    EXAMPLE_REST_PORT: restPort, EXAMPLE_FEED_PORT: feedPort, EXAMPLE_SSE_PORT: ssePort,
  })) {
    assert(Number.isInteger(port) && port >= 1024 && port <= 65535, `Invalid allocated ${name}`);
    const token = `"\${${name}}"`;
    assert.equal(template.split(token).length, 2, `Expected exactly one ${name} template field`);
    // Full-view REST config preserves plugin expressions; install concrete numeric ports.
    template = template.replace(token, String(port));
  }
  assert(!template.includes('${'), 'Unexpected unresolved example configuration field');
  return template;
}
