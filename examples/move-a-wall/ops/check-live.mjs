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
async function rows(query) { return get(`/queries/${query}/results`); }
async function state() {
  const input = await rows('scene-inputs');
  const records = input.flatMap(row => row.objects.map(JSON.parse));
  return {clock:records.find(r => r.type === 'clock'),entities:records.filter(r => r.type === 'entity').map(r => r.entity)};
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
  await until(async () => (await state()).clock?.revision === body.accepted_revision &&
    (await rows('geometry-status')).some(r => r.revision === body.accepted_revision),`input and geometry revision ${body.accepted_revision}`);
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
        if (data) messages.push(JSON.parse(data));
      }
    }
    if (!stop.signal.aborted) throw new Error('The actual SSE stream closed before acceptance completed.');
  } catch (error) { if (!stop.signal.aborted) streamFailure = error; }
})();
try {
  const graph = await get('/computation');
  assert.ok(graph.components.some(c => c.id === 'geometry' && c.role === 'Transformer'),'actual topology must contain the geometry transformer');
  for (const [from,to] of [['scene','geometry-context'],['geometry-context','geometry'],['geometry','affected-journeys']]) {
    assert.ok(graph.relationships.some(r => r.from.component === from && r.to.component === to),`missing actual graph edge ${from} -> ${to}`);
  }
  await command({action:'reset'});
  await until(async () => (await rows('affected-journeys')).length === 0,'clear fixture');
  const baseline = await state();
  const wall = baseline.entities.find(e => e.id === 'wall');
  const moved = {...wall,shape:{kind:'obstacle',vertices:[[10,4.5],[13,4.5],[13,5.5],[10,5.5]]}};
  const revision = await command({action:'put',entity:moved});
  await until(async () => (await rows('affected-journeys')).length === 1,'Ada affected');
  const affected = (await rows('affected-journeys'))[0];
  assert.deepEqual([affected.cart,affected.task,affected.destination,affected.obstacle],['Ada','Deliver packaging','Packing station','Movable wall']);
  assert.equal(affected.id,'journey-ada/wall');
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
  await command({action:'put',entity:{...moved,id:'wall-two',name:'Second barrier'}});
  await until(async () => (await rows('affected-journeys')).length === 2,'multiple causes');
  await command({action:'delete',id:'wall'});
  await until(async () => (await rows('affected-journeys')).length === 1,'one cause remains');
  assert.equal((await rows('affected-journeys'))[0].obstacle,'Second barrier');
  await command({action:'put',entity:{...moved,id:'wall-two',name:'Second barrier',active:false}});
  await until(async () => (await rows('affected-journeys')).length === 0,'inactive obstacle filtered');
  const context = (await rows('geometry-context')).flatMap(row => row.objects.map(JSON.parse));
  assert.ok(!context.some(r => r.entity?.id === 'wall-two'),'upstream CQ must exclude inactive obstacle');
  await command({action:'reset'});
  const path = baseline.entities.find(e => e.id === 'journey-ada');
  await command({action:'put',entity:{...path,shape:{...path.shape,points:[[2,6.5],[21,6.5]]}}});
  await until(async () => (await rows('affected-journeys')).length === 1,'path-only change');
  await command({action:'delete',id:'cart-ada'});
  await until(async () => (await rows('affected-journeys')).length === 0,'cart delete retracts');
  await command({action:'clear'});
  assert.equal((await rows('geometry-status'))[0].objects,0);
  assert.equal((await rows('obstructions')).length,0);
  await command({action:'reset'});
  const impactChanges = () => messages.filter(message => message.queryId === 'affected-journeys').flatMap(message => {
    assert.ok(Array.isArray(message.results),'stock SSE must use the actual untemplated results protocol');
    return message.results;
  });
  await until(async () => {
    const changes = impactChanges();
    return changes.some(change => change.type === 'ADD' && change.data?.id === 'journey-ada/wall' && change.data.obstacle === 'Movable wall') &&
      changes.some(change => change.type === 'UPDATE' && change.before?.id === 'journey-ada/wall' && change.before.obstacle === 'Movable wall' && change.after?.id === 'journey-ada/wall' && change.after.obstacle === 'Renamed wall') &&
      changes.some(change => change.type === 'DELETE' && change.data?.id === 'journey-ada/wall' && change.data.obstacle === 'Renamed wall');
  },'actual stock SSE ADD, stable-key UPDATE and DELETE with enriched rows');
  await mkdir(new URL('../artifacts/',import.meta.url),{recursive:true});
  await writeFile(new URL('../artifacts/sse-changes.json',import.meta.url),JSON.stringify(impactChanges(),null,2));
  console.log('PASS: stock server topology, input revisions, actual context filtering, native geometry, enriched query results, SSE, retract/reset, malformed and stale commands.');
  console.log(`Captured ${events.length} bytes from the actual SSE reaction; fixture restored.`);
} finally { stop.abort(); await consume; if (streamFailure) throw streamFailure; }
