import { useEffect, useRef, useState, type PointerEvent, type FormEvent } from 'react';
import { useDrasiClient, useDrasiConnectionStatus, useDrasiQuery } from '@drasi/react/react';
import { entity, inputs, key, number, object, queryIds, text, translate, validate, type Entity, type Point, type QueryId, type Shape } from './records';
import { ArchitectureOverlay } from './Architecture';

const inspectionStages: Record<QueryId,{label:string;description:string}> = {
  'scene-inputs': {label:'Source',description:'Current scene inputs before filtering.'},
  'geometry-context': {label:'Context query',description:'Active inputs passed to the geometry transformer.'},
  obstructions: {label:'Geometry transformer',description:'Obstruction records emitted by the geometry transformer.'},
  'affected-journeys': {label:'Impact query',description:'Affected journeys enriched with cart, destination and obstacle names.'},
  'geometry-status': {label:'SSE / UI',description:'Geometry revision and counts delivered to the UI. This is a query result, not an SSE event log.'},
};

function useView(id: QueryId) {
  return useDrasiQuery(id, { getKey: row => key(id,row), transform: row => validate(id,row) });
}
export function App() {
  const source = useView('scene-inputs'), context = useView('geometry-context'), obstructions = useView('obstructions');
  const impacts = useView('affected-journeys'), geometry = useView('geometry-status');
  const views = { 'scene-inputs':source, 'geometry-context':context, obstructions, 'affected-journeys':impacts, 'geometry-status':geometry };
  const client = useDrasiClient(), transport = useDrasiConnectionStatus();
  const [online,setOnline] = useState(navigator.onLine);
  const [infoOpen,setInfoOpen] = useState(false);
  useEffect(() => {
    const offline = () => setOnline(false), reconnect = () => { setOnline(true); client.retry(); };
    window.addEventListener('offline',offline); window.addEventListener('online',reconnect);
    return () => { window.removeEventListener('offline',offline); window.removeEventListener('online',reconnect); };
  },[client.retry]);
  const records = (source.data ?? []).flatMap(inputs);
  const clock = records.find(r => r.type === 'clock');
  const revision = clock?.revision ?? 0;
  const entities = records.flatMap(r => r.type === 'entity' ? [r.entity] : []);
  const [selected,setSelected] = useState('wall');
  const [inspect,setInspect] = useState<QueryId>('affected-journeys');
  const [draft,setDraft] = useState<Entity | null>(null), [adding,setAdding] = useState(false);
  const [sending,setSending] = useState(false), [accepted,setAccepted] = useState<number | null>(null);
  const [error,setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const current = entities.find(e => e.id === selected);
  const observed = geometry.data?.[0] ? number(geometry.data[0].revision) : 0;
  const stale = !online || !transport.connected || !client.initialized || Object.values(views).some(v => v.loading || v.stale || v.error);
  const unsettled = revision === 0 || observed !== revision || (accepted !== null && observed < accepted);
  const pending = sending || accepted !== null;
  const disabled = stale || pending || unsettled;
  useEffect(() => {
    if (accepted !== null && observed >= accepted && revision >= accepted) {
      setAccepted(null); setDraft(null); setAdding(false);
    }
  },[accepted,observed,revision]);
  useEffect(() => {
    if (accepted === null) return;
    const timer = window.setTimeout(() => setError(`Revision ${accepted} was accepted, but query confirmation has not arrived. Results are not current; inspect the graph or reconnect.`),10000);
    return () => clearTimeout(timer);
  },[accepted]);
  async function send(command: unknown) {
    if (busyRef.current) return;
    busyRef.current = true; setSending(true); setError(null);
    try {
      const response = await fetch('/commands',{ method:'POST',headers:{'Content-Type':'application/json','X-Wall-Command':'1'},body:JSON.stringify({expected_revision:revision,command}),signal:AbortSignal.timeout(10000) });
      const body = object(await response.json());
      if (!response.ok) throw new Error(typeof body.error === 'string' ? body.error : `Command failed (${response.status})`);
      setAccepted(number(body.accepted_revision));
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      if (!adding) setDraft(null);
    } finally { setSending(false); busyRef.current = false; }
  }
  const active = entities.filter(e => e.active);
  const entityIds = new Set(entities.map(e => e.id));
  const causes = (impacts.data ?? []).filter(row => entityIds.has(text(row.journey_id)));
  const blocks = obstructions.data ?? [];
  const obstructed = new Set(blocks.map(row => text(row.journey_id)));
  const affectedDestinations = new Set(causes.map(row => text(row.destination_id)));
  const selectedCauses = causes.filter(row => [row.cart_id,row.journey_id,row.destination_id,row.obstacle_id].includes(selected));
  function select(id: string) {
    setSelected(id); setDraft(null); setAdding(false);
  }
  function add(kind: Shape['kind']) {
    const id = `${kind}-${crypto.randomUUID().slice(0,8)}`;
    const shape: Shape = kind === 'obstacle' ? {kind,vertices:[[8,7],[9.5,7],[9.5,8],[8,8]]}
      : kind === 'cart' ? {kind,radius:0.45,clearance:0.2}
      : kind === 'destination' ? {kind,point:[21,7]}
      : {kind,cart_id:entities.find(e => e.shape.kind === 'cart')?.id ?? 'cart-ada',
        destination_id:entities.find(e => e.shape.kind === 'destination')?.id ?? 'packing',points:[[2,7],[21,7]]};
    setDraft({id,name:`New ${kind}`,active:true,shape}); setAdding(true); setSelected(id);
  }
  return <><main>
    <header className="demo-header">
      <div><div className="demo-title"><h1>Obstacle Impact</h1>
        <button type="button" className="info-button" aria-label="Information about this demo" title="Architecture Overview"
          aria-haspopup="dialog" aria-expanded={infoOpen} aria-controls={infoOpen ? 'architecture-overlay' : undefined} onClick={() => setInfoOpen(true)}><span aria-hidden="true">i</span></button>
      </div><p>How obstacles affect planned journeys.</p></div>
      <div className="demo-actions">
        <span className={`connection ${stale ? 'bad' : pending || unsettled ? 'waiting' : 'live'}`} role="status">
          {stale ? 'Disconnected / stale' : pending || unsettled ? 'Processing input changes' : 'Live query results'}
        </span>
        <button onClick={() => {
          setAccepted(null); setDraft(null); setAdding(false); client.retry();
        }}>Reconnect</button>
        <button className="reset" disabled={disabled} onClick={() => void send({action:'reset'})}>Reset scene</button>
      </div>
    </header>
    {(error || stale) && <div className="alert" role="alert">
      {error ?? (!online ? 'Browser is offline. Retained results are not current.' : client.error?.message ?? transport.error?.message ?? 'Query connection is not ready. No local fallback is used.')}
      {error && <button onClick={() => setError(null)}>Dismiss</button>}
    </div>}
    <div className="workspace">
      <div className="scene-column">
        <section className="scene-panel">
          <div className="panel-heading scene-heading">
            <div><h2>Planned journeys</h2><p>Drag an obstacle across a planned path.</p></div>
            <div className="legend" role="group" aria-label="Scene legend">
              <span><i className="blue-dot" aria-hidden="true"/>Planned path</span>
              <span><i className="swatch" aria-hidden="true"/>Radius + clearance</span>
              <span><i className="red-dot" aria-hidden="true"/>Query-confirmed obstruction</span>
            </div>
          </div>
          <Scene entities={active} selected={selected} select={select}
            obstructed={obstructed} affectedDestinations={affectedDestinations} disabled={disabled}
            pending={pending} draft={draft} preview={setDraft} commit={e => void send({action:'put',entity:e})}/>
        </section>
        <section className="inspector" aria-labelledby="inspector-heading"><div className="panel-heading"><h2 id="inspector-heading">Follow the change</h2><span className="revision">Input r{revision} / geometry r{observed}</span></div>
          <p className="muted">Choose a stage to inspect its query results below.</p>
          <nav className="pipeline" aria-label="Computation graph">
            {queryIds.map((id,i) => <button key={id} aria-pressed={inspect === id} aria-controls="query-records" onClick={() => setInspect(id)}>
              <span>{inspectionStages[id].label}</span><code>{id}</code>{i < queryIds.length - 1 && <b aria-hidden="true">→</b>}
            </button>)}
          </nav>
          <p className="muted" id="record-description">{inspectionStages[inspect].description}</p>
          {clock?.type === 'clock' && <p className="changed">Last command: <b>{clock.command}</b> · Changed: {clock.changed.join(', ') || '(empty bootstrap)'}</p>}
          <p className="query-state">{views[inspect].status}{views[inspect].stale ? ' · stale' : ''} · {views[inspect].data?.length ?? 0} rows <button onClick={() => views[inspect].retry()}>Retry query</button></p>
          {Object.entries(views).filter(([,v]) => v.error).map(([id,v]) => <p role="alert" className="error" key={id}>{id}: {v.error?.message}</p>)}
          <pre id="query-records" data-testid="query-records" aria-describedby="record-description">{JSON.stringify(inspect === 'scene-inputs' || inspect === 'geometry-context' ? (views[inspect].data ?? []).flatMap(inputs) : views[inspect].data, null,2)}</pre>
        </section>
      </div>
      <aside>
        <section className="impact-panel"><p className="eyebrow">QUERY: AFFECTED JOURNEYS</p><h2>{stale || unsettled ? 'Awaiting current queries' : causes.length ? `${new Set(causes.map(r => r.journey_id)).size} affected journey${new Set(causes.map(r => r.journey_id)).size === 1 ? '' : 's'}` : blocks.length ? 'Obstruction without task metadata' : 'Nothing in the way'}</h2>
          {stale || unsettled ? <p className="muted">Results are pending or stale; do not interpret this as a clear scene.</p> : null}
          {causes.length ? causes.map(row => <article className="impact" key={text(row.id)}>
            <strong>{text(row.cart)} · {text(row.task)}</strong><p><b>{text(row.obstacle)}</b> obstructs the planned path to <b>{text(row.destination)}</b>.</p>
            <small>Path distance {number(row.distance_m).toFixed(2)} · Required {number(row.required_m).toFixed(2)}</small>
          </article>) : <p className="muted">{blocks.length ? 'A geometric cause exists, but its active cart, destination or cause metadata is missing or has not arrived. Inspect obstructions; this is not a clear path.' : 'Move an obstacle across a path to see the affected cart, task and destination here.'}</p>}
        </section>
        <section className="editor-panel" aria-labelledby="edit-scene-heading">
          <h2 id="edit-scene-heading">Edit scene</h2>
          <p className="muted">Select an object in the scene or choose one below, then edit its properties here.</p>
          <label>Selected object<select value={adding ? '' : current?.id ?? ''} onChange={e => select(e.target.value)}>
            <option value="" disabled>{adding ? 'New object draft' : 'Choose an object'}</option>
            {entities.map(e => <option key={e.id} value={e.id}>{e.name} ({e.shape.kind}){!e.active && ' · inactive'}</option>)}
          </select></label>
          <div className="add-buttons" role="group" aria-label="Add an object"><span>Add</span>
            {(['obstacle','journey','cart','destination'] as const).map(kind => <button key={kind} disabled={disabled} onClick={() => add(kind)}>+ {kind}</button>)}
          </div>
          {(adding && draft) || current ? <Editor key={`${selected}/${revision}/${adding}`} value={adding && draft ? draft : current!} adding={adding} disabled={disabled}
            save={e => { setDraft(e); void send({action:'put',entity:e}); }}
            remove={() => void send({action:'delete',id:selected})}
            cancel={() => select(entities.find(e => e.id === 'wall')?.id ?? entities[0]?.id ?? '')}/>
            : <p className="muted">Choose an object to edit, or use Add to create one.</p>}
          {selectedCauses.length > 0 && <p className="selection-cause">This object participates in {selectedCauses.length} affected-journey result(s).</p>}
        </section>
      </aside>
    </div>
    <footer>
      <a href="/api/v1/instances/move-a-wall/computation" target="_blank" rel="noreferrer">Actual graph API ↗</a>
      <a href="http://127.0.0.1:8421/ui/" target="_blank" rel="noreferrer">Drasi Server Web UI ↗</a>
    </footer>
  </main>{infoOpen && <ArchitectureOverlay onClose={() => setInfoOpen(false)}/>}</>;
}

function Scene({entities,selected,select,obstructed,affectedDestinations,disabled,pending,draft,preview,commit}: {
  entities: Entity[]; selected:string; select:(id:string)=>void; obstructed:Set<string>; affectedDestinations:Set<string>;
  disabled:boolean; pending:boolean; draft:Entity|null; preview:(e:Entity|null)=>void; commit:(e:Entity)=>void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{entity:Entity;start:Point;next:Entity}|null>(null);
  function coordinate(e: PointerEvent): Point {
    const matrix = svg.current?.getScreenCTM();
    if (!matrix) throw new Error('Scene coordinates are unavailable');
    const p = new DOMPoint(e.clientX,e.clientY).matrixTransform(matrix.inverse());
    return [p.x,p.y];
  }
  function down(e: PointerEvent<SVGGElement>, object: Entity) {
    if (e.button !== 0) return;
    select(object.id);
    if (disabled || object.shape.kind === 'cart') return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = {entity:object,start:coordinate(e),next:object};
  }
  function move(e: PointerEvent) {
    if (!drag.current) return;
    const p = coordinate(e), d = drag.current;
    d.next = translate(d.entity,p[0]-d.start[0],p[1]-d.start[1]); preview(d.next);
  }
  function up() {
    if (!drag.current) return;
    const {entity:original,next} = drag.current; drag.current = null;
    if (JSON.stringify(original) !== JSON.stringify(next)) commit(next); else preview(null);
  }
  const pointText = (points: Point[]) => points.map(p => p.join(',')).join(' ');
  return <svg className="scene" ref={svg} viewBox="-1 -1 26 18" role="img" aria-label="Warehouse scene with static planned cart paths. Drag an obstacle, use arrow keys, or select an object to edit its properties."
    onPointerMove={move} onPointerUp={up} onPointerCancel={() => { drag.current = null; preview(null); }}>
    <defs><pattern id="grid" width="1" height="1" patternUnits="userSpaceOnUse"><path d="M 1 0 L 0 0 0 1" fill="none" stroke="#dce5e5" strokeWidth=".018"/></pattern></defs>
    <rect x="0" y="0" width="24" height="16" rx=".2" fill="#f4f8f6" stroke="#c5d4d4" strokeWidth=".06"/>
    <rect x="0" y="0" width="24" height="16" fill="url(#grid)"/>
    <text x=".5" y=".8" className="scene-label">SCENE</text>
    {entities.filter(e => e.shape.kind === 'journey').map(e => {
      if (e.shape.kind !== 'journey') return null;
      const cartId = e.shape.cart_id;
      const cart = entities.find(c => c.id === cartId);
      const radius = cart?.shape.kind === 'cart' ? cart.shape.radius + cart.shape.clearance : 0;
      return <g key={e.id} className={`path ${obstructed.has(e.id) ? 'blocked' : ''}`} onPointerDown={event => down(event,e)}>
        <polyline points={pointText(e.shape.points)} strokeWidth={radius*2} className="clearance"/>
        <polyline points={pointText(e.shape.points)} strokeWidth=".085" className="centerline"/>
        <text x={e.shape.points[0][0]+.1} y={e.shape.points[0][1]-.9} className="path-label">{e.name}</text>
        {cart?.shape.kind === 'cart' && <g className="cart" onPointerDown={event => { event.stopPropagation(); select(cart.id); }}>
          <circle cx={e.shape.points[0][0]} cy={e.shape.points[0][1]} r={cart.shape.radius}/>
          <text x={e.shape.points[0][0]} y={e.shape.points[0][1]+.12}>{cart.name[0]}</text>
          <text className="cart-name" x={e.shape.points[0][0]} y={e.shape.points[0][1]+1.0}>{cart.name}</text>
        </g>}
      </g>;
    })}
    {entities.filter(e => e.shape.kind === 'destination' || e.shape.kind === 'obstacle').map(e =>
      <g key={e.id} data-entity={e.id} className={`scene-object ${e.shape.kind} ${selected === e.id ? 'selected' : ''} ${affectedDestinations.has(e.id) ? 'affected' : ''}`}
        tabIndex={0} role="button" aria-label={`${e.name}; arrow keys move in 0.25 steps`} onPointerDown={event => down(event,e)}
        onKeyDown={event => {
          if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(e.id); return; }
          const offset: Record<string,Point> = {ArrowLeft:[-.25,0],ArrowRight:[.25,0],ArrowUp:[0,-.25],ArrowDown:[0,.25]};
          if (offset[event.key] && !disabled) { event.preventDefault(); select(e.id); const [x,y] = offset[event.key]; const next = translate(e,x,y); preview(next); commit(next); }
        }}>
        {e.shape.kind === 'obstacle' ? <>
          <polygon points={pointText(e.shape.vertices)}/>
          <text x={e.shape.vertices[0][0]+.15} y={e.shape.vertices[0][1]+.55}>{e.name}</text>
          {selected === e.id && <text className="drag-hint" x={e.shape.vertices[0][0]} y={e.shape.vertices[0][1]+1.6}>↕ drag to move</text>}
        </> : e.shape.kind === 'destination' ? <>
          <rect x={e.shape.point[0]-.55} y={e.shape.point[1]-.55} width="1.1" height="1.1" rx=".15"/>
          <text x={e.shape.point[0]+.85} y={e.shape.point[1]-.1}>{e.name.split(' ')[0]}</text>
          <text x={e.shape.point[0]+.85} y={e.shape.point[1]+.4}>{e.name.split(' ').slice(1).join(' ')}</text>
        </> : null}
      </g>)}
    {draft && <g className="tentative" aria-label="Tentative change, not a geometry result">
      {draft.shape.kind === 'obstacle' && <polygon points={pointText(draft.shape.vertices)}/>}
      {draft.shape.kind === 'journey' && <polyline points={pointText(draft.shape.points)}/>}
      {draft.shape.kind === 'destination' && <circle cx={draft.shape.point[0]} cy={draft.shape.point[1]} r=".55"/>}
      <text x="12" y="15.5" textAnchor="middle">{pending ? 'PENDING QUERY CONFIRMATION' : 'TENTATIVE INPUT PREVIEW'}</text>
    </g>}
  </svg>;
}

function Editor({value,adding,disabled,save,remove,cancel}: {value:Entity;adding:boolean;disabled:boolean;save:(e:Entity)=>void;remove:()=>void;cancel:()=>void}) {
  const [draft,setDraft] = useState(value), [coordinates,setCoordinates] = useState(JSON.stringify(
    value.shape.kind === 'journey' ? value.shape.points : value.shape.kind === 'obstacle' ? value.shape.vertices : value.shape.kind === 'destination' ? value.shape.point : []));
  const [error,setError] = useState<string|null>(null);
  function submit(event:FormEvent) {
    event.preventDefault(); setError(null);
    try {
      let shape = draft.shape;
      const parsed:unknown = JSON.parse(coordinates);
      if (shape.kind === 'obstacle') shape = entity({...draft,shape:{...shape,vertices:parsed}}).shape;
      if (shape.kind === 'journey') shape = entity({...draft,shape:{...shape,points:parsed}}).shape;
      if (shape.kind === 'destination') shape = entity({...draft,shape:{...shape,point:parsed}}).shape;
      save({...draft,shape});
    } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
  }
  return <form onSubmit={submit}><h3>{adding ? `New ${value.shape.kind}` : value.name}</h3><p className="identity">{value.id}</p>
    <fieldset disabled={disabled}>
      <label>Name<input value={draft.name} onChange={e => setDraft({...draft,name:e.target.value})} required maxLength={100}/></label>
    <label className="checkbox"><input type="checkbox" checked={draft.active} onChange={e => setDraft({...draft,active:e.target.checked})}/> Active (context query filter)</label>
    {draft.shape.kind === 'cart' ? <div className="fields">
      <label>Radius<input type="number" min=".05" max="2" step=".05" value={draft.shape.radius} onChange={e => {
        if (draft.shape.kind === 'cart') setDraft({...draft,shape:{...draft.shape,radius:e.target.valueAsNumber}});
      }}/></label>
      <label>Clearance<input type="number" min="0" max="2" step=".05" value={draft.shape.clearance} onChange={e => {
        if (draft.shape.kind === 'cart') setDraft({...draft,shape:{...draft.shape,clearance:e.target.valueAsNumber}});
      }}/></label>
    </div> : <label>{draft.shape.kind === 'obstacle' ? 'Convex polygon vertices' : draft.shape.kind === 'journey' ? 'Planned polyline points' : 'Destination point'}
      <textarea aria-label="Coordinates" value={coordinates} onChange={e => setCoordinates(e.target.value)} rows={3} spellCheck={false}/>
    </label>}
    {draft.shape.kind === 'journey' && <div className="fields">
      <label>Cart ID<input value={draft.shape.cart_id} onChange={e => {
        if (draft.shape.kind === 'journey') setDraft({...draft,shape:{...draft.shape,cart_id:e.target.value}});
      }}/></label>
      <label>Destination ID<input value={draft.shape.destination_id} onChange={e => {
        if (draft.shape.kind === 'journey') setDraft({...draft,shape:{...draft.shape,destination_id:e.target.value}});
      }}/></label>
    </div>}
    <div className="form-actions"><button type="submit" className="primary">{adding ? 'Add object' : 'Apply input change'}</button>
      {adding ? <button type="button" onClick={cancel}>Cancel</button> : <button type="button" className="danger" onClick={remove}>Remove object</button>}</div>
    </fieldset>{error && <p className="error" role="alert">{error}</p>}
    <p className="muted">Edits go through the source. Arrow keys nudge obstacles or destinations.</p>
  </form>;
}
