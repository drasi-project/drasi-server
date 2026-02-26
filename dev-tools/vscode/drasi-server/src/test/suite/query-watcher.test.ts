import * as assert from 'assert';

/**
 * Tests for QueryWatcher SSE parsing logic.
 * These tests verify the SSE chunk handling extracted from query-watcher.ts.
 */

interface ParsedEvent {
  type: 'json' | 'raw' | 'skip';
  payload?: any;
  raw?: string;
}

/**
 * Simulates the handleSseChunk logic from QueryWatcher
 */
function parseSSEChunk(chunk: string): ParsedEvent[] {
  const results: ParsedEvent[] = [];
  const lines = chunk.split('\n');
  for (const line of lines) {
    // Skip comments (heartbeats)
    if (line.startsWith(':')) {
      results.push({ type: 'skip' });
      continue;
    }
    if (!line.startsWith('data:')) {
      continue;
    }
    const payload = line.replace(/^data:\s?/, '');
    if (!payload) {
      continue;
    }
    try {
      const parsed = JSON.parse(payload);
      results.push({ type: 'json', payload: parsed });
    } catch {
      results.push({ type: 'raw', raw: payload });
    }
  }
  return results;
}

/**
 * Simulates SSE buffer processing from QueryWatcher.startStreaming
 */
function processSSEBuffer(buffer: string): { events: string[]; remaining: string } {
  const events: string[] = [];
  let normalized = buffer.replace(/\r\n/g, '\n');
  let boundary = normalized.indexOf('\n\n');
  while (boundary >= 0) {
    const chunk = normalized.slice(0, boundary).trim();
    normalized = normalized.slice(boundary + 2);
    if (chunk) {
      events.push(chunk);
    }
    boundary = normalized.indexOf('\n\n');
  }
  return { events, remaining: normalized };
}

suite('QueryWatcher SSE Parsing', () => {
  test('parses data events correctly', () => {
    const chunk = 'data: {"id":"123","status":"completed"}';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 1);
    assert.strictEqual(results[0].type, 'json');
    assert.deepStrictEqual(results[0].payload, { id: '123', status: 'completed' });
  });

  test('handles heartbeat comments', () => {
    const chunk = ': heartbeat';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 1);
    assert.strictEqual(results[0].type, 'skip');
  });

  test('handles multiple lines in a chunk', () => {
    const chunk = ': heartbeat\ndata: {"foo":"bar"}';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 2);
    assert.strictEqual(results[0].type, 'skip');
    assert.strictEqual(results[1].type, 'json');
    assert.deepStrictEqual(results[1].payload, { foo: 'bar' });
  });

  test('handles invalid JSON as raw', () => {
    const chunk = 'data: not-valid-json';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 1);
    assert.strictEqual(results[0].type, 'raw');
    assert.strictEqual(results[0].raw, 'not-valid-json');
  });

  test('ignores non-data lines', () => {
    const chunk = 'event: custom\nid: 12345\ndata: {"test":true}';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 1);
    assert.strictEqual(results[0].type, 'json');
    assert.deepStrictEqual(results[0].payload, { test: true });
  });

  test('handles data: with no space', () => {
    const chunk = 'data:{"compact":true}';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 1);
    assert.strictEqual(results[0].type, 'json');
    assert.deepStrictEqual(results[0].payload, { compact: true });
  });

  test('handles empty data payload', () => {
    const chunk = 'data: ';
    const results = parseSSEChunk(chunk);
    
    assert.strictEqual(results.length, 0);
  });
});

suite('QueryWatcher SSE Buffer Processing', () => {
  test('splits events on double newline (LF)', () => {
    const buffer = 'data: {"first":1}\n\ndata: {"second":2}\n\n';
    const { events, remaining } = processSSEBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], 'data: {"first":1}');
    assert.strictEqual(events[1], 'data: {"second":2}');
    assert.strictEqual(remaining, '');
  });

  test('splits events on double newline (CRLF)', () => {
    const buffer = 'data: {"first":1}\r\n\r\ndata: {"second":2}\r\n\r\n';
    const { events, remaining } = processSSEBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], 'data: {"first":1}');
    assert.strictEqual(events[1], 'data: {"second":2}');
    assert.strictEqual(remaining, '');
  });

  test('keeps partial event in remaining buffer', () => {
    const buffer = 'data: {"complete":true}\n\ndata: {"partial":';
    const { events, remaining } = processSSEBuffer(buffer);
    
    assert.strictEqual(events.length, 1);
    assert.strictEqual(events[0], 'data: {"complete":true}');
    assert.strictEqual(remaining, 'data: {"partial":');
  });

  test('handles heartbeat events', () => {
    const buffer = ': heartbeat\n\ndata: {"real":"data"}\n\n';
    const { events, remaining } = processSSEBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], ': heartbeat');
    assert.strictEqual(events[1], 'data: {"real":"data"}');
    assert.strictEqual(remaining, '');
  });

  test('handles empty buffer', () => {
    const { events, remaining } = processSSEBuffer('');
    
    assert.strictEqual(events.length, 0);
    assert.strictEqual(remaining, '');
  });

  test('handles buffer with no complete events', () => {
    const buffer = 'data: {"incomplete":true}';
    const { events, remaining } = processSSEBuffer(buffer);
    
    assert.strictEqual(events.length, 0);
    assert.strictEqual(remaining, 'data: {"incomplete":true}');
  });
});
