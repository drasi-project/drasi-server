import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { architectureEdges, architectureNodes, type ArchitectureEdge, type ArchitectureNode } from './architectureData';
import './architecture.css';

type Selection = { detail: ArchitectureNode | ArchitectureEdge; trigger: HTMLElement | SVGElement };

export function ArchitectureOverlay({ onClose, returnFocus }: { onClose: () => void; returnFocus: RefObject<HTMLButtonElement> }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const scrollOffset = useRef(window.scrollY);
  const [selection, setSelection] = useState<Selection | null>(null);
  const active = selection?.detail;
  useLayoutEffect(() => {
    const element = dialog.current;
    const trigger = returnFocus.current;
    const overflow = document.body.style.overflow;
    element?.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      element?.close();
      document.body.style.overflow = overflow;
      if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus({ preventScroll: true });
    };
  }, [returnFocus]);
  useLayoutEffect(() => {
    if (selection) heading.current?.focus({ preventScroll: true });
  }, [selection]);
  function closeDetails() {
    setSelection(null);
    selection?.trigger.focus({ preventScroll: true });
  }
  function select(detail: ArchitectureNode | ArchitectureEdge, trigger: HTMLElement | SVGElement) {
    if (active?.id === detail.id) closeDetails();
    else setSelection({ detail, trigger });
  }
  function escape() {
    if (selection) closeDetails();
    else onClose();
  }
  return <dialog ref={dialog} id="gpu-architecture-overlay" className="gpu-architecture-overlay" aria-labelledby="gpu-architecture-title"
    onCancel={event => { event.preventDefault(); escape(); }}
    onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); escape(); }
      if (event.key !== 'Tab') return;
      const controls = [...event.currentTarget.querySelectorAll<HTMLElement | SVGElement>(
        'button:not([disabled]), [role="button"][tabindex="0"], summary',
      )].filter(element => {
        const closedDetails = element.closest('details:not([open])');
        return element.getClientRects().length > 0 && (!closedDetails || closedDetails.firstElementChild === element);
      });
      const first = controls[0], last = controls.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }}>
    <div className="lab-page gpu-architecture-page" style={{ marginTop: -scrollOffset.current }}>
      <header className="lab-header">
        <div className="lab-title"><h1 id="gpu-architecture-title">GPU Cluster Lab</h1></div>
      </header>
      <button type="button" className="gpu-architecture-close" aria-label="Close scenario and architecture" title="Close" onClick={onClose} autoFocus>
        <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 4 16 16 M16 4 4 16"/></svg>
      </button>
      <section className="gpu-architecture-intro">
        <p className="gpu-architecture-subtitle">Managing AI workloads across a GPU cluster</p>
        <p>GPU Cluster Lab demonstrates managing the allocation of AI workloads across a simulated cluster of GPUs.
          Assistant, chat, embedding and reranking services share GPU memory and compute capacity.
          Change workloads, background load, machine availability or shared data-policy rules to see how allocations adapt.</p>
        <p>Built with Drasi, the demo integrates multiple sources, continuous queries, transformers and reactions
          in a custom embedded host. Database and runtime-status sources supply changes, queries assemble context
          and dashboard views, and reactions save placement plans and stream results to the UI.
          Four native Rust transformers provide the policy, optimization and simulation logic:</p>
        <div className="gpu-transformer-cards">
          <div><h2>Policy evaluator</h2><p>Uses Regorus to evaluate Rego policies in Rust.
            It determines which regional clusters may host each workload based on customer, data and processing-purpose rules.</p></div>
          <div><h2>Placement solver</h2><p>Uses the good_lp Rust library with the microlp mixed-integer solver to assign workload replicas to GPUs.
            It respects memory, compute, policy and VM-spread constraints, minimizing moves and balancing demand when replanning.</p></div>
          <div><h2>Resilience assessor</h2><p>Reuses the placement solver in Rust to evaluate the loss of each VM or region.
            It shows whether the remaining GPUs could accommodate the workloads under the same capacity and policy constraints.</p></div>
          <div><h2>Telemetry simulator</h2><p>A timer-driven Rust transformer models GPU execution and generates telemetry.
            It applies saved allocations and models background load, power state and reporting interruptions, feeding new measurements back into Drasi.</p></div>
        </div>
      </section>
      <section className="gpu-architecture-board" aria-labelledby="gpu-architecture-diagram-title">
        <div className="gpu-architecture-guide">
          <div><h2 id="gpu-architecture-diagram-title">Solution architecture</h2>
            <p id="gpu-architecture-hint">Click a component or arrow for details. Tab then Enter/Space also selects.
              Escape closes details first, then this overview.</p></div>
          <div className="gpu-architecture-legend" aria-label="Connection types">
            <span className="graph">Graph changes</span><span className="rows">Query rows</span>
            <span className="transport">HTTP / SQL / CDC</span><span className="lifecycle">Host / hub reporting</span>
          </div>
        </div>
        <p className="gpu-architecture-scroll-hint">On a narrow screen, scroll the diagram sideways or use the component and connection index below.</p>
        <div className="gpu-architecture-scroll" role="region" aria-label="Scrollable architecture diagram" tabIndex={0}>
          <div className="gpu-architecture-diagram" aria-describedby="gpu-architecture-hint">
            <div className="gpu-architecture-lane inputs"><span>INPUTS AND DATABASE FEEDBACK</span></div>
            <div className="gpu-architecture-lane processing"><span>EMBEDDED DRASI · QUERIES + NATIVE PLUGINS</span></div>
            <div className="gpu-architecture-lane observations"><span>RESULTS AND DASHBOARD</span></div>
            <svg className="gpu-architecture-connections" viewBox="0 0 1360 850" role="group" aria-label="Selectable dataflow connections">
              <defs>{['graph', 'rows', 'transport', 'lifecycle'].map(kind =>
                <marker key={kind} id={`gpu-arrow-${kind}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
                  <path d="M0 1 L9 5 L0 9" className={kind}/>
                </marker>)}</defs>
              {architectureEdges.map(edge => <g key={edge.id} className="gpu-architecture-edge" data-active={active?.id === edge.id}>
                <path d={edge.path} className={`gpu-edge-line ${edge.kind}`} markerEnd={`url(#gpu-arrow-${edge.kind})`} aria-hidden="true"/>
                {edge.branches?.map(path => <path key={path} d={path} className={`gpu-edge-line ${edge.kind}`} aria-hidden="true"/>)}
                <path d={[edge.path, ...edge.branches ?? []].join(' ')} className="gpu-edge-target" data-edge={edge.id}
                  role="button" tabIndex={0} aria-label={edge.title} aria-pressed={active?.id === edge.id}
                  aria-controls={active?.id === edge.id ? 'gpu-architecture-details' : undefined}
                  onClick={event => select(edge, event.currentTarget)}
                  onKeyDown={event => {
                    if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(edge, event.currentTarget); }
                  }}/>
              </g>)}
              <g aria-hidden="true" className="gpu-edge-labels">
                <text x="530" y="192">PLACEMENT PLAN</text>
                <text x="860" y="208">CURRENT CONFIGURATION + POLICY</text>
                <text x="742" y="465">FAILURE ANALYSIS</text>
                <text x="1030" y="620">COMPONENT OUTPUTS</text>
                <text x="650" y="815">QUERY SNAPSHOTS + SSE THROUGH CONTROL TO REACT</text>
              </g>
            </svg>
            {architectureNodes.map(node => <button key={node.id} type="button"
              className={`gpu-architecture-node ${node.kind}`} data-node={node.id}
              style={{ left: `${node.x / 1360 * 100}%`, top: `${node.y / 850 * 100}%` }}
              aria-label={node.title} aria-pressed={active?.id === node.id}
              aria-controls={active?.id === node.id ? 'gpu-architecture-details' : undefined}
              onClick={event => select(node, event.currentTarget)}>
              <span>{node.label}</span><strong>{node.title}</strong><small>{node.summary}</small>
            </button>)}
          </div>
        </div>
        <p className="gpu-architecture-caption">Queries are grouped by role.
          Select a component or connection for implementation details and data contracts.</p>
        <details className="gpu-architecture-index"><summary>Component and connection index</summary>
          <h3>Components</h3><div>{architectureNodes.map(node => <button key={node.id} type="button"
            aria-pressed={active?.id === node.id} onClick={event => select(node, event.currentTarget)}>{node.title}</button>)}</div>
          <h3>Connections</h3><div>{architectureEdges.map(edge => <button key={edge.id} type="button"
            aria-pressed={active?.id === edge.id} onClick={event => select(edge, event.currentTarget)}>{edge.title}</button>)}</div>
        </details>
      </section>
      {active && <section id="gpu-architecture-details" className="gpu-architecture-details" aria-labelledby="gpu-detail-title">
        <button className="gpu-detail-close" type="button" aria-label="Close selected detail" onClick={closeDetails}>×</button>
        <p className="eyebrow">{'sources' in active ? 'CONNECTION CONTRACT' : active.label}</p>
        <h2 id="gpu-detail-title" ref={heading} tabIndex={-1}>{active.title}</h2>
        <code className="gpu-detail-implementation">{active.implementation}</code>
        <p>{active.detail}</p>
        {'sources' in active ? <p><strong>When it changes.</strong> {active.changes}</p>
          : <><dl><dt>Input</dt><dd>{active.input}</dd><dt>Output</dt><dd>{active.output}</dd></dl>
            <p><strong>Example.</strong> {active.example}</p>
            {active.queryIds && <p><strong>Queries:</strong> {active.queryIds.map(id => <code className="gpu-query-id" key={id}>{id}</code>)}</p>}</>}
        {active.schema && <><p className="gpu-schema-label">Source query or compact contract · explanatory, not live data</p><pre><code>{active.schema}</code></pre></>}
        <p className="gpu-detail-note">{active.note}</p>
      </section>}
    </div>
  </dialog>;
}
