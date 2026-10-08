import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { connections, type Connection } from './architectureFlows';
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
    detail: 'Dragging previews an input, not a computed answer. On release, the UI sends a revision-checked command. Actual continuous-query results drive the scene, affected journeys and inspector.',
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
    output: 'out → SceneObject graph-change envelopes; inserts, updates and sparse deletes.',
    note: 'The store is volatile. Startup reconstructs the fixture; it never calculates obstructions.',
  },
  {
    id: 'context', kind: 'query', label: 'CONTINUOUS QUERY', title: 'Active context',
    summary: 'Select the objects that matter.', x: 670, y: 210,
    implementation: 'geometry-context · drasi/continuous-query',
    detail: 'A real Cypher query selects active SceneObject records and collects their payloads. The source manifest travels with the selected inputs, including an empty scene.',
    input: 'in ← scene.out (graph changes).',
    output: 'out → geometry.in (query-row changes), and the shared result outlet.',
    note: 'MATCH (n:SceneObject) WHERE n.active = true RETURN collect(n.payload) AS objects',
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
    output: 'Each out → query-results.in and wall-ui.in. All five query feeds are available to the UI.',
    note: 'Grouped here for clarity. These are query projections, not transformer cache inspection or a transport event log.',
  },
  {
    id: 'results', kind: 'delivery', label: 'CONFIGURED SINK', title: 'Query results outlet',
    summary: 'query-results', x: 1190, y: 405,
    implementation: 'query-results · drasi/query-results-outlet',
    detail: 'This stock sink is explicitly declared as query-results in the Server computation configuration. All five queries connect to its input. It publishes their results into the shared QueryResultsCatalog resource.',
    input: 'in ← out from all five continuous queries.',
    output: 'Catalog publication, used by the query snapshot API. This sink has no graph output port.',
    note: 'The catalog is a graph-owned resource, not another component. SSE receives independent direct query-output edges, not catalog subscriptions.',
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
    id: 'sse', kind: 'delivery', label: 'NATIVE SINK', title: 'SSE sink',
    summary: 'wall-ui · live result changes.', x: 670, y: 580,
    implementation: 'wall-ui · drasi.network/sse-sink · port 8422',
    detail: 'The Server graph connects all five query outputs directly to wall-ui. The native sink emits the existing browser protocol without a legacy reaction or subscription worker.',
    input: 'in ← out from all five continuous queries through bounded graph pipes.',
    output: 'Server-sent events on /events.',
    note: 'The five-second heartbeat checks the connection; it does not run geometry. Delivery is volatile and acceptance-only; reconnect reads fresh query snapshots.',
  },
];

type Selection = {
  detail: Component | Connection;
  trigger: Element;
  point?: { x: number; y: number };
};

export function Architecture({onClose, scrollOffset = 0}: {onClose: () => void; scrollOffset?: number}) {
  const [selection, setSelection] = useState<Selection | null>(null);
  const active = selection?.detail;
  const [position, setPosition] = useState<CSSProperties>({});
  const popup = useRef<HTMLDivElement>(null);
  function close() { setSelection(null); }
  function toggle(detail: Component | Connection, trigger: Element, pointer?: {clientX: number; clientY: number}) {
    if (selection) { close(); return; }
    const bounds = trigger.getBoundingClientRect();
    setSelection({detail, trigger, point: pointer ? {
      x: (pointer.clientX - bounds.x) / (bounds.width || 1),
      y: (pointer.clientY - bounds.y) / (bounds.height || 1),
    } : undefined});
  }
  useEffect(() => {
    if (!active) return;
    function escape(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.preventDefault();
        close();
      }
    }
    function dismiss(event: MouseEvent) {
      // Trigger clicks are handled by toggle, including the click that opens details.
      if (event.target instanceof Element && event.target.closest('.architecture-node, .architecture-edge, .architecture-flow-link')) return;
      close();
    }
    document.addEventListener('keydown', escape);
    document.addEventListener('click', dismiss);
    return () => {
      document.removeEventListener('keydown', escape);
      document.removeEventListener('click', dismiss);
    };
  }, [active]);
  useLayoutEffect(() => {
    if (!selection) return;
    function place() {
      const anchor = selection?.trigger.getBoundingClientRect();
      const panel = popup.current?.getBoundingClientRect();
      if (!anchor || !panel) return;
      const margin = 12;
      const x = anchor.x + anchor.width * (selection?.point?.x ?? .5);
      const y = selection?.point ? anchor.y + anchor.height * selection.point.y : undefined;
      let left = Math.max(margin, Math.min(x - panel.width / 2, window.innerWidth - panel.width - margin));
      const below = (y ?? anchor.bottom) + 10;
      const preferredTop = below + panel.height <= window.innerHeight - margin ? below : (y ?? anchor.top) - panel.height - 10;
      if (below + panel.height > window.innerHeight - margin && preferredTop < margin) {
        if (x + 10 + panel.width <= window.innerWidth - margin) left = x + 10;
        else if (x - 10 - panel.width >= margin) left = x - 10 - panel.width;
      }
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
  }, [selection]);
  return <main className="architecture-page" style={{marginTop: -scrollOffset}}>
    <button type="button" className="architecture-close" aria-label="Close architecture overview" title="Close" onClick={onClose} autoFocus>
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 4 16 16 M16 4 4 16"/></svg>
    </button>
    <section className="architecture-intro" aria-labelledby="architecture-title">
      <div><h1 id="architecture-title">Obstacle Impact</h1>
        <p className="architecture-subtitle" id="architecture-subtitle">Architecture Overview</p></div>
      <p>Move an obstacle and Drasi updates which planned journeys are affected. The geometry transformer
        uses the geo library to measure <strong>Euclidean distance</strong>: the shortest gap between
        a planned path and an obstacle. The gap is zero if they touch or overlap. Comparing it with
        the cart's radius plus clearance checks whether the whole cart can pass, even when its
        centreline misses the obstacle.</p>
    </section>
    <section className="architecture-board" aria-label="Solution architecture" aria-describedby="architecture-hint">
      <div className="architecture-guide">
        <p id="architecture-hint"><span aria-hidden="true">ⓘ</span> Click a component or arrow for details. Click anywhere or press Esc to dismiss.</p>
        <div className="architecture-legend" aria-label="Connection types">
          <span className="graph">Graph changes</span><span className="rows">Query rows</span><span className="transport">HTTP / SSE</span>
        </div>
      </div>
      <div className="architecture-diagram">
        <div className="architecture-server"><strong>DRASI SERVER</strong></div>
        <svg className="architecture-connections" viewBox="0 0 1360 700" role="group" aria-label="Dataflow connection schemas">
          <defs>{['graph','rows','transport'].map(kind => <marker key={kind} id={`arrow-${kind}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1 L 9 5 L 0 9" className={kind}/>
          </marker>)}</defs>
          {connections.map(edge => <g key={edge.id} className="architecture-edge" data-flow={edge.id} data-active={active?.id === edge.id}
            onClick={event => toggle(edge, event.currentTarget, event.detail > 0 ? event : undefined)}>
            <path d={edge.path} className={`architecture-edge-line ${edge.kind}`} markerEnd={`url(#arrow-${edge.kind})`} aria-hidden="true"/>
            <path d={edge.path} className="architecture-edge-target" role="button" tabIndex={0}
              aria-label={`Schema: ${edge.title}`} aria-expanded={active?.id === edge.id}
              aria-controls={active?.id === edge.id ? 'component-details' : undefined}
              aria-describedby={active?.id === edge.id ? 'component-description' : undefined}
              onKeyDown={event => {
                if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); toggle(edge, event.currentTarget); }
              }}/>
          </g>)}
          <g aria-hidden="true">
          <text x="985" y="95" className="rows">CONTEXT QUERY RESULTS</text>
          <text x="865" y="391" className="rows">QUERY RESULTS</text>
          <text x="970" y="477" className="rows">VIA SHARED RESULT CATALOG</text>
          <text x="212" y="567" className="transport">SNAPSHOTS</text>
          <text x="375" y="649" className="transport">LIVE RESULT CHANGES</text>
          <text x="137" y="291" className="transport">COMMAND</text>
          </g>
        </svg>
        {components.map(component => <button key={component.id}
          type="button" className={`architecture-node ${component.kind}`} data-component={component.id}
          style={{left:`${component.x / 1360 * 100}%`,top:`${component.y / 700 * 100}%`}}
          aria-expanded={active?.id === component.id}
          aria-controls={active?.id === component.id ? 'component-details' : undefined}
          aria-describedby={active?.id === component.id ? 'component-description' : undefined}
          onClick={event => toggle(component, event.currentTarget)}>
          <span className="architecture-node-label">{component.label}<span aria-hidden="true">↗</span></span>
          <strong>{component.title}</strong>
          <span className="architecture-node-summary">{component.summary}</span>
          <span className="architecture-mobile-input">{component.input}</span>
        </button>)}
      </div>
      <details className="architecture-flow-list">
        <summary>Connection schemas</summary>
        <ul>{connections.map(edge => <li key={edge.id}>
          <button type="button" className="architecture-flow-link" data-flow={edge.id}
            aria-label={`Schema: ${edge.title}`} aria-expanded={active?.id === edge.id}
            aria-controls={active?.id === edge.id ? 'component-details' : undefined}
            aria-describedby={active?.id === edge.id ? 'component-description' : undefined}
            onClick={event => toggle(edge, event.currentTarget)}>{edge.title}</button>
        </li>)}</ul>
      </details>
    </section>
    {active && <div id="component-details" className={`architecture-popover ${active.kind}${'schema' in active ? ' schema' : ''}`} ref={popup} style={position}
      role="tooltip" aria-labelledby="component-title">
      <p className="eyebrow">{'schema' in active ? 'DATA SCHEMA' : active.label}</p>
      <h2 id="component-title">{active.title}</h2>
      <div id="component-description">
        <code>{active.implementation}</code><p>{active.detail}</p>
        {'schema' in active ? <><p className="architecture-schema-label">Readable schema · not live data</p>
          <pre className="architecture-schema"><code>{active.schema}</code></pre></>
          : <dl><div><dt>IN</dt><dd>{active.input}</dd></div><div><dt>OUT</dt><dd>{active.output}</dd></div></dl>}
        <p className="architecture-popover-note">{active.note}</p>
      </div>
      <span className="architecture-dismiss">Click anywhere or press Esc to dismiss</span>
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
      const controls = [...event.currentTarget.querySelectorAll<HTMLElement | SVGElement>('button:not([disabled]), [role="button"][tabindex="0"], summary')]
        .filter(element => element.getClientRects().length > 0);
      const first = controls[0], last = controls.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }}>
    <Architecture onClose={() => dialog.current?.close()} scrollOffset={scrollOffset.current}/>
  </dialog>;
}
