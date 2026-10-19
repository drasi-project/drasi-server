import * as assert from 'assert';
import { parseQuerySseEvent, processSseBuffer } from '../../query-sse';

suite('QueryWatcher SSE Parsing', () => {
  test('parses data events correctly', () => {
    const result = parseQuerySseEvent('data: {"query_id":"123","results":[]}');
    assert.strictEqual(result?.kind, 'changes');
    if (result?.kind === 'changes') {
      assert.deepStrictEqual(result.payload, { query_id: '123', results: [] });
    }
  });

  test('handles heartbeat comments', () => {
    const chunk = ': heartbeat';
    assert.strictEqual(parseQuerySseEvent(chunk), undefined);
  });

  test('handles multiple lines in a chunk', () => {
    const chunk = ': heartbeat\nevent: snapshot\ndata: [{"foo":"bar"}]';
    assert.deepStrictEqual(parseQuerySseEvent(chunk), {
      kind: 'snapshot', rows: [{ foo: 'bar' }],
    });
  });

  test('rejects invalid JSON rather than treating it as successful data', () => {
    assert.throws(() => parseQuerySseEvent('data: not-valid-json'));
  });

  test('ignores event IDs while preserving the event type', () => {
    assert.deepStrictEqual(parseQuerySseEvent('event: snapshot\nid: 12345\ndata: []'), {
      kind: 'snapshot', rows: [],
    });
    assert.throws(() => parseQuerySseEvent('event: custom\ndata: {"results":[]}'));
  });

  test('handles data: with no space', () => {
    assert.strictEqual(parseQuerySseEvent('data:{"results":[]}')?.kind, 'changes');
  });

  test('handles empty data payload', () => {
    const chunk = 'data: ';
    assert.strictEqual(parseQuerySseEvent(chunk), undefined);
  });

  test('joins multiline JSON and validates snapshot rows', () => {
    assert.deepStrictEqual(parseQuerySseEvent('event: snapshot\ndata: [\ndata: {"value":1}]'), {
      kind: 'snapshot', rows: [{ value: 1 }],
    });
    assert.throws(() => parseQuerySseEvent('event: snapshot\ndata: [42]'));
    assert.throws(() => parseQuerySseEvent('event: snapshot\ndata: {"rows":[]}'));
  });
});

suite('QueryWatcher SSE Buffer Processing', () => {
  test('splits events on double newline (LF)', () => {
    const buffer = 'data: {"first":1}\n\ndata: {"second":2}\n\n';
    const { events, remaining } = processSseBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], 'data: {"first":1}');
    assert.strictEqual(events[1], 'data: {"second":2}');
    assert.strictEqual(remaining, '');
  });

  test('splits events on double newline (CRLF)', () => {
    const buffer = 'data: {"first":1}\r\n\r\ndata: {"second":2}\r\n\r\n';
    const { events, remaining } = processSseBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], 'data: {"first":1}');
    assert.strictEqual(events[1], 'data: {"second":2}');
    assert.strictEqual(remaining, '');
  });

  test('keeps partial event in remaining buffer', () => {
    const buffer = 'data: {"complete":true}\n\ndata: {"partial":';
    const { events, remaining } = processSseBuffer(buffer);
    
    assert.strictEqual(events.length, 1);
    assert.strictEqual(events[0], 'data: {"complete":true}');
    assert.strictEqual(remaining, 'data: {"partial":');
  });

  test('handles heartbeat events', () => {
    const buffer = ': heartbeat\n\ndata: {"real":"data"}\n\n';
    const { events, remaining } = processSseBuffer(buffer);
    
    assert.strictEqual(events.length, 2);
    assert.strictEqual(events[0], ': heartbeat');
    assert.strictEqual(events[1], 'data: {"real":"data"}');
    assert.strictEqual(remaining, '');
  });

  test('handles empty buffer', () => {
    const { events, remaining } = processSseBuffer('');
    
    assert.strictEqual(events.length, 0);
    assert.strictEqual(remaining, '');
  });

  test('handles buffer with no complete events', () => {
    const buffer = 'data: {"incomplete":true}';
    const { events, remaining } = processSseBuffer(buffer);
    
    assert.strictEqual(events.length, 0);
    assert.strictEqual(remaining, 'data: {"incomplete":true}');
  });
});
