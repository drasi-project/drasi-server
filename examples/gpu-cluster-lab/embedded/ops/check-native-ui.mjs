import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { queryIds, rowKey, validateRow } from '../../shared/ui/src/rows.ts';

assert.ok(process.argv[2], 'Usage: node --experimental-strip-types ops/check-native-ui.mjs <native-output.json>');
const batches = JSON.parse(await readFile(process.argv[2], 'utf8'));
assert.ok(Array.isArray(batches) && batches.length > 0, 'Native output must contain projection batches');
const expected = new Set(queryIds);
let count = 0;
for (const { query, rows } of batches) {
  assert.ok(expected.has(query), `Unexpected native projection ${query}`);
  assert.ok(Array.isArray(rows) && rows.length > 0, `${query} must contain actual CQ rows`);
  const keys = new Set();
  for (const row of rows) {
    validateRow(query, row);
    const key = rowKey(query, row);
    assert.ok(!keys.has(key), `Duplicate ${query} identity: ${key}`);
    keys.add(key);
    count++;
  }
}
assert.deepEqual(new Set(batches.map(batch => batch.query)), expected);
console.log(`Validated ${count} real source/native/CQ rows across ${batches.length} snapshots against all ${expected.size} implemented UI projection contracts.`);
