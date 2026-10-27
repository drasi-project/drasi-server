// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe('Trading API address', () => {
  it.each([
    [undefined, 'http://localhost:9200'],
    ['http://127.0.0.1:49346', 'http://127.0.0.1:49346'],
  ])('uses %s without changing the API contract', async (configured, expected) => {
    vi.stubEnv('VITE_TRADING_API_URL', configured);
    vi.resetModules();
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: [] })));
    vi.stubGlobal('fetch', fetcher);
    const { tradingApi } = await import('../../src/services/TradingApi');
    expect(await tradingApi.getStocks()).toEqual([]);
    expect(fetcher).toHaveBeenCalledWith(`${expected}/api/stocks`);
  });
});
