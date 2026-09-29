import { useCallback, useEffect, useId, useRef, useState, type ReactNode, type RefObject } from 'react';

type HierarchyKind = 'region' | 'vm' | 'gpu' | 'workload' | 'replica';
type PresenterTarget = Exclude<HierarchyKind, 'replica'>;

const icons = {
  region: <><path d="M12 21s7-6 7-12a7 7 0 1 0-14 0c0 6 7 12 7 12Z"/><circle cx="12" cy="9" r="2.5"/></>,
  vm: <><rect x="3" y="3" width="18" height="14" rx="2"/><path d="M8 21h8M12 17v4M6 7h12M6 11h5"/></>,
  gpu: <><rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M8 2v3m4-3v3m4-3v3M8 19v3m4-3v3m4-3v3M2 8h3m-3 4h3m-3 4h3m14-8h3m-3 4h3m-3 4h3"/></>,
  workload: <><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></>,
  replica: <path d="m12 3 8 4v10l-8 4-8-4V7l8-4Zm0 8 8-4M4 7l8 4v10"/>,
  chevron: <path d="m6 9 6 6 6-6"/>,
} satisfies Record<HierarchyKind | 'chevron', ReactNode>;

export function HierarchyIcon({ kind }: { kind: keyof typeof icons }) {
  return <svg className={`hierarchy-icon hierarchy-icon-${kind}`} viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {icons[kind]}
  </svg>;
}

const targets: readonly { kind: PresenterTarget; label: string; description: string }[] = [
  { kind: 'region', label: 'Region', description: 'Regions are the outer geographical containers for VMs.' },
  { kind: 'vm', label: 'VM', description: 'Each VM contains GPUs and forms a shared failure boundary.' },
  { kind: 'gpu', label: 'GPU', description: 'Each GPU has its own capacity and hosts individual workload replicas.' },
  { kind: 'workload', label: 'Workloads', description: 'Workload definitions span the fleet. Their replicas are the placements inside GPUs.' },
];

export function usePresenterHighlight(root: RefObject<HTMLElement>) {
  const [target, setTarget] = useState<PresenterTarget | null>(null);
  const [reveal, setReveal] = useState(true);
  const savedOpen = useRef(new Map<HTMLDetailsElement, boolean>());
  const revealedTarget = useRef<PresenterTarget | null>(null);
  const restore = useCallback(() => {
    const focused = document.activeElement;
    let hidesFocus = false;
    for (const [element, open] of savedOpen.current) {
      if (element.isConnected) {
        hidesFocus ||= !open && element.contains(focused);
        element.open = open;
      }
    }
    savedOpen.current.clear();
    if (hidesFocus) {
      const help = root.current?.querySelector<HTMLDetailsElement>('.reading-guide');
      const destination = help?.open
        ? help.querySelector<HTMLButtonElement>(`[data-presenter-target="${revealedTarget.current}"]`)
        : help?.querySelector<HTMLElement>('summary');
      destination?.focus({ preventScroll: true });
    }
  }, [root]);
  const clear = useCallback(() => {
    restore();
    revealedTarget.current = null;
    setTarget(null);
  }, [restore]);

  useEffect(() => {
    const requested = reveal ? target : null;
    if (revealedTarget.current !== requested) {
      restore();
      revealedTarget.current = requested;
    }
    const container = root.current;
    if (!requested || !container) return;
    for (const element of savedOpen.current.keys()) {
      if (!container.contains(element)) savedOpen.current.delete(element);
    }
    const selector = requested === 'workload'
      ? '[data-hierarchy-kind="workload"], [data-hierarchy-kind="replica"]'
      : `[data-hierarchy-kind="${requested}"]`;
    for (const item of container.querySelectorAll(selector)) {
      let parent = item.parentElement?.closest<HTMLDetailsElement>('details[data-hierarchy-container]');
      while (parent && container.contains(parent)) {
        if (!savedOpen.current.has(parent)) {
          savedOpen.current.set(parent, parent.open);
          parent.open = true;
        }
        parent = parent.parentElement?.closest<HTMLDetailsElement>('details[data-hierarchy-container]');
      }
    }
  });
  useEffect(() => restore, [restore]);
  useEffect(() => {
    if (!target) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || document.querySelector('dialog[open]')) return;
      if (event.target instanceof Element && event.target.closest('select, textarea, [contenteditable="true"]')) return;
      event.preventDefault();
      clear();
    };
    // Clear the teaching overlay before the analysis drawer handles Escape.
    document.addEventListener('keydown', escape, true);
    return () => document.removeEventListener('keydown', escape, true);
  }, [target, clear]);

  const toggle = useCallback((next: PresenterTarget) => setTarget(current => current === next ? null : next), []);
  return { target, reveal, setReveal, toggle, clear };
}

export function PresenterControls({ presenter }: { presenter: ReturnType<typeof usePresenterHighlight> }) {
  const description = useId();
  const selected = targets.find(item => item.kind === presenter.target);
  return <section className="presenter-controls" aria-label="Presenter highlighting">
    <h3>Explain the hierarchy</h3>
    <p>Highlight a component type while presenting. Outlines explain structure, not health.</p>
    <div className="presenter-targets" role="group" aria-label="Highlight component types">
      {targets.map(item => <button type="button" key={item.kind} data-presenter-target={item.kind}
        aria-pressed={presenter.target === item.kind} aria-describedby={description}
        onClick={() => presenter.toggle(item.kind)}>
        <HierarchyIcon kind={item.kind}/><span>{item.label}</span><span className="presenter-check" aria-hidden="true"/>
      </button>)}
    </div>
    <div className="presenter-options">
      <label><input type="checkbox" checked={presenter.reveal} onChange={event => presenter.setReveal(event.target.checked)}/>
        Temporarily reveal collapsed regions and VMs</label>
      <button type="button" onClick={presenter.clear}>Clear highlight</button>
    </div>
    <p id={description} className="presenter-description" aria-live="polite">
      {selected ? selected.description : 'Region contains VMs; VMs contain GPUs; GPUs host workload replicas.'}
    </p>
    <p className="presenter-keyboard">Press again, Clear, or Escape to finish. Previous expansion state is restored.
      {' '}Closing Help also clears highlighting. No data or configuration changes.</p>
  </section>;
}
