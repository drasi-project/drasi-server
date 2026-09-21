// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it } from 'vitest';
import { verifyRuntimeVersion } from '../live/runtimeVersion';

const expected = { server: '0.2.3', sdk: '0.11.0' };
const banner = 'drasi-server 0.2.3\nrustc: rustc 1.95.0\nplugin-sdk: 0.11.0';

describe('actual native runtime diagnostics', () => {
  it('reports the verified binary version rather than the historical image version', () => {
    expect(verifyRuntimeVersion(banner, expected)).toBe('drasi-server 0.2.3');
  });

  it('accepts platform line endings without weakening the version checks', () => {
    expect(verifyRuntimeVersion(banner.replaceAll('\n', '\r\n'), expected)).toBe('drasi-server 0.2.3');
  });

  it.each([
    banner.replace('drasi-server 0.2.3', 'drasi-server 0.2.1'),
    '',
  ])('rejects a stale or missing server version', output => {
    expect(() => verifyRuntimeVersion(output, expected)).toThrow('locked server version');
  });

  it.each([
    banner.replace('plugin-sdk: 0.11.0', 'plugin-sdk: 0.10.0'),
    banner.replace('\nplugin-sdk: 0.11.0', ''),
    `${banner}\nplugin-sdk: 0.11.0`,
  ])('rejects stale, missing or ambiguous SDK metadata', output => {
    expect(() => verifyRuntimeVersion(output, expected)).toThrow('resolved plugin SDK version');
  });
});
