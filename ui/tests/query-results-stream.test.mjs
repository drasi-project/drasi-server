import assert from "node:assert/strict";
import test from "node:test";
import { subscribeQueryResults } from "../src/api/query-results-stream.ts";

function fixture(t) {
  let source;
  const original = globalThis.EventSource;
  globalThis.EventSource = class extends EventTarget {
    closed = false;
    constructor(path) {
      super();
      this.path = path;
      source = this;
    }
    close() { this.closed = true; }
    open() { this.onopen(new Event("open")); }
    error() { this.onerror(new Event("error")); }
    snapshot(rows) {
      this.dispatchEvent(new MessageEvent("snapshot", { data: JSON.stringify(rows) }));
    }
    change(results) {
      this.onmessage(new MessageEvent("message", { data: JSON.stringify({ results }) }));
    }
  };
  t.after(() => { globalThis.EventSource = original; });
  let rows = [];
  const changes = [];
  const statuses = [];
  const close = subscribeQueryResults("/api/v1/instances/first/queries/q/attach", {
    onSnapshot(value) { rows = value; },
    onChange(value) { changes.push(value); },
    onStatus(value) { statuses.push(value); },
  });
  return { source, close, changes, statuses, rows: () => rows };
}

test("connection becomes current only after its initial snapshot", (t) => {
  const f = fixture(t);
  assert.equal(f.source.path, "/api/v1/instances/first/queries/q/attach");
  f.source.open();
  assert.deepEqual(f.statuses.at(-1), { loading: true, streaming: false, error: null });
  f.source.snapshot([{ value: 1 }]);
  assert.deepEqual(f.rows(), [{ value: 1 }]);
  assert.deepEqual(f.statuses.at(-1), { loading: false, streaming: true, error: null });
  f.source.change([{ type: "ADD", data: { value: 2 } }]);
  assert.deepEqual(f.changes, [{ results: [{ type: "ADD", data: { value: 2 } }] }]);
  f.close();
});

test("reconnection replaces stale rows, including an empty replacement", (t) => {
  const f = fixture(t);
  f.source.open();
  f.source.snapshot([{ value: 1 }, { value: 2 }]);
  f.source.error();
  assert.equal(f.source.closed, false);
  assert.equal(f.statuses.at(-1).streaming, false);
  assert.match(f.statuses.at(-1).error, /fresh snapshot/);
  f.source.open();
  assert.equal(f.statuses.at(-1).streaming, false);
  f.source.snapshot([{ value: 3 }]);
  assert.deepEqual(f.rows(), [{ value: 3 }]);
  f.source.error();
  f.source.open();
  f.source.snapshot([]);
  assert.deepEqual(f.rows(), []);
  assert.equal(f.statuses.at(-1).streaming, true);
  f.close();
});

test("changes before a snapshot fail instead of using stale state", (t) => {
  const f = fixture(t);
  f.source.open();
  f.source.change([{ type: "ADD", data: { value: 1 } }]);
  assert.equal(f.source.closed, true);
  assert.match(f.statuses.at(-1).error, /before the initial snapshot/);
  assert.deepEqual(f.changes, []);
});

test("malformed snapshots and changes are explicit failures", (t) => {
  const f = fixture(t);
  f.source.open();
  f.source.snapshot({ rows: [] });
  assert.equal(f.source.closed, true);
  assert.match(f.statuses.at(-1).error, /Invalid query result stream/);
});

test("malformed changes do not silently disappear", (t) => {
  const f = fixture(t);
  f.source.open();
  f.source.snapshot([]);
  f.source.change([{ type: "ADD", data: 42 }]);
  assert.equal(f.source.closed, true);
  assert.match(f.statuses.at(-1).error, /Invalid query result stream/);
  assert.deepEqual(f.changes, []);
});

test("disposed connections cannot overwrite a newly selected query", (t) => {
  const f = fixture(t);
  f.source.open();
  f.source.snapshot([{ value: 1 }]);
  f.close();
  const statusCount = f.statuses.length;
  f.source.error();
  f.source.open();
  f.source.snapshot([{ value: 99 }]);
  f.source.change([{ type: "DELETE", data: { value: 1 } }]);
  assert.deepEqual(f.rows(), [{ value: 1 }]);
  assert.deepEqual(f.changes, []);
  assert.equal(f.statuses.length, statusCount);
});
