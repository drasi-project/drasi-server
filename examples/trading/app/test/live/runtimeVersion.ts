// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';

export function verifyRuntimeVersion(output: string, expected: { server: string; sdk: string }): string {
  const lines = output.trim().split(/\r?\n/);
  const version = `drasi-server ${expected.server}`;
  assert.equal(lines[0], version, 'Native binary does not match the locked server version; rebuild the checkout');
  assert.deepEqual(lines.filter(line => line.startsWith('plugin-sdk:')), [`plugin-sdk: ${expected.sdk}`],
    'Native binary does not match the resolved plugin SDK version; rebuild the checkout');
  return version;
}
