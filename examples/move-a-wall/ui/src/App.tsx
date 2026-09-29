import { useEffect, useRef, useState, type PointerEvent, type FormEvent } from 'react';
import { useDrasiClient, useDrasiConnectionStatus, useDrasiQuery } from '@drasi/react/react';
import { entity, inputs, key, number, object, queryIds, text, translate, validate, type Entity, type Point, type QueryId, type Shape } from './records';

function useView(id: QueryId) {
  return useDrasiQuery(id, { getKey: row => key(id,row), transform: row => validate(id,row) });
}
export function App() {
  const source = useView('scene-inputs'), context = useView('geometry-context'), obstructions = useView('obstructions');
  const impacts = useView('affected-journeys'), geometry = useView('geometry-status');
  const views = { 'scene-inputs':source, 'geometry-context':context, obstructions, 'affected-journeys':impacts, 'geometry-status':geometry };
  const client = useDrasiClient(), transport = useDrasiConnectionStatus();
  const [online,setOnline] = useState(navigator.onLine);
  useEffect(() => {
    const offline = () => setOnline(false), reconnect = () => { setOnline(true); client.retry(); };
    window.addEventListener('offline',offline); window.addEventListener('online',reconnect);
    return () => { window.removeEventListener('offline',offline); window.removeEventListener('online',reconnect); };
  },[client.retry]);
  const records = (source.data ?? []).flatMap(inputs);
  const clock = records.find(r => r.type === 'clock');
  const revision = clock?.revision ?? 0;
  const entities = records.flatMap(r => r.type === 'entity' ? [r.entity] : []);
  const [selected,setSelected] = useState('wall'), [floor,setFloor] = useState('ground');
  const [inspect,setInspect] = useState<QueryId>('affected-journeys');
  const [draft,setDraft] = useState<Entity | null>(null), [adding,setAdding] = useState(false);
  const [sending,setSending] = useState(false), [accepted,setAccepted] = useState<number | null>(null);
  const [notice,setNotice] = useState(''), [error,setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const current = entities.find(e => e.id === selected);
  const observed = geometry.data?.[0] ? number(geometry.data[0].revision) : 0;
  const stale = !online || !transport.connected || !client.initialized || Object.values(views).some(v => v.loading || v.stale || v.error);
  const unsettled = revision === 0 || observed !== revision || (accepted !== null && observed < accepted);
  const pending = sending || accepted !== null;
  const disabled = stale || pending || unsettled;
  useEffect(() => {
    if (accepted !== null && observed >= accepted && revision >= accepted) {
      setAccepted(null); setDraft(null);
      setNotice(`Input revision ${accepted} processed by the geometry transformer. Views are delivered independently over SSE.`);
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
    setNotice('Submitting input change; no geometric result is predicted in the browser.');
    try {
      const response = await fetch('/commands',{ method:'POST',headers:{'Content-Type':'application/json','X-Wall-Command':'1'},body:JSON.stringify({expected_revision:revision,command}),signal:AbortSignal.timeout(10000) });
      const body = object(await response.json());
      if (!response.ok) throw new Error(typeof body.error === 'string' ? body.error : `Command failed (${response.status})`);
      setAccepted(number(body.accepted_revision));
      setNotice(`Source accepted revision ${String(body.accepted_revision)}. Waiting for query confirmation...`);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure)); setDraft(null);
    } finally { setSending(false); busyRef.current = false; }
  }
  const active = entities.filter(e => e.active && e.floor === floor);
  const causes = impacts.data ?? [], blocks = obstructions.data ?? [];
  const obstructed = new Set(blocks.map(row => text(row.journey_id)));
  const affectedDestinations = new Set(causes.map(row => text(row.destination_id)));
  const selectedCauses = causes.filter(row => [row.cart_id,row.journey_id,row.destination_id,row.obstacle_id].includes(selected));
  const floors = [...new Set(['ground',floor,...entities.map(e => e.floor)])];
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
    setDraft({id,name:`New ${kind}`,floor,active:true,shape}); setAdding(true); setSelected(id);
  }
  return <main>
    <header className="topbar"><a className="brand" href="https://drasi.io">drasi<span>/ examples</span></a>
      <span className={`connection ${stale ? 'bad' : unsettled ? 'waiting' : 'live'}`} role="status">
        {stale ? 'Disconnected / stale' : unsettled ? 'Processing input changes' : 'Live query results'}
      </span>
      <button onClick={() => {
        setAccepted(null); setDraft(null); client.retry();
        setNotice('Reconnecting to authoritative query snapshots. Pending previews are discarded; commands are not resubmitted.');
      }}>Reconnect</button>
    </header>
    <section className="intro"><div><p className="eyebrow">CHANGE PROCESSING, MEET GEOMETRY</p><h1>Move a Wall</h1>
      <p>One changed obstacle. A different computation. The right journeys light up.</p></div>
      <button className="reset" disabled={disabled} onClick={() => void send({action:'reset'})}>Reset floor</button>
    </section>
    <nav className="pipeline" aria-label="Computation graph">
      {['Source','Context query','Geometry transformer','Impact query','SSE / UI'].map((name,i) =>
        <button key={name} className={i === 2 ? 'native' : ''} onClick={() => setInspect((['scene-inputs','geometry-context','obstructions','affected-journeys','geometry-status'] as const)[i])}>
          <span>0{i+1}</span>{name}{i < 4 && <b aria-hidden="true">→</b>}
        </button>)}
    </nav>
    {(error || stale) && <div className="alert" role="alert">
      {error ?? (!online ? 'Browser is offline. Retained results are not current.' : client.error?.message ?? transport.error?.message ?? 'Query connection is not ready. No local fallback is used.')}
      {error && <button onClick={() => setError(null)}>Dismiss</button>}
    </div>}
    <div className="workspace">
      <section className="floor-panel">
        <div className="panel-heading"><div><h2>Planned journeys</h2><p>Drag the movable wall onto Ada's blue path.</p></div>
          <label className="floor-select">Floor <select value={floor} onChange={e => setFloor(e.target.value)}>{floors.map(f => <option key={f}>{f}</option>)}</select></label></div>
        <Scene entities={active} selected={selected} select={select}
          obstructed={obstructed} affectedDestinations={affectedDestinations} disabled={disabled}
          pending={pending} draft={draft} preview={setDraft} commit={e => void send({action:'put',entity:e})}/>
        <div className="legend"><span className="blue-dot"/> Planned path <span className="swatch"/> Radius + clearance <span className="red-dot"/> Query-confirmed obstruction</div>
        <p className="scene-note">24 × 16 metres · Static plans, not moving robots · Touching the clearance boundary counts as blocked</p>
        <div className="journeys">{active.filter(e => e.shape.kind === 'journey').map(j => {
          if (j.shape.kind !== 'journey') return null;
          const cartId = j.shape.cart_id;
          const cart = entities.find(e => e.id === cartId);
          const affected = causes.filter(row => row.journey_id === j.id);
          const blocked = obstructed.has(j.id);
          return <button key={j.id} className={`journey-card ${blocked ? 'blocked' : ''} ${selected === j.id ? 'selected' : ''}`} onClick={() => select(j.id)}>
            <span className="journey-cart">{cart?.name ?? 'Missing cart'}</span><strong>{j.name}</strong>
            <span>{stale || unsettled ? 'Awaiting current queries' : !cart?.active || cart.shape.kind !== 'cart' || cart.floor !== j.floor ? 'No active same-floor cart' : blocked ? `Obstructed by ${affected.map(r => text(r.obstacle)).join(', ') || 'an obstacle (missing or pending metadata)'}` : 'Path unobstructed'}</span>
          </button>;
        })}</div>
      </section>
      <aside>
        <section className="impact-panel"><p className="eyebrow">QUERY: AFFECTED JOURNEYS</p><h2>{stale || unsettled ? 'Awaiting current queries' : causes.length ? `${new Set(causes.map(r => r.journey_id)).size} affected journey${new Set(causes.map(r => r.journey_id)).size === 1 ? '' : 's'}` : blocks.length ? 'Obstruction without task metadata' : 'Nothing in the way'}</h2>
          {stale || unsettled ? <p className="muted">Results are pending or stale; do not interpret this as a clear floor.</p> : null}
          {causes.length ? causes.map(row => <article className="impact" key={text(row.id)}>
            <strong>{text(row.cart)} · {text(row.task)}</strong><p><b>{text(row.obstacle)}</b> obstructs the planned path to <b>{text(row.destination)}</b>.</p>
            <small>Path distance {number(row.distance_m).toFixed(2)} m · Required {number(row.required_m).toFixed(2)} m</small>
          </article>) : <p className="muted">{blocks.length ? 'A geometric cause exists, but its active same-floor cart, destination or cause metadata is missing or has not arrived. Inspect obstructions; this is not a clear path.' : 'Move the wall across a path. The geometry transformer emits an Obstruction; this query joins the cart, task and destination.'}</p>}
        </section>
        <section className="entity-panel"><div className="panel-heading"><h2>Scene objects</h2><span>{entities.length}/64</span></div>
          <div className="entity-list">{entities.map(e => <button className={selected === e.id ? 'selected' : ''} key={e.id} onClick={() => select(e.id)}>
            <span className={`entity-icon ${e.shape.kind}`}/><span>{e.name}<small>{e.shape.kind} · {e.floor}{!e.active && ' · inactive'}</small></span>
          </button>)}</div>
          <div className="add-buttons">{(['obstacle','journey','cart','destination'] as const).map(kind => <button key={kind} disabled={disabled} onClick={() => add(kind)}>+ {kind}</button>)}</div>
        </section>
      </aside>
    </div>
    <div className="details">
      <section className="editor-panel"><p className="eyebrow">{adding ? 'ADD AN INPUT' : 'EDIT AN INPUT'}</p>
        {(adding && draft) || current ? <Editor key={`${selected}/${revision}/${adding}`} value={adding && draft ? draft : current!} adding={adding} disabled={disabled}
          save={e => { setAdding(false); setDraft(e); void send({action:'put',entity:e}); }}
          remove={() => void send({action:'delete',id:selected})}/>
          : <p className="muted">Select an object, or add one. Deleting objects retracts their obstruction records.</p>}
        {selectedCauses.length > 0 && <p className="selection-cause">This object participates in {selectedCauses.length} actual affected-journey row(s).</p>}
      </section>
      <section className="inspector"><div className="panel-heading"><div><p className="eyebrow">REAL RECORDS, NOT A SIMULATION</p><h2>Follow the change</h2></div><span className="revision">Input r{revision} / geometry r{observed}</span></div>
        {clock?.type === 'clock' && <p className="changed">Last command: <b>{clock.command}</b> · Changed: {clock.changed.join(', ') || '(empty bootstrap)'}</p>}
        <div className="query-tabs">{queryIds.map(id => <button key={id} className={inspect === id ? 'selected' : ''} onClick={() => setInspect(id)}>{id}</button>)}</div>
        <p className="query-state">{views[inspect].status}{views[inspect].stale ? ' · stale' : ''} · {views[inspect].data?.length ?? 0} rows <button onClick={() => views[inspect].retry()}>Retry query</button></p>
        {Object.entries(views).filter(([,v]) => v.error).map(([id,v]) => <p role="alert" className="error" key={id}>{id}: {v.error?.message}</p>)}
        <pre data-testid="query-records">{JSON.stringify(inspect === 'scene-inputs' || inspect === 'geometry-context' ? (views[inspect].data ?? []).flatMap(inputs) : views[inspect].data, null,2)}</pre>
      </section>
    </div>
    <footer><span role="status">{notice || 'The source bootstraps a deterministic, unobstructed floor.'}</span>
      <span>Geometry runs inside drasi-server. This is not a robot safety system.</span>
      <a href="/api/v1/instances/move-a-wall/computation" target="_blank" rel="noreferrer">Actual graph API ↗</a>
    </footer>
  </main>;
}

function Scene({entities,selected,select,obstructed,affectedDestinations,disabled,pending,draft,preview,commit}: {
  entities: Entity[]; selected:string; select:(id:string)=>void; obstructed:Set<string>; affectedDestinations:Set<string>;
  disabled:boolean; pending:boolean; draft:Entity|null; preview:(e:Entity|null)=>void; commit:(e:Entity)=>void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{entity:Entity;start:Point;next:Entity}|null>(null);
  function coordinate(e: PointerEvent): Point {
    const matrix = svg.current?.getScreenCTM();
    if (!matrix) throw new Error('Floor coordinates are unavailable');
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
  return <svg className="scene" ref={svg} viewBox="-1 -1 26 18" role="img" aria-label="Warehouse floor with static planned cart paths. Select an obstacle and use arrow keys or edit coordinates below."
    onPointerMove={move} onPointerUp={up} onPointerCancel={() => { drag.current = null; preview(null); }}>
    <defs><pattern id="grid" width="1" height="1" patternUnits="userSpaceOnUse"><path d="M 1 0 L 0 0 0 1" fill="none" stroke="#dce5e5" strokeWidth=".018"/></pattern></defs>
    <rect x="0" y="0" width="24" height="16" rx=".2" fill="#f4f8f6" stroke="#c5d4d4" strokeWidth=".06"/>
    <rect x="0" y="0" width="24" height="16" fill="url(#grid)"/>
    <text x=".5" y=".8" className="floor-label">FLOOR PLAN / METRES</text>
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
        tabIndex={0} role="button" aria-label={`${e.name}; arrow keys move by 0.25 metres`} onPointerDown={event => down(event,e)}
        onKeyDown={event => {
          if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(e.id); return; }
          const offset: Record<string,Point> = {ArrowLeft:[-.25,0],ArrowRight:[.25,0],ArrowUp:[0,-.25],ArrowDown:[0,.25]};
          if (offset[event.key] && !disabled) { event.preventDefault(); select(e.id); const [x,y] = offset[event.key]; const next = translate(e,x,y); preview(next); commit(next); }
        }}>
        {e.shape.kind === 'obstacle' ? <>
          <polygon points={pointText(e.shape.vertices)}/>
          <text x={e.shape.vertices[0][0]+.15} y={e.shape.vertices[0][1]+.55}>{e.name}</text>
          {e.id === 'wall' && <text className="drag-hint" x={e.shape.vertices[0][0]} y={e.shape.vertices[0][1]+1.6}>↕ drag me</text>}
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
      <text x="12" y="15.5" textAnchor="middle">{pending ? 'PENDING QUERY CONFIRMATION' : 'TENTATIVE DRAG PREVIEW'}</text>
    </g>}
  </svg>;
}

function Editor({value,adding,disabled,save,remove}: {value:Entity;adding:boolean;disabled:boolean;save:(e:Entity)=>void;remove:()=>void}) {
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
  return <form onSubmit={submit}><h2>{adding ? `New ${value.shape.kind}` : value.name}</h2><p className="identity">{value.id}</p>
    <fieldset disabled={disabled}><div className="fields">
      <label>Name<input value={draft.name} onChange={e => setDraft({...draft,name:e.target.value})} required maxLength={100}/></label>
      <label>Floor<input value={draft.floor} onChange={e => setDraft({...draft,floor:e.target.value})} required pattern="[A-Za-z0-9_-]+"/></label>
    </div>
    <label className="checkbox"><input type="checkbox" checked={draft.active} onChange={e => setDraft({...draft,active:e.target.checked})}/> Active (context query filter)</label>
    {draft.shape.kind === 'cart' ? <div className="fields">
      <label>Radius (m)<input type="number" min=".05" max="2" step=".05" value={draft.shape.radius} onChange={e => {
        if (draft.shape.kind === 'cart') setDraft({...draft,shape:{...draft.shape,radius:e.target.valueAsNumber}});
      }}/></label>
      <label>Clearance (m)<input type="number" min="0" max="2" step=".05" value={draft.shape.clearance} onChange={e => {
        if (draft.shape.kind === 'cart') setDraft({...draft,shape:{...draft.shape,clearance:e.target.valueAsNumber}});
      }}/></label>
    </div> : <label>{draft.shape.kind === 'obstacle' ? 'Convex polygon vertices' : draft.shape.kind === 'journey' ? 'Planned polyline points' : 'Destination point'} (metres)
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
      {!adding && <button type="button" className="danger" onClick={remove}>Remove object</button>}</div>
    </fieldset>{error && <p className="error" role="alert">{error}</p>}
    <p className="muted">No geometry runs here. Invalid or unsupported shapes are rejected by the source. Arrow keys move obstacles or destinations by 0.25 m.</p>
  </form>;
}
