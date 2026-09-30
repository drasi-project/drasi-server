import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import './about.css';

type Component = {
  id: string;
  kind: 'client' | 'native' | 'query' | 'delivery';
  label: string;
  title: string;
  summary: string;
  x: number;
  y: number;
  implementation: string;
  detail: string;
  input: string;
  output: string;
  note: string;
};

const components: Component[] = [
  {
    id: 'browser', kind: 'client', label: 'BROWSER', title: 'React UI',
    summary: 'Edit the scene. See the impact.', x: 115, y: 405,
    implementation: '@drasi/react · headless hooks',
    detail: 'Dragging previews an input, not a computed answer. On release, the UI sends a revision-checked command. Actual continuous-query results drive the floor, affected journeys and inspector.',
    input: 'REST query snapshots and SSE result changes.',
    output: 'POST /commands: put, delete or reset an input.',
    note: 'No browser geometry or local fallback. Pending, stale and disconnected states stay visible.',
  },
  {
    id: 'scene', kind: 'native', label: 'NATIVE SOURCE', title: 'Scene store',
    summary: 'One authoritative set of inputs.', x: 410, y: 210,
    implementation: 'scene · move-a-wall/scene',
    detail: 'A small in-memory store owns carts, journeys, destinations and obstacles. It validates commands, rejects stale revisions and publishes only changed inputs plus an explicit revision/membership manifest.',
    input: 'HTTP commands, including obstacle-only and path-only edits.',
    output: 'out → FloorObject graph-change envelopes; inserts, updates and sparse deletes.',
    note: 'The store is volatile. Startup reconstructs the fixture; it never calculates obstructions.',
  },
  {
    id: 'context', kind: 'query', label: 'CONTINUOUS QUERY', title: 'Active context',
    summary: 'Select the objects that matter.', x: 670, y: 210,
    implementation: 'geometry-context · drasi/continuous-query',
    detail: 'A real Cypher query selects active FloorObject records and collects their payloads. The source manifest travels with the selected inputs, including an empty scene.',
    input: 'in ← scene.out (graph changes).',
    output: 'out → geometry.in (query-row changes), and the shared result outlet.',
    note: 'MATCH (n:FloorObject) WHERE n.active = true RETURN collect(n.payload) AS objects',
  },
  {
    id: 'geometry', kind: 'native', label: 'NATIVE TRANSFORMER', title: 'Geometry',
    summary: 'Compute and diff obstructions.', x: 930, y: 210,
    implementation: 'geometry · move-a-wall/geometry · Rust geo',
    detail: 'The event handler caches coherent inputs, computes polyline-to-convex-polygon distance, and diffs its last output. A circular cart footprint plus clearance is swept along each planned path. Contact counts as obstruction.',
    input: 'in ← geometry-context.out (query-row changes).',
    output: 'out → Obstruction graph changes, cart/journey/destination/obstacle metadata and GeometryStatus.',
    note: 'Numerical tolerance: 1e-7. Stable IDs: journey/obstacle. Moving the obstacle away emits a deletion. No simulation timer.',
  },
  {
    id: 'impact', kind: 'query', label: 'CONTINUOUS QUERY', title: 'Affected journeys',
    summary: 'Turn a cause into useful context.', x: 1190, y: 210,
    implementation: 'affected-journeys · drasi/continuous-query',
    detail: 'Synthetic joins connect each obstruction to its cart, planned journey, destination and obstacle. This query, not the UI, assembles the human-readable impact.',
    input: 'in ← geometry.out (graph changes).',
    output: 'out → query-results.in; named cart, task, destination and cause.',
    note: 'A journey can have several causes. Removing one obstruction cannot erase the others.',
  },
  {
    id: 'inspection', kind: 'query', label: '3 CONTINUOUS QUERIES', title: 'Inspect the change',
    summary: 'Inputs, causes and revision status.', x: 670, y: 405,
    implementation: 'scene-inputs · obstructions · geometry-status',
    detail: 'Three independent queries expose the authoritative scene (including inactive objects), geometric obstruction records, and the last computed revision/counts. The inspector also shows the context and impact queries.',
    input: 'scene-inputs ← scene.out; obstructions and geometry-status ← geometry.out.',
    output: 'Each out → query-results.in. All five query feeds are available to the UI.',
    note: 'Grouped here for clarity. These are query projections, not transformer cache inspection or a transport event log.',
  },
  {
    id: 'results', kind: 'delivery', label: 'CONFIGURED SINK', title: 'Query results outlet',
    summary: 'query-results', x: 1190, y: 405,
    implementation: 'query-results · drasi/query-results-outlet',
    detail: 'This stock sink is explicitly declared as query-results in the Server computation configuration. All five queries connect to its input. It publishes their results into the shared QueryResultsCatalog resource.',
    input: 'in ← out from all five continuous queries.',
    output: 'Catalog publication, used by the query API and result subscriptions. This sink has no graph output port.',
    note: 'The catalog is a graph-owned resource, not another component. The outgoing diagram lines show access through that resource, not additional graph pipes.',
  },
  {
    id: 'api', kind: 'delivery', label: 'SERVER API', title: 'REST API',
    summary: 'Current query snapshots.', x: 410, y: 580,
    implementation: 'Stock Server HTTP API · port 8421',
    detail: 'The React SDK requests current query results through the standard Server API. Initial load and reconnect reconcile the UI with snapshots from the shared result catalog.',
    input: 'HTTP query-results requests; reads the shared catalog.',
    output: '/api/v1/instances/move-a-wall/queries/:id/results',
    note: 'This is the host API configured on port 8421, not an additional ComputationGraph component. Snapshots from different queries are not one atomic UI snapshot.',
  },
  {
    id: 'sse', kind: 'delivery', label: 'CONFIGURED REACTION', title: 'SSE reaction',
    summary: 'wall-ui · live result changes.', x: 670, y: 580,
    implementation: 'wall-ui · drasi-reaction-sse · port 8422',
    detail: 'The Server configuration declares wall-ui as a standard SSE reaction subscribed to all five queries. It streams actual result changes to the React SDK, which updates the query-backed views.',
    input: 'Result subscriptions for all five continuous queries.',
    output: 'Server-sent events on /events.',
    note: 'The five-second heartbeat checks the connection; it does not run geometry. Each query feed is delivered independently.',
  },
];

const connections = [
  { kind: 'transport', path: 'M115 347 V210 H307', label: 'React UI sends input commands to the scene source.' },
  { kind: 'graph', path: 'M513 210 H567', label: 'Scene source sends graph changes to the active-context query.' },
  { kind: 'rows', path: 'M773 210 H827', label: 'Active-context query sends query-row changes to the geometry transformer.' },
  { kind: 'graph', path: 'M1033 210 H1087', label: 'Geometry transformer sends graph changes to the affected-journeys query.' },
  { kind: 'graph', path: 'M410 268 V405 H567', label: 'Scene source feeds the scene-inputs inspection query.' },
  { kind: 'graph', path: 'M930 268 V315 H670 V347', label: 'Geometry transformer feeds the obstructions and geometry-status inspection queries.' },
  { kind: 'rows', path: 'M670 152 V110 H1320 V405 H1293', label: 'Active-context query also feeds the result outlet.' },
  { kind: 'rows', path: 'M1190 268 V347', label: 'Affected-journeys query feeds the result outlet.' },
  { kind: 'rows', path: 'M773 405 H1087', label: 'All three inspection queries feed the result outlet.' },
  { kind: 'rows', path: 'M1190 463 V490 H410 V522', label: 'Published results are available to the REST API through the shared catalog, not a graph output pipe.' },
  { kind: 'rows', path: 'M670 490 V522', label: 'The SSE reaction subscribes to published query results through the shared catalog.' },
  { kind: 'transport', path: 'M307 580 H115 V463', label: 'REST query snapshots return to the React UI.' },
  { kind: 'transport', path: 'M670 638 V660 H75 V463', label: 'The SSE reaction streams query-result changes to the React UI.' },
];

export function Architecture({onClose, scrollOffset = 0}: {onClose: () => void; scrollOffset?: number}) {
  const [active, setActive] = useState<Component | null>(null);
  const [position, setPosition] = useState<CSSProperties>({});
  const triggers = useRef(new Map<string, HTMLButtonElement>());
  const popup = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pinned = useRef(false);
  const popupHovered = useRef(false);
  function cancelClose() { clearTimeout(timer.current); }
  function close() { cancelClose(); pinned.current = false; popupHovered.current = false; setActive(null); }
  function show(component: Component) {
    cancelClose();
    if (active?.id !== component.id) { pinned.current = false; popupHovered.current = false; }
    setActive(component);
  }
  function scheduleClose() {
    cancelClose();
    timer.current = setTimeout(() => {
      if (!pinned.current && !popupHovered.current && document.activeElement !== triggers.current.get(active?.id ?? '')) setActive(null);
    }, 120);
  }
  useEffect(() => {
    function escape(event: KeyboardEvent) {
      if (event.key === 'Escape' && active) {
        event.preventDefault();
        close();
      }
    }
    function outside(event: globalThis.PointerEvent) {
      if (event.target instanceof Element && !event.target.closest('.architecture-node, .architecture-popover')) close();
    }
    document.addEventListener('keydown', escape);
    document.addEventListener('pointerdown', outside);
    return () => {
      cancelClose();
      document.removeEventListener('keydown', escape);
      document.removeEventListener('pointerdown', outside);
    };
  }, [active]);
  useLayoutEffect(() => {
    if (!active) return;
    const id = active.id;
    function place() {
      const anchor = triggers.current.get(id)?.getBoundingClientRect();
      const panel = popup.current?.getBoundingClientRect();
      if (!anchor || !panel) return;
      const margin = 12;
      const left = Math.max(margin, Math.min(anchor.x + anchor.width / 2 - panel.width / 2, window.innerWidth - panel.width - margin));
      const below = anchor.bottom + 10;
      const preferredTop = below + panel.height <= window.innerHeight - margin ? below : anchor.top - panel.height - 10;
      const top = Math.max(margin, Math.min(preferredTop, window.innerHeight - panel.height - margin));
      setPosition({left, top});
    }
    place();
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
    };
  }, [active]);
  return <main className="architecture-page" style={{marginTop: -scrollOffset}}>
    <button type="button" className="architecture-close" aria-label="Close architecture overview" title="Close" onClick={onClose} autoFocus>
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 4 16 16 M16 4 4 16"/></svg>
    </button>
    <section className="architecture-intro" aria-labelledby="architecture-title">
      <div><h1 id="architecture-title">Obstacle Impact</h1>
        <p className="architecture-subtitle" id="architecture-subtitle">Architecture Overview</p></div>
      <p>Move an obstacle. Drasi connects input changes, pluggable geometry and continuous queries
        to explain which planned journeys are affected.</p>
    </section>
    <section className="architecture-board" aria-label="Solution architecture" aria-describedby="architecture-hint">
      <div className="architecture-guide">
        <p id="architecture-hint"><span aria-hidden="true">ⓘ</span> Hover, focus or tap a component to explore.</p>
        <div className="architecture-legend" aria-label="Connection types">
          <span className="graph">Graph changes</span><span className="rows">Query rows</span><span className="transport">HTTP / SSE</span>
        </div>
      </div>
      <div className="architecture-diagram">
        <div className="architecture-server"><strong>DRASI SERVER</strong></div>
        <svg className="architecture-connections" viewBox="0 0 1360 700" aria-hidden="true">
          <defs>{['graph','rows','transport'].map(kind => <marker key={kind} id={`arrow-${kind}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1 L 9 5 L 0 9" className={kind}/>
          </marker>)}</defs>
          {connections.map(edge => <path key={edge.path} d={edge.path} className={edge.kind} markerEnd={`url(#arrow-${edge.kind})`}/>)}
          <text x="985" y="95" className="rows">CONTEXT QUERY RESULTS</text>
          <text x="865" y="391" className="rows">QUERY RESULTS</text>
          <text x="970" y="477" className="rows">VIA SHARED RESULT CATALOG</text>
          <text x="212" y="567" className="transport">SNAPSHOTS</text>
          <text x="375" y="649" className="transport">LIVE RESULT CHANGES</text>
          <text x="137" y="291" className="transport">COMMAND</text>
        </svg>
        {components.map(component => <button key={component.id}
          ref={node => { if (node) triggers.current.set(component.id, node); else triggers.current.delete(component.id); }}
          type="button" className={`architecture-node ${component.kind}`} data-component={component.id}
          style={{left:`${component.x / 1360 * 100}%`,top:`${component.y / 700 * 100}%`}}
          aria-expanded={active?.id === component.id}
          aria-controls={active?.id === component.id ? 'component-details' : undefined}
          aria-describedby={active?.id === component.id ? 'component-description' : undefined}
          onPointerEnter={event => { if (event.pointerType === 'mouse') show(component); }}
          onPointerLeave={scheduleClose} onFocus={() => show(component)} onBlur={scheduleClose}
          onClick={() => { if (active?.id === component.id && pinned.current) close(); else { show(component); pinned.current = true; } }}>
          <span className="architecture-node-label">{component.label}<span aria-hidden="true">↗</span></span>
          <strong>{component.title}</strong>
          <span className="architecture-node-summary">{component.summary}</span>
          <span className="architecture-mobile-input">{component.input}</span>
        </button>)}
      </div>
      <ol className="architecture-sr-only" aria-label="Dataflow connections">{connections.map(edge => <li key={edge.path}>{edge.label}</li>)}</ol>
    </section>
    {active && <div id="component-details" className={`architecture-popover ${active.kind}`} ref={popup} style={position}
      role="tooltip" aria-labelledby="component-title"
      onPointerEnter={() => { popupHovered.current = true; cancelClose(); }}
      onPointerLeave={() => { popupHovered.current = false; scheduleClose(); }}>
      <p className="eyebrow">{active.label}</p>
      <h2 id="component-title">{active.title}</h2>
      <div id="component-description">
        <code>{active.implementation}</code><p>{active.detail}</p>
        <dl><div><dt>IN</dt><dd>{active.input}</dd></div><div><dt>OUT</dt><dd>{active.output}</dd></div></dl>
        <p className="architecture-popover-note">{active.note}</p>
      </div>
      <span className="architecture-dismiss">Esc to dismiss · tap a card to pin / close</span>
    </div>}
  </main>;
}

export function ArchitectureOverlay({onClose}: {onClose: () => void}) {
  const dialog = useRef<HTMLDialogElement>(null);
  // Keep the title aligned even when the demo header is partially scrolled.
  const scrollOffset = useRef(window.scrollY);
  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    const overflow = document.body.style.overflow;
    element.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = overflow;
      if (element.open) element.close();
    };
  }, []);
  return <dialog ref={dialog} id="architecture-overlay" className="architecture-overlay" aria-labelledby="architecture-subtitle"
    onClose={event => { if (!event.currentTarget.open) onClose(); }}
    onKeyDown={event => {
      if (event.key !== 'Tab') return;
      const buttons = event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not([disabled])');
      const first = buttons.item(0), last = buttons.item(buttons.length - 1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }}>
    <Architecture onClose={() => dialog.current?.close()} scrollOffset={scrollOffset.current}/>
  </dialog>;
}
