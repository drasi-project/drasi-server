import { DrasiClient } from '@drasi/react/client';
import { expect, it, vi } from 'vitest';

it('the packaged client accepts current Server query metadata without retired bootstrap settings', async () => {
  const path = '/api/v1/instances/gpu-demo/queries/ui-gpus';
  const config = {
    id: 'ui-gpus',
    query: 'MATCH (g:Gpu) RETURN g.id AS gpu_id',
    queryLanguage: 'Cypher',
    autoStart: true,
    enableBootstrap: true,
    outboxCapacity: 128,
    bootstrapTimeoutSecs: 30,
    middleware: [],
    inputs: ['gpu-inventory'],
  };
  const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(JSON.stringify({
    success: true,
    data: {
      id: 'ui-gpus',
      status: 'Running',
      config,
      links: { self: path, full: `${path}?view=full` },
    },
    error: null,
  }), { status: 200, headers: { 'Content-Type': 'application/json' } }));
  const client = new DrasiClient({
    serverUrl: 'http://gpu.example',
    instanceId: 'gpu-demo',
    outputIds: ['ui-gpus'],
    reaction: { id: 'gpu-demo-ui', endpoint: 'http://gpu.example/events/gpu-demo' },
    fetch,
  });
  try {
    expect((await client.getQuery('ui-gpus')).config).toEqual(config);
    expect(fetch).toHaveBeenCalledWith(
      `http://gpu.example${path}?view=full`,
      expect.objectContaining({ method: 'GET' }),
    );
  } finally {
    await client.disconnect();
  }
});
