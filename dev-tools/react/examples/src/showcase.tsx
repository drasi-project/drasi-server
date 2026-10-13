// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { DataTable, Modal, queryTableState, type SortConfig } from '@drasi/react/components';
import type { DrasiError, QueryStatus } from '@drasi/react/client';
import '@drasi/react/styles.css';
import './layout.css';
import { Shell } from './Shell';
import { columns, rowKey } from './columns';
import type { Reading } from './readings';
import { simulatedQuery, simulatedRows, simulatedStates } from './showcaseState';

function Showcase() {
  const [status, setStatus] = useState<QueryStatus>('initial-loading');
  const [rows, setRows] = useState(simulatedRows);
  const [sort, setSort] = useState<SortConfig | null>(null);
  const [coolTheme, setCoolTheme] = useState(false);
  const [selected, setSelected] = useState<Reading | null>(null);
  const scope = useRef<HTMLElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const retry = () => setStatus('resynchronizing');
  const query = simulatedQuery(status, rows, retry);
  const close = () => setSelected(null);
  return <Shell title="Cold storage: simulated states">
    <p><strong>Simulation only.</strong> No server, requests, snapshots or real reconnects.
      Select deterministic presentation states; this is not protocol evidence.</p>
    <section ref={scope} className={coolTheme ? 'example-theme example-controls' : 'example-controls'} aria-label="State showcase">
      <h2 ref={heading} tabIndex={-1}>Presentation controls</h2>
      <label>Simulated query state{' '}
        <select value={status} onChange={event => {
          const next = simulatedStates.find(value => value === event.target.value);
          if (!next) throw new Error('Unknown simulated state');
          setStatus(next);
        }}>
          {simulatedStates.map(state => <option key={state}>{state}</option>)}
        </select>
      </label>{' '}
      <label><input type="checkbox" checked={coolTheme} onChange={event => setCoolTheme(event.target.checked)} /> Use cool theme</label>
      <p role="status">Simulated query: {status}{query.stale ? ' (last-good data)' : ''}</p>
      <p>
        <button type="button" disabled={status !== 'live'} onClick={() =>
          setRows(current => current.map(row => row.key === 'sim-probe-101' ? { ...row, celsius: row.celsius + 1 } : row))}>
          Apply simulated update
        </button>{' '}
        <button type="button" disabled={status !== 'resynchronizing'} onClick={() => setStatus('live')}>
          Finish simulated refresh
        </button>{' '}
        <button type="button" onClick={() => setSort(null)}>Input order</button>
      </p>
      <DataTable<Reading, DrasiError>
        title="Simulated readings" rows={query.data} columns={columns} rowKey={rowKey}
        state={{ ...queryTableState(query, retry), retryLabel: 'Retry simulated query' }}
        sort={sort} onSortChange={setSort} animateOnChange="celsius"
        height="var(--drasi-example-height, 20rem)"
        emptyMessage="No simulated readings."
        actions={[{ icon: <span aria-hidden="true">i</span>, label: 'Inspect probe', onClick: setSelected }]}
      />
      <Modal
        open={selected !== null} onClose={close} title="Simulated probe details"
        description="Example-owned content in a generic dialog."
        initialFocusRef={closeButton} fallbackFocusRef={heading} themeRef={scope}
      >
        <h2>Simulated probe details</h2>
        <p>{selected?.key}: {selected?.celsius.toFixed(1)} C in {selected?.room} room.</p>
        <p>The table and this body portal share the local theme. Closing restores the row action's focus.</p>
        <button ref={closeButton} type="button" onClick={close}>Close details</button>
      </Modal>
    </section>
  </Shell>;
}

createRoot(document.getElementById('root')!).render(<Showcase />);
