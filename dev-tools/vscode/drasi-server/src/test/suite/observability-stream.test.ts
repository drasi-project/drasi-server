import * as assert from 'assert';
import { ObservabilityStream } from '../../observability-stream';

class TestViewer {
  items: any[] = [];
  raw: string[] = [];
  errors: string[] = [];

  appendItems(items: any[]) {
    this.items.push(...items);
  }

  appendRaw(raw: string) {
    this.raw.push(raw);
  }

  appendError(message: string) {
    this.errors.push(message);
  }
}

suite('ObservabilityStream', () => {
  test('parses SSE chunks with CRLF', async () => {
    const stream = new ObservabilityStream();
    const viewer = new TestViewer();
    const encoder = new TextEncoder();
    const chunks = [
      'data: {"foo":1}\r\n\r\n',
      'data: not-json\r\n\r\n',
    ];
    let index = 0;
    const reader = {
      read: async () => {
        if (index >= chunks.length) {
          return { value: undefined, done: true };
        }
        const value = encoder.encode(chunks[index]);
        index += 1;
        return { value, done: false };
      },
    };
    const response = {
      ok: true,
      body: {
        getReader: () => reader,
      },
    };
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () => response) as any;
    try {
      await stream.stream('http://example', viewer as any);
    } finally {
      globalThis.fetch = originalFetch;
    }
    assert.strictEqual(viewer.items.length, 1);
    assert.deepStrictEqual(viewer.items[0], { foo: 1 });
    assert.strictEqual(viewer.raw.length, 1);
    assert.strictEqual(viewer.raw[0], 'not-json');
  });

  test('reports non-ok responses', async () => {
    const stream = new ObservabilityStream();
    const viewer = new TestViewer();
    const response = {
      ok: false,
      status: 500,
      statusText: 'Server error',
      body: null,
    };
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () => response) as any;
    try {
      await stream.stream('http://example', viewer as any);
    } finally {
      globalThis.fetch = originalFetch;
    }
    assert.ok(viewer.errors.some((err) => err.includes('500')));
  });
});
