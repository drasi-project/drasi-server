import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
const api = 'http://127.0.0.1:8421/api/v1/instances/move-a-wall';
let streamFailure;
async function get(path) {
  const response = await fetch(`${api}${path}`,{signal:AbortSignal.timeout(5000)});
  const body = await response.json();
  assert.ok(response.ok && body.success,JSON.stringify(body));
  return body.data;
}
function assertSceneFields(value) {
  if (Array.isArray(value)) { value.forEach(assertSceneFields); return; }
  if (!value || typeof value !== 'object') return;
  assert.equal(Object.hasOwn(value,'floor'),false,'Query and SSE records must not retain the removed field');
  assert.equal(Object.hasOwn(value,'payload'),false,'Scene records must be plain typed properties');
  for (const child of Object.values(value)) assertSceneFields(child);
}
async function rows(query) {
  const result = await get(`/queries/${query}/results`);
  assertSceneFields(result);
  return result;
}
async function assertNativeSnapshot(query) {
  const expected = await rows(query);
  const response = await fetch(`http://127.0.0.1:8422/snapshot/${query}`,{signal:AbortSignal.timeout(5000)});
  assert.equal(response.status,200,`native snapshot for ${query}`);
  assert.match(response.headers.get('content-type') ?? '',/application\/json/);
  assert.match(response.headers.get('x-drasi-output-epoch') ?? '',/^[0-9a-f-]{36}$/,`${query}: output epoch`);
  assert.match(response.headers.get('x-drasi-output-position') ?? '',/^\d+$/,`${query}: output position`);
  const changes = await response.json();
  assert.ok(Array.isArray(changes),'native snapshots must stream a JSON array of ADD changes');
  assert.ok(changes.every(change => change.operation === 'ADD' && change.identity?.id && !('row_signature' in change)),'snapshot rows are canonical ADD changes with identity');
  const actual = changes.map(change => change.after);
  assertSceneFields(actual);
  const ordered = values => values.map(value => JSON.stringify(value)).sort();
  assert.deepEqual(ordered(actual),ordered(expected),`${query}: SSE and Server must read the same query catalogue`);
}
const shapeFields = {cart:['radius','clearance'],journey:['cart_id','destination_id','points'],destination:['point'],obstacle:['vertices']};
function entity(row) {
  const fields = shapeFields[row.kind];
  assert.ok(fields,`unknown scene kind ${row.kind}`);
  return {id:row.id,name:row.name,active:row.active,shape:{kind:row.kind,...Object.fromEntries(fields.map(key => [key,row[key]]))}};
}
async function state() {
  const [clock] = await rows('scene-status');
  return {clock,entities:(await rows('scene-inputs')).map(entity)};
}
async function until(predicate,description) {
  for (let n=0;n<100;n++) {
    if (streamFailure) throw streamFailure;
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve,50));
  }
  throw new Error(`Timed out: ${description}`);
}
async function command(command) {
  const {clock} = await state();
  const response = await fetch('http://127.0.0.1:8423/commands',{
    method:'POST',headers:{'Content-Type':'application/json','X-Wall-Command':'1'},
    body:JSON.stringify({expected_revision:clock.revision,command}),
  });
  const body = await response.json();
  assert.ok(response.ok,JSON.stringify(body));
  await until(async () => {
    const {clock,entities} = await state();
    return clock?.revision === body.accepted_revision && entities.length === clock.objects;
  },`scene revision ${body.accepted_revision}`);
  return body.accepted_revision;
}
const stop = new AbortController();
const response = await fetch('http://127.0.0.1:8422/events',{signal:stop.signal});
assert.equal(response.status,200);
assert.match(response.headers.get('content-type'),/text\/event-stream/);
let events = '';
let pending = '';
const messages = [];
const decoder = new TextDecoder();
const consume = (async () => {
  try {
    for await (const chunk of response.body) {
      const text = decoder.decode(chunk,{stream:true});
      events += text;
      pending += text;
      let end;
      while ((end = pending.indexOf('\n\n')) >= 0) {
        const frame = pending.slice(0,end);
        pending = pending.slice(end+2);
        const data = frame.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
        if (data) {
          const message = JSON.parse(data);
          assertSceneFields(message);
          messages.push(message);
        }
      }
    }
    if (!stop.signal.aborted) throw new Error('The actual SSE stream closed before acceptance completed.');
  } catch (error) { if (!stop.signal.aborted) streamFailure = error; }
})();
try {
  const graph = await get('/computation');
  // Components have UUIDs; relationships refer to them by ID, people by name.
  const id = Object.fromEntries(graph.components.map(c => [c.name, c.id]));
  const edge = (from,to) => graph.relationships.some(r => r.from.component === id[from] && r.to.component === id[to]);
  assert.ok(graph.components.some(c => c.name === 'geometry' && c.role === 'Transformer' &&
    c.implementation?.name === 'drasi.standard/geometry'),'actual topology must contain the standard geometry transformer');
  assert.ok(graph.components.some(c => c.name === 'wall-ui' && c.role === 'Sink' &&
    c.implementation?.name === 'drasi.network/sse-sink'),'UI delivery must be the native SSE sink');
  for (const [from,to] of [['scene','journey-footprints'],['scene','obstacle-footprints'],['journey-footprints','geometry'],
    ['obstacle-footprints','geometry'],['geometry','obstructions'],['geometry','affected-journeys'],['scene','affected-journeys'],['scene','scene-status']]) {
    assert.ok(edge(from,to),`missing actual graph edge ${from} -> ${to}`);
  }
  for (const query of ['scene-inputs','journey-footprints','obstacle-footprints','affected-journeys','obstructions','scene-status']) {
    assert.ok(edge(query,'wall-ui'),
      `missing direct native SSE edge ${query} -> wall-ui`);
  }
  await command({action:'reset'});
  await until(async () => (await rows('affected-journeys')).length === 0,'clear fixture');
  await assertNativeSnapshot('affected-journeys');
  assert.equal((await fetch('http://127.0.0.1:8422/snapshot/foreign',{signal:AbortSignal.timeout(5000)})).status,404);
  const baseline = await state();
  const wall = baseline.entities.find(e => e.id === 'wall');
  const moved = {...wall,shape:{kind:'obstacle',vertices:[[10,4.5],[13,4.5],[13,5.5],[10,5.5]]}};
  const revision = await command({action:'put',entity:moved});
  await until(async () => (await rows('affected-journeys')).length === 1,'Ada affected');
  const affected = (await rows('affected-journeys'))[0];
  assert.deepEqual([affected.cart,affected.task,affected.destination,affected.obstacle],['Ada','Deliver packaging','Packing station','Movable wall']);
  assert.equal(affected.id,'journey-ada/wall');
  await assertNativeSnapshot('affected-journeys');
  const noOp = await command({action:'put',entity:moved});
  assert.equal(noOp,revision,'identical input must not advance revision');
  const bad = await fetch('http://127.0.0.1:8423/commands',{method:'POST',headers:{'Content-Type':'application/json','X-Wall-Command':'1'},
    body:JSON.stringify({expected_revision:revision-1,command:{action:'delete',id:'wall'}})});
  assert.equal(bad.status,409,'obsolete commands must be rejected');
  const malformed = await fetch('http://127.0.0.1:8423/commands',{method:'POST',headers:{'Content-Type':'application/json','X-Wall-Command':'1'},
    body:JSON.stringify({expected_revision:revision,command:{action:'put',entity:{...moved,shape:{kind:'obstacle',vertices:[[0,0],[2,2],[2,0],[0,2]]}}}})});
  assert.equal(malformed.status,409,'unsupported self-intersecting polygon must be rejected');
  await command({action:'put',entity:{...moved,name:'Renamed wall'}});
  await until(async () => (await rows('affected-journeys')).some(r => r.id === 'journey-ada/wall' && r.obstacle === 'Renamed wall'),'same-key impact update');
  await assertNativeSnapshot('affected-journeys');
  await command({action:'put',entity:{...moved,id:'wall-two',name:'Second barrier'}});
  await until(async () => (await rows('affected-journeys')).length === 2,'multiple causes');
  await command({action:'delete',id:'wall'});
  await until(async () => (await rows('affected-journeys')).length === 1,'one cause remains');
  assert.equal((await rows('affected-journeys'))[0].obstacle,'Second barrier');
  await assertNativeSnapshot('affected-journeys');
  await command({action:'put',entity:{...moved,id:'wall-two',name:'Second barrier',active:false}});
  await until(async () => (await rows('affected-journeys')).length === 0,'inactive obstacle filtered');
  await assertNativeSnapshot('affected-journeys');
  const footprints = await rows('obstacle-footprints');
  assert.ok(footprints.some(r => r.obstacle_id === 'pallet') && !footprints.some(r => r.obstacle_id === 'wall-two'),'upstream CQ must exclude inactive obstacle');
  await command({action:'reset'});
  const path = baseline.entities.find(e => e.id === 'journey-ada');
  await command({action:'put',entity:{...path,shape:{...path.shape,points:[[2,6.5],[21,6.5]]}}});
  await until(async () => (await rows('affected-journeys')).length === 1,'path-only change');
  await command({action:'delete',id:'cart-ada'});
  await until(async () => (await rows('affected-journeys')).length === 0,'cart delete retracts');
  await command({action:'clear'});
  assert.equal((await rows('scene-status'))[0].objects,0);
  await until(async () => (await rows('obstructions')).length === 0 && (await rows('journey-footprints')).length === 0,'clear retracts every obstruction');
  await command({action:'reset'});
  const impactChanges = () => messages.filter(message =>
    message.output?.alias === 'affected-journeys' && message.output.component === id['affected-journeys'] && message.output.port === 'out').flatMap(message => {
    assert.ok(Array.isArray(message.changes),'native SSE must use the canonical graph-change protocol');
    assert.ok(!('queryId' in message),'native output identity must not masquerade as a query ID');
    return message.changes;
  });
  await until(async () => {
    const changes = impactChanges();
    const added = changes.find(change => change.operation === 'ADD' && change.after?.id === 'journey-ada/wall' && change.after.obstacle === 'Movable wall');
    const replaced = changes.find(change => change.operation === 'REPLACE' && change.before?.id === 'journey-ada/wall' && change.before.obstacle === 'Movable wall' && change.after?.id === 'journey-ada/wall' && change.after.obstacle === 'Renamed wall');
    const deleted = changes.find(change => change.operation === 'DELETE' && change.before?.id === 'journey-ada/wall' && change.before.obstacle === 'Renamed wall');
    if (!added || !replaced || !deleted) return false;
    assert.ok(added.identity?.id,'result nodes must carry their canonical identity');
    assert.deepEqual(replaced.identity,added.identity,'replacement must keep the result identity');
    assert.deepEqual(deleted.identity,added.identity,'deletion must name that same result identity');
    return true;
  },'actual native SSE ADD, stable-key REPLACE and DELETE with enriched rows');
  for (const query of ['scene-inputs','journey-footprints','obstacle-footprints','affected-journeys','obstructions','scene-status']) {
    await assertNativeSnapshot(query);
  }
  await mkdir(new URL('../artifacts/',import.meta.url),{recursive:true});
  await writeFile(new URL('../artifacts/sse-changes.json',import.meta.url),JSON.stringify(impactChanges(),null,2));
  console.log('PASS: stock server topology, input revisions, actual footprint filtering, drasi.standard/geometry, enriched query results, SSE, scoped native snapshots, retract/reset, malformed and stale commands.');
  console.log(`Captured ${events.length} bytes from the actual SSE reaction; fixture restored.`);
} finally { stop.abort(); await consume; if (streamFailure) throw streamFailure; }
