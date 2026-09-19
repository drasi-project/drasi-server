// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import {
  StrictMode, useEffect, useRef, useState, type CSSProperties, type RefObject,
} from 'react';
import { createRoot } from 'react-dom/client';
import {
  DataTable, Modal, type ColumnDef, type DataTableProps, type DataTableState, type SortConfig, type TableHeight,
} from '@drasi/react/components';
import { useReducedMotion } from '@drasi/react/react';
import '@drasi/react/styles.css';
import './host.css';

interface Item {
  id: string;
  name: string;
  quantity: number;
}

const initialRows: readonly Item[] = [
  { id: 'b', name: 'Bravo', quantity: 2 },
  { id: 'a', name: 'Alpha', quantity: 1 },
  { id: 'c', name: 'Charlie', quantity: 3 },
];
const columns: readonly ColumnDef<Item>[] = [
  { key: 'name', label: 'Name' },
  { key: 'quantity', label: 'Quantity', align: 'right' },
  { key: 'id', label: 'Code', sortable: false },
];
const rowKey = (item: Item) => item.id;
const params = new URLSearchParams(window.location.search);

function StrictModeProbe() {
  const setups = useRef(0);
  const [count, setCount] = useState(0);
  useEffect(() => { setCount(++setups.current); }, []);
  return <output className="fixture-output" data-testid="strict-setups" aria-label="StrictMode effect setups">{count}</output>;
}

function SortTable({ controlled }: { controlled: boolean }) {
  const id = controlled ? 'controlled' : 'uncontrolled';
  const [sort, setSort] = useState<SortConfig | null>(null);
  const [applyRequests, setApplyRequests] = useState(true);
  const [calls, setCalls] = useState(0);
  const [lastRequest, setLastRequest] = useState<SortConfig | null>(null);
  return (
    <section className="fixture-section" aria-label={`${id} sorting`}>
      {controlled && (
        <div className="fixture-controls">
          <label>
            <input type="checkbox" checked={applyRequests} onChange={event => setApplyRequests(event.target.checked)} />
            Apply controlled sort requests
          </label>
          <button type="button" onClick={() => setSort({ column: 'quantity', direction: 'desc' })}>
            Set sort from owner
          </button>
          <button type="button" onClick={() => setSort(null)}>Clear sort from owner</button>
        </div>
      )}
      <output className="fixture-output" aria-label={`${id} sort callbacks`} data-testid={`${id}-calls`}>{calls}</output>
      <output className="fixture-output" aria-label={`${id} last requested sort`} data-testid={`${id}-request`}>
        {JSON.stringify(lastRequest)}
      </output>
      <DataTable
        title={controlled ? 'Controlled inventory' : 'Uncontrolled inventory'}
        rows={initialRows}
        columns={columns}
        rowKey={rowKey}
        height={250}
        {...(controlled ? { sort } : { defaultSort: null })}
        onSortChange={next => {
          setCalls(value => value + 1);
          setLastRequest(next);
          if (applyRequests) setSort(next);
        }}
        renderHeader={context => (
          <>
            {context.defaultRender()}
            <button type="button" onClick={() => context.setSort(null)}>Clear {id} sort</button>
          </>
        )}
      />
    </section>
  );
}

function StateFixture() {
  const stateName = params.get('state') ?? 'actions';
  const [retryCount, setRetryCount] = useState(0);
  const [action, setAction] = useState('None');
  const state: DataTableState = {
    ...(stateName === 'loading' || stateName === 'refreshing' ? { loading: true } : {}),
    ...(stateName === 'error' || stateName === 'stale-error' ? {
      error: new Error('Synthetic table unavailable'),
      retry: () => setRetryCount(value => value + 1),
    } : {}),
    ...(stateName === 'stale' || stateName === 'stale-error' ? { stale: true } : {}),
  };
  const rows = stateName === 'loading' || stateName === 'error' ? null
    : stateName === 'empty' ? [] : initialRows;
  return (
    <>
      <DataTable
        title={`${stateName} inventory`}
        ariaLabel={`${stateName} records`}
        rows={rows}
        columns={columns}
        rowKey={rowKey}
        state={state}
        actions={stateName === 'actions' ? [{
          label: 'Inspect item',
          icon: <span aria-hidden="true">+</span>,
          onClick: item => setAction(item.name),
          loading: item => item.id === 'b',
          disabled: item => item.id === 'c',
        }] : undefined}
      />
      <output className="fixture-output" aria-label="Retry count" data-testid="retry-count">{retryCount}</output>
      <output className="fixture-output" aria-label="Inspected item" data-testid="action-result">{action}</output>
    </>
  );
}

type Owner = 'primary' | 'secondary';
type TriggerState = 'available' | 'missing' | 'disabled' | 'hidden';

interface OwnerProps {
  id: Owner;
  open: boolean;
  close: () => void;
  completeClose: () => void;
  openOther: () => void;
  closeOther: () => void;
  unmountOther: () => void;
  unmountSelf: () => void;
  unmountAll: () => void;
  closeAll: () => void;
  changeTrigger: (state: TriggerState) => void;
  fallback: RefObject<HTMLButtonElement>;
  returnTarget: RefObject<HTMLButtonElement>;
}

function ModalOwner({
  id, open, close, completeClose, openOther, closeOther, unmountOther, unmountSelf, unmountAll,
  closeAll, changeTrigger, fallback, returnTarget,
}: OwnerProps) {
  const [removable, setRemovable] = useState(true);
  const initialFocus = useRef<HTMLInputElement>(null);
  const other = id === 'primary' ? 'secondary' : 'primary';
  return (
    <Modal
      open={open}
      onClose={close}
      title={`${id} dialog`}
      description={`${id} test controls`}
      initialFocusRef={params.has('initial-focus') ? initialFocus : undefined}
      returnFocusRef={params.has('explicit-return') ? returnTarget : undefined}
      fallbackFocusRef={params.has('body-fallback') ? undefined : fallback}
      closeOnEscape={!params.has('no-dismiss')}
      closeOnOutsideClick={!params.has('no-dismiss')}
    >
      <div className="fixture-modal-body" data-testid={`${id}-contents`}>
        <h2>{id} controls</h2>
        <button type="button">First {id} control</button>
        <label>{id} note<input ref={initialFocus} /></label>
        {removable && <button type="button" onClick={() => setRemovable(false)}>Remove focused {id} control</button>}
        {params.has('long') && (
          <div className="fixture-long-content">
            {Array.from({ length: 30 }, (_, index) => <p key={index}>Keyboard-scrollable paragraph {index + 1}.</p>)}
          </div>
        )}
        <button type="button" onClick={openOther}>Open {other} layer</button>
        <button type="button" onClick={closeOther}>Close {other} owner</button>
        <button type="button" onClick={unmountOther}>Unmount {other} owner</button>
        <button type="button" onClick={() => changeTrigger('missing')}>Remove {id} trigger</button>
        <button type="button" onClick={() => changeTrigger('disabled')}>Disable {id} trigger</button>
        <button type="button" onClick={() => changeTrigger('hidden')}>Hide {id} trigger</button>
        <button type="button" onClick={unmountSelf}>Unmount {id} dialog</button>
        <button type="button" onClick={closeAll}>Close all owners</button>
        <button type="button" onClick={unmountAll}>Unmount all owners</button>
        <button type="button" onClick={() => root.unmount()}>Unmount fixture</button>
        {params.has('pending-close') && (
          <button type="button" onClick={completeClose}>Apply {id} close request</button>
        )}
        <button type="button" onClick={close}>Close {id} dialog</button>
      </div>
    </Modal>
  );
}

function ModalFixture() {
  const [open, setOpen] = useState<Record<Owner, boolean>>({ primary: params.has('initial'), secondary: false });
  const [mounted, setMounted] = useState<Record<Owner, boolean>>({ primary: true, secondary: true });
  const [closes, setCloses] = useState<Record<Owner, number>>({ primary: 0, secondary: 0 });
  const [triggers, setTriggers] = useState<Record<Owner, TriggerState>>({ primary: 'available', secondary: 'available' });
  const fallback = useRef<HTMLButtonElement>(null);
  const returnTarget = useRef<HTMLButtonElement>(null);
  const close = (id: Owner) => {
    setCloses(value => ({ ...value, [id]: value[id] + 1 }));
    if (!params.has('pending-close')) setOpen(value => ({ ...value, [id]: false }));
  };
  return (
    <>
      <div className="fixture-controls">
        {(['primary', 'secondary'] as const).map(id => triggers[id] !== 'missing' && (
          <button
            key={id}
            type="button"
            disabled={triggers[id] === 'disabled'}
            hidden={triggers[id] === 'hidden'}
            onClick={() => {
              setMounted(value => ({ ...value, [id]: true }));
              setOpen(value => ({ ...value, [id]: true }));
            }}
          >Open {id} dialog</button>
        ))}
        <button type="button" ref={fallback}>Fallback target</button>
        <button type="button" ref={returnTarget}>Explicit return target</button>
        <button type="button">Background control</button>
      </div>
      {(['primary', 'secondary'] as const).map(id => {
        const other = id === 'primary' ? 'secondary' : 'primary';
        return (
          <section key={id} aria-label={`${id} owner`}>
            <output className="fixture-output" aria-label={`${id} close callbacks`} data-testid={`${id}-closes`}>{closes[id]}</output>
            {mounted[id] && (
              <ModalOwner
                id={id}
                open={open[id]}
                close={() => close(id)}
                completeClose={() => setOpen(value => ({ ...value, [id]: false }))}
                openOther={() => {
                  setMounted(value => ({ ...value, [other]: true }));
                  setOpen(value => ({ ...value, [other]: true }));
                }}
                closeOther={() => close(other)}
                unmountOther={() => setMounted(value => ({ ...value, [other]: false }))}
                unmountSelf={() => setMounted(value => ({ ...value, [id]: false }))}
                unmountAll={() => setMounted({ primary: false, secondary: false })}
                closeAll={() => setOpen({ primary: false, secondary: false })}
                changeTrigger={state => setTriggers(value => ({ ...value, [id]: state }))}
                fallback={fallback}
                returnTarget={returnTarget}
              />
            )}
          </section>
        );
      })}
    </>
  );
}

function HostControls() {
  return (
    <section className="fixture-host" aria-label="Host styling probe">
      <h2 data-testid="host-heading">Host heading</h2>
      <button type="button" data-testid="host-button">Host button</button>
      <table aria-label="Host table" data-testid="host-table">
        <thead><tr><th scope="col">Host column</th></tr></thead>
        <tbody><tr><td>Host cell</td></tr></tbody>
      </table>
    </section>
  );
}

function ThemeFixture() {
  const [palette, setPalette] = useState('violet');
  const [firstOpen, setFirstOpen] = useState(false);
  const [secondOpen, setSecondOpen] = useState(false);
  const secondTheme = useRef<HTMLDivElement>(null);
  return (
    <>
      <HostControls />
      <section className="fixture-section" data-testid="default-theme">
        <DataTable title="Default light inventory" rows={initialRows} columns={columns} rowKey={rowKey} height={220} />
      </section>
      <section className="fixture-theme" data-palette={palette} data-testid="first-theme">
        <button type="button" onClick={() => setFirstOpen(true)}>Open themed dialog</button>
        <DataTable title="Scoped inventory" rows={initialRows} columns={columns} rowKey={rowKey} height={220} />
        <Modal open={firstOpen} onClose={() => setFirstOpen(false)} title="Scoped theme dialog">
          <div className="fixture-modal-body">
            <p className="fixture-typography-probe" data-testid="themed-typography">Portaled theme content</p>
            <button type="button" onClick={() => setPalette(value => value === 'violet' ? 'ocean' : 'violet')}>Change ancestor theme</button>
            <button type="button" onClick={() => setSecondOpen(true)}>Open other theme</button>
            <button type="button" onClick={() => setFirstOpen(false)}>Close themed dialog</button>
          </div>
        </Modal>
      </section>
      <div
        ref={secondTheme}
        className={`fixture-theme${params.has('environment') ? ' fixture-environment-theme' : ''}`}
        data-palette="ocean"
        data-testid="second-theme"
      >
        <p>Explicit second theme source</p>
      </div>
      <Modal
        open={secondOpen}
        onClose={() => setSecondOpen(false)}
        title="Other theme dialog"
        themeRef={secondTheme}
      >
        <div className="fixture-modal-body">
          <p>Independently themed portal</p>
          <button type="button" onClick={() => setSecondOpen(false)}>Close other theme</button>
        </div>
      </Modal>
    </>
  );
}

function SizingFixture() {
  const cases: { id: string; height?: TableHeight; style?: DataTableProps<Item>['style']; parentStyle?: CSSProperties }[] = [
    { id: 'default' },
    { id: 'numeric', height: 180, style: { height: 999 } },
    { id: 'pixels', height: '150px' },
    { id: 'rem', height: '12rem' },
    { id: 'em', height: '10em' },
    { id: 'viewport', height: '25vh' },
    { id: 'dynamic-viewport', height: '25dvh' },
    { id: 'percentage', height: '50%' },
    { id: 'zero', height: 0 },
    { id: 'zero-string', height: '0' },
    { id: 'auto', height: 'auto' },
    { id: 'variable', height: 'var(--fixture-size)', parentStyle: { '--fixture-size': '216px' } as CSSProperties },
    { id: 'variable-fallback', height: 'var(--fixture-missing, 9rem)' },
    { id: 'default-token', parentStyle: { '--drasi-table-height': '232px' } as CSSProperties },
  ];
  return (
    <>
      {cases.map(value => (
        <section
          key={value.id}
          className={`fixture-size-case ${value.id === 'percentage' ? 'fixture-percent-parent' : ''}`}
          style={value.parentStyle}
          data-testid={`size-${value.id}`}
          aria-label={`${value.id} sizing`}
        >
          <DataTable
            ariaLabel={`${value.id} sized records`}
            rows={initialRows}
            columns={columns}
            rowKey={rowKey}
            height={value.height}
            style={value.style}
          />
        </section>
      ))}
    </>
  );
}

function MotionFixture() {
  const reduced = useReducedMotion();
  const [rows, setRows] = useState(initialRows);
  const [mounted, setMounted] = useState(true);
  const controlledAnimations = new Map([['b', 'up' as const]]);
  return (
    <>
      <output className="fixture-output" data-testid="motion-preference" aria-label="Motion preference">
        {reduced ? 'reduce' : 'no-preference'}
      </output>
      <div className="fixture-controls">
        <button type="button" onClick={() => setRows(value => value.map(item => ({ ...item, quantity: item.quantity + 5 })))}>
          Update quantities
        </button>
        <button type="button" onClick={() => setMounted(false)}>Unmount animated tables</button>
      </div>
      {mounted && (
        <>
          <DataTable title="Tracked motion" rows={rows} rowKey={rowKey} columns={columns} animateOnChange="quantity" height={240} />
          <DataTable title="Controlled motion" rows={rows} rowKey={rowKey} columns={columns} rowAnimations={controlledAnimations} height={240} />
        </>
      )}
    </>
  );
}

function DeferredModalFixture() {
  const [mounted, setMounted] = useState(false);
  const [open, setOpen] = useState(false);
  const [closes, setCloses] = useState(0);
  const close = () => {
    setCloses(value => value + 1);
    setOpen(false);
  };
  return (
    <>
      <div className="fixture-controls">
        <button type="button" onClick={() => setMounted(true)}>Mount closed modal</button>
        <button type="button" onClick={() => { setMounted(true); setOpen(true); }}>Open deferred modal</button>
        <button type="button" onClick={() => setOpen(false)}>Cancel pending open</button>
        <button type="button" onClick={() => setMounted(false)}>Unmount pending owner</button>
        <label>Background note<input /></label>
      </div>
      <output className="fixture-output" data-testid="deferred-closes" aria-label="Deferred close callbacks">{closes}</output>
      {mounted && (
        <Modal open={open} onClose={close} title="Deferred dialog">
          <div className="fixture-modal-body">
            <button type="button">First deferred control</button>
            <button type="button" onClick={close}>Close deferred dialog</button>
          </div>
        </Modal>
      )}
    </>
  );
}

function StaticScrollFixture() {
  const rows: Item[] = Array.from({ length: 40 }, (_, index) => ({
    id: `item-${index + 1}`, name: `Inventory item ${index + 1}`, quantity: index + 1,
  }));
  return (
    <>
      <div className="fixture-controls"><label>Before viewport<input /></label></div>
      <DataTable
        title="Static inventory"
        ariaLabel="Read-only records"
        rows={rows}
        columns={columns.map(column => ({ ...column, sortable: false }))}
        rowKey={rowKey}
        height={220}
        className="fixture-static-table"
      />
      <div className="fixture-controls"><label>After viewport<input /></label></div>
    </>
  );
}

function Fixture() {
  const name = params.get('case');
  return (
    <main className={`fixture-main${params.has('scrolled-host') ? ' fixture-scrolled-host' : ''}`}>
      <h1>Installed component browser fixture</h1>
      <StrictModeProbe />
      {name === 'modal' ? <ModalFixture />
        : name === 'deferred-modal' ? <DeferredModalFixture />
        : name === 'static-scroll' ? <StaticScrollFixture />
        : name === 'theme' ? <ThemeFixture />
        : name === 'sizing' ? <SizingFixture />
        : name === 'motion' ? <MotionFixture />
        : name === 'states' ? <StateFixture />
        : <><SortTable controlled /><SortTable controlled={false} /></>}
    </main>
  );
}

const container = document.getElementById('root');
if (!container) throw new Error('Missing fixture root');
const root = createRoot(container);
root.render(<StrictMode><Fixture /></StrictMode>);
