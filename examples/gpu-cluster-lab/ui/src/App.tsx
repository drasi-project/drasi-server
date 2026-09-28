import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useDrasiClient, useDrasiConnectionStatus, useDrasiQuery } from '@drasi/react/react';
import type { ResultRow } from '@drasi/react/client';
import { rowKey, validateRow, text, number, boolean, strings, records, type QueryId } from './rows';
import { DecisionEvidence, PlacementEvidence, PolicyEvidence, ResilienceEvidence, StatusEvidence,
  TimelineEvidence, timelineMessage, clusterReplicaCounts, correlated, count, currentFeeds } from './Evidence';
import { label } from './labels';
import { GpuCard } from './GpuCard';
import { WorkloadsPanel } from './WorkloadsPanel';

function useView(id: QueryId) {
  return useDrasiQuery(id, { getKey: row => rowKey(id, row), transform: row => validateRow(id, row) });
}

export type Command = (path: string, method: string, body?: unknown, idempotencyKey?: string, expectedRevision?: string) => Promise<void>;
export type Commands = { send: Command; pending: boolean; notice: string | null; error: string | null };
export type View = {
  data: ResultRow[] | null;
  loading: boolean;
  stale: boolean;
  error: Error | null;
  status: string;
  retry: () => void;
};
export type Views = Record<QueryId, View>;
function useCommand(resetSubscriptions: () => void): Commands {
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const send: Command = async (path, method, body, idempotencyKey, expectedRevision) => {
    setPending(true); setNotice(null); setError(null);
    try {
      const csrf = await fetch('/api/csrf', { cache: 'no-store' });
      if (!csrf.ok) throw new Error(`Command authorization unavailable (${csrf.status})`);
      const token: unknown = await csrf.json();
      if (!token || typeof token !== 'object' || !('token' in token) || typeof token.token !== 'string') {
        throw new Error('Invalid command authorization response');
      }
      const headers: Record<string, string> = { 'Content-Type': 'application/json', 'X-CSRF-Token': token.token };
      if (idempotencyKey) headers['Idempotency-Key'] = idempotencyKey;
      if (expectedRevision) headers['If-Match'] = expectedRevision;
      const response = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
      if (!response.ok) {
        const detail: unknown = await response.json();
        const message = detail && typeof detail === 'object' && 'message' in detail && typeof detail.message === 'string'
          ? detail.message : `Command failed (${response.status})`;
        throw new Error(message);
      }
      if (path.startsWith('/api/demo/presets/')) {
        resetSubscriptions();
        setNotice('Scenario reset completed. Reconnecting to its query results.');
      } else {
        setNotice('Change saved to the database. Waiting for the demo to process it, apply any new plan and confirm the result with GPU reports.');
      }
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      throw failure;
    } finally { setPending(false); }
  };
  return { send, pending, notice, error };
}

export function LiveApp() {
  // Each shared query is subscribed exactly once, above all views.
  const gpus = useView('ui-gpus'), workloads = useView('ui-workloads'), placements = useView('ui-placements');
  const resilience = useView('ui-resilience'), decisions = useView('ui-decisions'), status = useView('ui-status');
  const timeline = useView('ui-timeline'), clusters = useView('ui-clusters'), policy = useView('ui-policy');
  const connection = useDrasiClient();
  const transport = useDrasiConnectionStatus();
  const [online, setOnline] = useState(() => navigator.onLine);
  useEffect(() => {
    const offline = () => setOnline(false);
    const reconnect = () => { connection.retry(); setOnline(true); };
    window.addEventListener('offline', offline);
    window.addEventListener('online', reconnect);
    setOnline(navigator.onLine);
    return () => {
      window.removeEventListener('offline', offline);
      window.removeEventListener('online', reconnect);
    };
  }, [connection.retry]);
  const command = useCommand(connection.retry);
  return <Lab views={{
    'ui-gpus': gpus, 'ui-workloads': workloads, 'ui-placements': placements, 'ui-resilience': resilience,
    'ui-decisions': decisions, 'ui-status': status, 'ui-timeline': timeline, 'ui-clusters': clusters, 'ui-policy': policy,
  }} connection={{
    ...connection,
    initialized: connection.initialized && transport.connected,
    error: !online ? new Error('Browser is offline') : connection.error ?? transport.error ?? null,
  }} command={command}/>;
}

export function Lab({ views: feeds, connection, command, demoPanel }: {
  views: Views;
  connection: { initialized: boolean; error: Error | null; retry: () => void };
  command: Commands;
  demoPanel?: React.ReactNode;
}) {
  const { 'ui-gpus': gpus, 'ui-workloads': workloads, 'ui-placements': placements,
    'ui-status': status, 'ui-clusters': clusters, 'ui-policy': policy } = feeds;
  const views = Object.values(feeds);
  const [dialog, setDialog] = useState<'workload' | 'host' | 'policy' | 'gpu' | 'workload-edit' | null>(null);
  const [selectedRow, setSelectedRow] = useState<ResultRow | null>(null);
  const [selectedWorkload, selectWorkload] = useState<string | null>(null);
  const [policyDetails, setPolicyDetails] = useState<{ workloadId: string; clusterId: string } | null>(null);
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const [decisionRequest, setDecisionRequest] = useState<{ id: string } | null>(null);
  const [systemOpen, setSystemOpen] = useState(false);
  const analysisButton = useRef<HTMLButtonElement>(null);
  const openDecision = (id: string) => { setAnalysisOpen(true); setDecisionRequest({ id }); };
  const toggleWorkload = (id: string) => selectWorkload(current => current === id ? null : id);
  const observedScenario = status.data?.find(row => row.fleet_id === 'demo')?.scenario;
  const [preset, setPreset] = useState(() =>
    observedScenario === 'fragmentation' || observedScenario === 'regional-boundary' ? observedScenario : 'baseline');
  useEffect(() => {
    if (observedScenario === 'baseline' || observedScenario === 'fragmentation' || observedScenario === 'regional-boundary') {
      setPreset(observedScenario);
    }
  }, [observedScenario]);
  const inputsReady = status.data?.some(row => row.fleet_id === 'demo' && row.inputs_ready === true) === true;
  const connectionUnavailable = !!connection.error || !connection.initialized;
  const stale = !currentFeeds(feeds) || connectionUnavailable;
  const systemView = { ...status, stale: status.stale || connectionUnavailable };
  const system = systemSummary(systemView);
  const disabled = command.pending || stale || !inputsReady;
  const run = (action: Promise<void>) => { void action.catch(() => { /* useCommand renders the failure. */ }); };
  const gpuRows = gpus.data ?? [];
  const patchGpu = (gpu: ResultRow, changes: Record<string, unknown>) => run(command.send(
    `/api/gpus/${encodeURIComponent(text(gpu, 'gpu_id'))}/telemetry`, 'PATCH',
    { expected_revision: text(gpu, 'telemetry_revision'), changes }));
  const activeWorkload = workloads.data?.find(w => w.workload_id === selectedWorkload);
  const setRegionPower = (region: string, members: ResultRow[], powered_on: boolean) => run(command.send(
    `/api/regions/${encodeURIComponent(region)}/telemetry`, 'PATCH',
    { gpu_revisions: Object.fromEntries(members.map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on } }));
  return <main className="lab-app">
    <header className="lab-header">
      <h1>GPU Cluster Lab</h1>
      <div className="scenario-controls" role="group" aria-label="Scenario setup">
        <div className="scenario-picker"><label htmlFor="preset">Starting scenario</label>
          <select id="preset" value={preset} onChange={event => setPreset(event.target.value)}>
            <option value="baseline">Baseline fleet</option>
            <option value="fragmentation">Memory fragmentation</option>
            <option value="regional-boundary">Regional policy</option>
          </select>
          <button disabled={command.pending} onClick={() => {
            if (window.confirm('Reset to the selected starting scenario? This replaces the current fleet and workload settings and restarts simulated workloads.')) {
              run(command.send(`/api/demo/presets/${preset}`, 'POST'));
            }
          }}>Reset scenario</button>
        </div>
      </div>
      <div className="header-tools">
      <button type="button" className={`system-status system-${system.kind}`} aria-label="System status"
        aria-describedby={system.note ? 'system-status-note' : undefined} aria-haspopup="dialog" title={system.detail} onClick={() => setSystemOpen(true)}>
        System status{system.note && <span id="system-status-note"> · {system.note}</span>}
      </button>
      <button type="button" ref={analysisButton} className="analysis-toggle" aria-controls="global-analysis" aria-expanded={analysisOpen}
        title="View fleet-wide placement progress and recovery assessment" onClick={() => { setDecisionRequest(null); setAnalysisOpen(open => !open); }}>Global analysis</button>
      <details className="reading-guide"><summary>Help</summary><div>
      <h3>How to read this demo</h3>
      <p><strong>Fleet → region → VM → GPU.</strong> The fleet is the whole demo. Each regional cluster groups VMs in one region.
        A VM (virtual machine) models Standard_NC80adis_H100_v5 with two H100 NVL GPUs. Losing a VM loses both GPUs. Different VMs do not imply different physical hosts or availability zones.</p>
      <p><strong>Workload and replica.</strong> A workload is an AI service, such as chat or document search. A replica is one copy of that service.
        Each replica must fit on one GPU; free memory on different GPUs cannot be combined. A replica move is a simulated restart on another GPU, not live memory migration.</p>
      <p><strong>Required / running / confirmed.</strong> Required is the configured number of replicas. Running means processing is active.
        Confirmed is the subset of running replicas with current policy permission and a recent GPU report confirming the applied plan.
        Each regional cluster shows planned, running and confirmed replicas; required counts belong to workloads, not regions.
        Planned is the number assigned to this region in the saved plan, which can lag behind configuration changes.
        Unknown means current evidence is unavailable or a requested stop has not been confirmed.</p>
      <p><strong>Memory and compute demand.</strong> Each GPU has an 80 GiB demo memory budget. GiB is a memory unit; 1 GiB is 1,024 MiB.
        Compute demand uses illustrative units: 100 is the reference capacity, and plans stay within 85, including background activity. Demand units are not utilization percentages or model benchmarks.</p>
      <p><strong>GPU reports and policy.</strong> A report becomes overdue after five seconds. Missing reports do not prove a GPU has stopped.
        Unknown policy pauses processing but keeps memory reserved. A policy denial stops processing and releases memory once the stop is confirmed.</p>
      <p><strong>Regional policy.</strong> The policy represents an example customer agreement, not a general GDPR requirement.
        This local simulation does not keep real data in the named regions.</p>
      <p>After a connection interruption, displayed information may remain out of date until a fresh snapshot arrives.</p>
      </div></details>
      </div>
    </header>
    {demoPanel}
    {(connection.error || views.some(v => v.error || v.stale)) && <section className="error" role="alert">
      <strong>Updates are unavailable or out of date. Previously received data does not confirm current policy permission.</strong>
      <p>{connection.error?.message ?? views.find(v => v.error)?.error?.message ?? 'Waiting for the latest data.'}</p>
      <button onClick={connection.retry}>Reconnect updates</button>
      {Object.entries(feeds).filter(([, v]) => v.error || v.stale).map(([id, v]) => <p key={id}>{label('query', id)}: {v.error?.message ?? 'Out-of-date data'}
        <button onClick={v.retry}>Retry {label('query', id).toLowerCase()} updates</button></p>)}
    </section>}
    {command.error && <div className="error" role="alert">{command.error}</div>}
    {command.notice && <div className="notice" role="status">{command.notice}</div>}
    <div className={`lab-layout${analysisOpen ? ' analysis-open' : ''}${analysisOpen && decisionRequest ? ' decision-navigation' : ''}`}>
    <WorkloadsPanel query={workloads} stale={stale} disabled={disabled} selectedWorkload={activeWorkload ? text(activeWorkload, 'workload_id') : null}
      onToggleWorkload={toggleWorkload} onAdd={() => setDialog('workload')} onEditPolicy={() => setDialog('policy')}
      onEdit={w => { setSelectedRow(w); setDialog('workload-edit'); }}
      onDelete={w => {
        if (window.confirm(`Delete ${text(w, 'name')}? Its replicas will stop after the demo processes this change.`)) {
          run(command.send(`/api/workloads/${encodeURIComponent(text(w, 'workload_id'))}`, 'DELETE', undefined, undefined, text(w, 'revision')));
        }
      }}/>
    <div className="lab-content">
    <section className="fleet" aria-label="Regions">
      {!clusters.data?.length && <Empty loading={clusters.loading}/>}
      {(clusters.data ?? []).map(cluster => {
        const rows = gpuRows.filter(g => g.cluster_id === cluster.cluster_id);
        const regionRows = gpuRows.filter(g => clusters.data?.some(c => c.cluster_id === g.cluster_id && c.region === cluster.region));
        const hosts = [...new Set(rows.map(g => text(g, 'host_id')))];
        const replicas = stale ? { planned: null, running: null, ready: null } : clusterReplicaCounts(feeds, text(cluster, 'cluster_id'));
        return <article className="cluster" key={text(cluster, 'cluster_id')}>
          <div className="cluster-title"><div className="region-heading"><span className="region-label">REGION</span>
            <h3>{text(cluster, 'name')} <small>· {label('region', text(cluster, 'region'))}</small></h3></div>
            <button disabled={disabled} aria-label={`Add ready VM to ${text(cluster, 'name')}`}
              onClick={() => { setSelectedRow(cluster); setDialog('host'); }}>Add ready VM</button>
            <button disabled={disabled || !rows.length} onClick={() => {
              if (window.confirm('Simulate every VM in this region losing power? Processing stops. Placement decisions detect the loss after GPU reports have been missing for five seconds.')) setRegionPower(text(cluster, 'region'), regionRows, false);
            }}>Simulate region failure</button>
            <button disabled={disabled || !regionRows.length} onClick={() => setRegionPower(text(cluster, 'region'), regionRows, true)}>Restore region</button></div>
          <details className="cluster-body"><summary><span className="cluster-summary">
            <span>{hosts.length} VMs in inventory · {count(cluster.healthy_gpus)} GPUs with recent reports{stale && ' (last received data)'}</span>
            <span className="regional-replicas" aria-label={`Replica activity in ${text(cluster, 'name')}`}
              title="Confirmed replicas are running, allowed by current policy and verified by recent GPU reports.">
              Replicas: <span className="planned-replicas" title={replicas.planned === null
                ? 'Saved-plan assignments are unavailable or cannot be matched to current inventory.'
                : `Assigned here in saved plan v${placements.data?.[0]?.desired_plan_version}. This is not a configured regional requirement; the saved plan can lag behind configuration changes.`}>
                <strong>{count(replicas.planned)}</strong> planned</span>{' · '}
              <span><strong>{count(replicas.running)}</strong> running</span>{' · '}
              <span><strong>{count(replicas.ready)}</strong> confirmed</span>
            </span>
            <span className="region-preview" aria-label={`VM and GPU summary in ${text(cluster, 'name')}`}>
              {hosts.length ? <><span className="region-preview-label" title="Colours describe report freshness, not execution or placement eligibility.">GPU reports</span>
                {hosts.map(host => <span className="vm-preview" key={host}><strong>{host}</strong><span className="vm-preview-gpus">
                  {rows.filter(g => g.host_id === host).map(g => {
                    const health = stale ? 'unknown' : text(g, 'health');
                    const settings = stale ? [] : [
                      ...(!boolean(g, 'powered_on') ? ['Power off'] : []),
                      ...(!boolean(g, 'reporting_enabled') ? ['Reports paused'] : []),
                      ...(!boolean(g, 'scheduling_enabled') ? ['Excluded from plans'] : []),
                    ];
                    const description = `${text(g, 'name')}: ${label('health', health)}${settings.length ? ` · ${settings.join(' · ')}` : ''}${stale ? ' · last received inventory' : ''}`;
                    return <span key={text(g, 'gpu_id')} className={`mini-gpu report-${health}`} role="img" aria-label={description} title={description}>
                      GPU {number(g, 'slot')}{settings.length > 0 && <span className="mini-gpu-setting"> · {settings[0]}</span>}
                    </span>;
                  })}
                </span></span>)}</> : <span className="muted">No VMs registered</span>}
            </span>
          </span></summary>
          {hosts.map(host => <section className="worker" key={host}><div className="section-title"><h4><span className="dot"/> {host}<small> · GPU VM · {rows.filter(g => g.host_id === host).length} GPU{rows.filter(g => g.host_id === host).length === 1 ? '' : 's'}</small></h4>
            <div><button disabled={disabled} onClick={() => run(command.send(`/api/hosts/${encodeURIComponent(host)}/telemetry`, 'PATCH',
              { gpu_revisions: Object.fromEntries(rows.filter(g => g.host_id === host).map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on: false } }))}>Power off VM</button>
            <button disabled={disabled} onClick={() => run(command.send(`/api/hosts/${encodeURIComponent(host)}/telemetry`, 'PATCH',
              { gpu_revisions: Object.fromEntries(rows.filter(g => g.host_id === host).map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on: true } }))}>Restore VM</button></div></div>
            <div className="gpu-grid">{rows.filter(g => g.host_id === host).map(g => {
              const pair = policy.data?.find(p => p.workload_id === activeWorkload?.workload_id && p.cluster_id === g.cluster_id);
              const authorization = !stale && pair?.current && pair.policy_signature === status.data?.[0]?.policy_signature
                && pair.observation_epoch === status.data?.[0]?.observation_epoch ? pair.authorization : 'unknown';
              return <GpuCard key={text(g, 'gpu_id')} gpu={g} placement={placements.data?.[0]} workloads={workloads.data ?? []}
                selectedWorkload={activeWorkload ? text(activeWorkload, 'workload_id') : null} onToggleWorkload={toggleWorkload}
                stale={stale} allocationStale={stale || !correlated(placements.data?.[0], status.data?.[0])}
                permission={activeWorkload ? {
                  workloadName: text(activeWorkload, 'name'), authorization: String(authorization),
                  inspect: () => setPolicyDetails({ workloadId: text(activeWorkload, 'workload_id'), clusterId: text(g, 'cluster_id') }),
                } : undefined}
                disabled={disabled} actions={{
                  reports: () => patchGpu(g, { reporting_enabled: !boolean(g, 'reporting_enabled') }),
                  power: () => patchGpu(g, { powered_on: !boolean(g, 'powered_on') }),
                  edit: () => { setSelectedRow(g); setDialog('gpu'); },
                  scheduling: () => run(command.send(`/api/gpus/${encodeURIComponent(text(g, 'gpu_id'))}`, 'PATCH',
                    { expected_revision: text(g, 'inventory_revision'), changes: { scheduling_enabled: !boolean(g, 'scheduling_enabled') } })),
                  remove: () => {
                    if (window.confirm(`Remove ${text(g, 'name')} from inventory? Any workload replicas on this simulated GPU will stop.`)) {
                      run(command.send(`/api/gpus/${encodeURIComponent(text(g, 'gpu_id'))}`, 'DELETE', undefined, undefined, text(g, 'inventory_revision')));
                    }
                  },
                }}/>;
            })}</div></section>)}
          {!hosts.length && <p className="muted">No VMs in this regional cluster.</p>}
          </details>
        </article>;
      })}
    </section>
    <Activity feeds={feeds} connectionUnavailable={connectionUnavailable} onDecision={openDecision}/>
    </div>
    <AnalysisDrawer open={analysisOpen} onOpenChange={setAnalysisOpen} returnFocus={analysisButton} feeds={feeds} stale={stale}
      decisionRequest={decisionRequest} onDecision={openDecision}/>
    </div>
    {dialog && <CommandDialog kind={dialog} clusters={clusters.data ?? []} policies={(policy.data ?? []).filter(p => p.policy_revision !== null)}
      record={selectedRow} disabled={disabled || (dialog === 'host' && !clusters.data?.some(c => c.cluster_id === selectedRow?.cluster_id))}
      pending={command.pending} send={command.send} close={() => { setDialog(null); setSelectedRow(null); }}/>}
    {policyDetails && <PolicyDetailsDialog selection={policyDetails} feeds={feeds} stale={stale} close={() => setPolicyDetails(null)}/>}
    {systemOpen && <SystemStatusDialog query={systemView} feeds={feeds} close={() => setSystemOpen(false)}/>}
  </main>;
}

function Empty({ loading }: { loading: boolean }) {
  return <p className="empty">{loading ? 'Waiting for data…' : 'No current data to display.'}</p>;
}
function Panel({ title, queryId, query, feeds, headerAction, children, onDecision }: {
  title?: string; queryId: QueryId; query: View; feeds: Views; headerAction?: React.ReactNode; children?: React.ReactNode; onDecision?: (id: string) => void;
}) {
  const render = (row: ResultRow) => {
    switch (queryId) {
      case 'ui-placements': return <PlacementEvidence row={row} views={feeds} onDecision={onDecision}/>;
      case 'ui-resilience': return <ResilienceEvidence row={row} views={feeds}/>;
      case 'ui-policy': return <PolicyEvidence row={row} views={feeds}/>;
      case 'ui-decisions': return <DecisionEvidence row={row} views={feeds}/>;
      case 'ui-timeline': return <TimelineEvidence row={row} onDecision={onDecision}
        previous={!!feeds['ui-status'].data?.[0] && row.observation_epoch !== feeds['ui-status'].data[0].observation_epoch}/>;
      case 'ui-status': return <StatusEvidence row={row} stale={query.stale || !!query.error || query.loading}/>;
      default: return null;
    }
  };
  const unavailable = query.stale || query.error || query.loading || !query.data;
  return <section className={`panel${title ? '' : ' inset-panel'}`}>{(title || unavailable || headerAction) && <div className="section-title">{title && <h3>{title}</h3>}{unavailable &&
    <span className={query.stale ? 'warning' : 'muted'}>{query.stale ? 'Out of date' : query.error ? 'Unavailable' : query.loading ? 'Loading' : 'Waiting for data'}</span>}{headerAction}</div>}
    {query.error && <div role="alert" className="error">{query.error.message}<button onClick={query.retry}>Retry updates</button></div>}
    <div className="panel-content">{query.data?.length ? query.data.map(row => <div key={rowKey(queryId, row)}>{render(row)}
      <details className="technical-details"><summary>Technical details</summary><p className="muted">{queryId} · {query.status}</p><pre>{JSON.stringify(row, null, 2)}</pre></details></div>) : <Empty loading={query.loading}/>}</div>
    {children}
  </section>;
}

function Activity({ feeds, connectionUnavailable, onDecision }: { feeds: Views; connectionUnavailable: boolean; onDecision: (id: string) => void }) {
  const source = feeds['ui-timeline'], epoch = feeds['ui-status'].data?.[0]?.observation_epoch;
  const data = source.data === null ? null : [...source.data].sort((a, b) => {
    if (epoch && (a.observation_epoch === epoch) !== (b.observation_epoch === epoch)) return a.observation_epoch === epoch ? -1 : 1;
    if (a.observation_epoch === b.observation_epoch) {
      const left = BigInt(text(a, 'event_sequence')), right = BigInt(text(b, 'event_sequence'));
      return left > right ? -1 : left < right ? 1 : 0;
    }
    return number(b, 'time_ms') - number(a, 'time_ms') || text(a, 'event_id').localeCompare(text(b, 'event_id'));
  });
  const query = { ...source, data, stale: source.stale || connectionUnavailable };
  const latest = data?.[0], outdated = query.stale || query.error || query.loading;
  return <details className="activity panel"><summary><strong>Activity</strong>{' · '}
    {latest ? <span className={outdated ? 'warning activity-latest' : 'activity-latest'} title={timelineMessage(latest)}>
      {outdated ? 'Last received · ' : epoch && latest.observation_epoch !== epoch ? 'Previous run · ' : ''}
      <time>{new Date(number(latest, 'time_ms')).toISOString().slice(11, 19)} UTC</time>{' · '}
      {label('event', text(latest, 'kind'))}: {timelineMessage(latest)}
    </span> : <span className="muted">{query.error || query.stale ? 'Activity unavailable' : query.loading || data === null ? 'Waiting for activity' : 'No activity recorded'}</span>}
  </summary>
    <Panel queryId="ui-timeline" query={query} feeds={feeds} onDecision={onDecision}/>
  </details>;
}

function systemSummary(query: View): { kind: string; note: string; detail: string } {
  const components = query.data?.flatMap(row => records(row, 'components')) ?? [];
  const working = ['pending', 'policy-pending', 'awaiting-policy', 'awaiting-source-bootstrap', 'awaiting-policy-bootstrap'];
  const current = ['ready', 'running', 'current', 'policy-current', 'plan-current', 'resilience-current', 'candidate-produced'];
  if (query.stale || query.error || query.loading || !components.length) return { kind: 'unknown', note: 'Unknown', detail: 'Current component status is unavailable.' };
  if (components.some(c => c.error || ['failed', 'stopped', 'unavailable'].includes(text(c, 'status')))) {
    return { kind: 'issue', note: 'Issue', detail: 'A reported component needs attention. Open system status for details.' };
  }
  if (components.some(c => ![...current, ...working, 'starting'].includes(text(c, 'status')))) {
    return { kind: 'unknown', note: 'Unknown', detail: 'Some components have not reported a ready or running state.' };
  }
  if (components.some(c => c.status === 'starting')) return { kind: 'starting', note: 'Starting', detail: 'Some reported components are starting.' };
  if (components.some(c => working.includes(text(c, 'status')))) {
    return { kind: 'starting', note: 'Working', detail: 'Components are processing or awaiting current inputs.' };
  }
  return { kind: 'reported', note: '', detail: `${components.length} reported components are ready, running or up to date. This is not GPU or replica health.` };
}

function AnalysisDrawer({ open, onOpenChange, returnFocus, feeds, stale, decisionRequest, onDecision }: {
  open: boolean; onOpenChange: (open: boolean) => void; returnFocus: React.RefObject<HTMLButtonElement>; feeds: Views; stale: boolean;
  decisionRequest: { id: string } | null; onDecision: (id: string) => void;
}) {
  const closeButton = useRef<HTMLButtonElement>(null);
  const decisionDetails = useRef<HTMLDetailsElement>(null);
  const missingDecision = useRef<HTMLParagraphElement>(null);
  const close = useCallback(() => {
    onOpenChange(false);
    returnFocus.current?.focus();
  }, [onOpenChange, returnFocus]);
  useEffect(() => {
    if (!open) return;
    if (!document.querySelector('dialog[open]')) closeButton.current?.focus();
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || document.querySelector('dialog[open]')) return;
      event.preventDefault();
      close();
    };
    document.addEventListener('keydown', escape);
    return () => document.removeEventListener('keydown', escape);
  }, [open, close]);
  useEffect(() => {
    if (!open || !decisionRequest || !decisionDetails.current) return;
    decisionDetails.current.open = true;
    const target = document.getElementById(`decision-${decisionRequest.id}`) ?? missingDecision.current;
    target?.focus({ preventScroll: true });
    target?.scrollIntoView({ block: 'nearest' });
  }, [open, decisionRequest]);
  const displayed = stale ? {
    ...feeds,
    'ui-placements': { ...feeds['ui-placements'], stale: true },
    'ui-resilience': { ...feeds['ui-resilience'], stale: true },
    'ui-decisions': { ...feeds['ui-decisions'], stale: true },
  } : feeds;
  return <aside id="global-analysis" className="analysis-drawer" hidden={!open} aria-labelledby="global-analysis-title">
    <div className="analysis-heading"><h2 id="global-analysis-title">Global analysis</h2>
      <button type="button" ref={closeButton} onClick={close} aria-label="Close global analysis">×</button>
    </div>
    <div className="analysis-content">
      <Panel title="Placement plan" queryId="ui-placements" query={displayed['ui-placements']} feeds={displayed} onDecision={onDecision}>
        <details className="decision-details" ref={decisionDetails}><summary>Decision details</summary>
          {decisionRequest && !displayed['ui-decisions'].data?.some(row => row.decision_id === decisionRequest.id) &&
            <p ref={missingDecision} tabIndex={-1} className="warning" role="status">The requested decision is not in the available history.</p>}
          <Panel queryId="ui-decisions" query={displayed['ui-decisions']} feeds={displayed}/>
        </details>
      </Panel>
      <Panel title="Recovery after failures" queryId="ui-resilience" query={displayed['ui-resilience']} feeds={displayed}/>
    </div>
  </aside>;
}

function useModalDialog() {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return dialog;
}

function SystemStatusDialog({ query, feeds, close }: { query: View; feeds: Views; close: () => void }) {
  const dialog = useModalDialog();
  return <dialog ref={dialog} aria-label="System status" className="modal system-status-dialog"
    onCancel={event => { event.preventDefault(); close(); }}>
    <Panel title="System status" queryId="ui-status" query={query} feeds={feeds}
      headerAction={<button type="button" onClick={close} aria-label="Close system status">×</button>}/>
    <p className="muted">Reported processing components, not GPU health or confirmation that all replicas are running.</p>
  </dialog>;
}

function PolicyDetailsDialog({ selection, feeds, stale, close }: {
  selection: { workloadId: string; clusterId: string }; feeds: Views; stale: boolean; close: () => void;
}) {
  const dialog = useModalDialog(), policy = { ...feeds['ui-policy'], stale: stale || feeds['ui-policy'].stale };
  const query = { ...policy, data: policy.data?.filter(p => p.workload_id === selection.workloadId && p.cluster_id === selection.clusterId) ?? null };
  return <dialog ref={dialog} aria-label="Policy details" className="modal policy-details-dialog"
    onCancel={event => { event.preventDefault(); close(); }}>
    <Panel title="Policy details" queryId="ui-policy" query={query} feeds={{ ...feeds, 'ui-policy': policy }}
      headerAction={<button type="button" onClick={close} aria-label="Close policy details">×</button>}/>
  </dialog>;
}

function CommandDialog({ kind, clusters, policies, record, disabled, pending, send, close }: {
  kind: 'workload' | 'host' | 'policy' | 'gpu' | 'workload-edit'; clusters: ResultRow[]; policies: ResultRow[];
  record: ResultRow | null; disabled: boolean; pending: boolean; send: Command; close: () => void;
}) {
  const [key] = useState(() => crypto.randomUUID());
  const [failure, setFailure] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState<{ path: string; body: unknown } | null>(null);
  const [policyId, setPolicyId] = useState(() => policies[0] ? text(policies[0], 'policy_id') : '');
  const [allowedRegions, setAllowedRegions] = useState(() => policies[0] ? strings(policies[0], 'allowed_regions') : []);
  const dialog = useModalDialog();
  const hostCluster = kind === 'host' ? clusters.find(c => c.cluster_id === record?.cluster_id) : undefined;
  const submit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault(); setFailure(null);
    if (disabled) { setFailure('Actions are unavailable until current data is ready.'); return; }
    const data = new FormData(event.currentTarget);
    let path: string, body: unknown;
    if (submitted) { path = submitted.path; body = submitted.body; }
    else if (kind === 'workload') {
      path = '/api/workloads'; body = { name: data.get('name'), profile_id: data.get('profile'), replicas: Number(data.get('replicas')),
        data_profile_id: data.get('data'), purpose: data.get('purpose'), spread_across_domains: data.get('spread') === 'on' };
    } else if (kind === 'host') {
      if (!hostCluster) { setFailure('The selected region is no longer available. Close this dialog and choose an available region.'); return; }
      path = '/api/hosts'; body = { host_id: data.get('name'), cluster_id: text(hostCluster, 'cluster_id'), hardware_profile_id: 'h100-nvl-pair-v1' };
    } else if (kind === 'gpu' && record) {
      path = `/api/gpus/${encodeURIComponent(text(record, 'gpu_id'))}/telemetry`;
      body = { expected_revision: text(record, 'telemetry_revision'), changes: {
        background_compute_units: Number(data.get('demand')), background_memory_mib: Number(data.get('memory')),
        interval_ms: Number(data.get('interval')),
      } };
    } else if (kind === 'workload-edit' && record) {
      path = `/api/workloads/${encodeURIComponent(text(record, 'workload_id'))}`;
      body = { expected_revision: text(record, 'revision'), changes: { name: data.get('name'),
        ...(data.get('profile') !== record.profile_id ? { profile_id: data.get('profile') } : {}),
        replicas: Number(data.get('replicas')), data_profile_id: data.get('data'), purpose: data.get('purpose'),
        spread_across_domains: data.get('spread') === 'on' } };
    } else {
      const policy = policies.find(p => p.policy_id === data.get('policy'));
      if (!policy || typeof policy.policy_revision !== 'string') { setFailure('Current policy revision is unavailable.'); return; }
      if (!window.confirm('Changing allowed regions can pause or stop workloads once the demo observes the change. Continue?')) return;
      path = `/api/policies/${encodeURIComponent(String(policy.policy_id))}`;
      body = { expected_revision: policy.policy_revision, changes: { allowed_regions: data.getAll('regions') } };
    }
    setSubmitted({ path, body });
    const creating = kind === 'workload' || kind === 'host';
    try { await send(path, creating ? 'POST' : 'PATCH', body, creating ? key : undefined); close(); }
    catch (error) { setFailure(error instanceof Error ? error.message : String(error)); }
  };
  return <dialog ref={dialog} onCancel={event => { event.preventDefault(); if (!pending) close(); }} aria-labelledby="dialog-title" className="modal">
    <div className="section-title"><h3 id="dialog-title">{kind === 'host' ? 'Add ready VM' : kind === 'policy' ? 'Edit processing policy' : kind === 'gpu' ? 'Edit background load' : kind === 'workload-edit' ? 'Edit workload' : 'Add workload'}</h3><button disabled={pending} onClick={close} aria-label="Close dialog">×</button></div>
    <form onSubmit={event => { void submit(event); }}><fieldset disabled={disabled || submitted !== null}>
      {kind !== 'policy' && kind !== 'gpu' && <label>Name<input name="name" required maxLength={256} autoFocus defaultValue={kind === 'workload-edit' && record ? text(record, 'name') : ''}/></label>}
      {kind === 'host' && <>{hostCluster
        ? <p className="host-region">Region: <strong>{text(hostCluster, 'name')} / {label('region', text(hostCluster, 'region'))}</strong></p>
        : <p role="alert" className="error">The selected region is no longer available. Close this dialog and choose an available region.</p>}
        <p>Adds a simulated VM that is already ready to use: two H100 NVL GPUs, with an 80 GiB demo memory budget each. It does not create a real Azure VM.</p></>}
      {(kind === 'workload' || kind === 'workload-edit') && <>
        <label>Model and resources per replica<select name="profile" defaultValue={kind === 'workload-edit' && record ? text(record, 'profile_id') : 'assistant-v1'}><option value="assistant-v1">Qwen2.5-32B assistant · 76 GiB / 55 demand units</option><option value="chat-v1">Llama3.1-8B chat · 24 GiB / 30 demand units</option><option value="embeddings-v1">BGE embeddings · 4 GiB / 20 demand units</option><option value="reranker-v1">BGE reranker · 4 GiB / 25 demand units</option>{record?.profile_id === 'custom' && <option value="custom">Existing custom resources (edit resources in the database)</option>}</select></label>
        <p>These are illustrative resource requirements, not measured model benchmarks.</p>
        <label>Replicas (copies of this service)<input name="replicas" type="number" min={0} max={32} defaultValue={kind === 'workload-edit' && record ? number(record, 'replicas') : 2} required/></label>
        <label>Data being processed<select name="data" defaultValue={kind === 'workload-edit' && record ? text(record, 'data_profile_id') : 'demo-open'}><option value="demo-open">Synthetic demo data</option><option value="customer-eu-documents">Example customer EU documents</option></select></label>
        <label>Purpose<select name="purpose" defaultValue={kind === 'workload-edit' && record ? text(record, 'purpose') : 'demo'}><option value="demo">Demo</option><option value="customer-support">Customer support</option></select></label>
        <label className="checkbox"><input name="spread" type="checkbox" defaultChecked={kind === 'workload-edit' && record ? boolean(record, 'spread_across_domains') : true}/>Place replicas on different VMs</label>
      </>}
      {kind === 'gpu' && record && <>
        <p>Background load represents other simulated activity on this GPU, separate from the workloads listed in the demo.</p>
        <label>Background compute demand (demand units)<input autoFocus name="demand" type="number" min={0} max={200} step={1} required defaultValue={number(record, 'background_compute_units')}/></label>
        <label>Background memory requested (MiB; 1,024 MiB = 1 GiB)<input name="memory" type="number" min={0} max={81920} step={1} required defaultValue={number(record, 'background_memory_mib')}/></label>
        <label>Report interval (milliseconds)<input name="interval" type="number" min={250} max={2000} step={1} required defaultValue={number(record, 'interval_ms')}/></label>
        <p>Saving changes updates the simulator settings. A later GPU report confirms their effect; saving alone does not.</p>
      </>}
      {kind === 'policy' && <><label>Policy<select name="policy" value={policyId} onChange={e => {
        setPolicyId(e.target.value);
        const selected = policies.find(p => p.policy_id === e.target.value);
        setAllowedRegions(selected ? strings(selected, 'allowed_regions') : []);
      }}>{[...new Set(policies.flatMap(p => typeof p.policy_id === 'string' ? [p.policy_id] : []))].map(p => <option key={p} value={p}>{label('policy', p)}</option>)}</select></label>
        <p>If policy information is unavailable, processing pauses and memory stays reserved. If a region is not allowed, processing stops and releases resources once the stop is confirmed.</p>
        <label className="checkbox"><input type="checkbox" name="regions" value="*" checked={allowedRegions.includes('*')}
          onChange={e => setAllowedRegions(e.target.checked ? ['*'] : [])}/>Allow all regions</label>
        {['westeurope', 'northeurope', 'eastus'].map(r => <label key={r} className="checkbox"><input type="checkbox" name="regions" value={r}
          disabled={allowedRegions.includes('*')} checked={allowedRegions.includes(r)}
          onChange={e => setAllowedRegions(e.target.checked ? [...allowedRegions, r] : allowedRegions.filter(v => v !== r))}/>{label('region', r)}</label>)}
        <p>{allowedRegions.length ? 'Current settings are shown. Clear "Allow all regions" to choose individual regions.' : 'No regions selected: processing will not be allowed anywhere.'}</p></>}
    </fieldset>{failure && <p className="error" role="alert">{failure}</p>}
      {disabled && !pending && <p role="status">Actions are unavailable until current data is ready. You can close this dialog without changing the fleet.</p>}
      {submitted && <p>Retry sends the same request with the same request ID. Close this dialog to make a different change.</p>}
      <button className="primary" type="submit" disabled={disabled}>{pending ? 'Saving…' : submitted ? 'Retry same change' : 'Save change'}</button></form>
  </dialog>;
}
