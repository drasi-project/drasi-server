import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useDrasiClient, useDrasiConnectionStatus, useDrasiQuery } from '@drasi/react/react';
import type { ResultRow } from '@drasi/react/client';
import { rowKey, validateRow, text, number, boolean, strings, records, type QueryId } from './rows';
import { DecisionEvidence, PlacementEvidence, PolicyEvidence, ResilienceEvidence, StatusEvidence,
  TimelineEvidence, timelineMessage, clusterReplicaCounts, correlated, count, currentFeeds, policyCurrent } from './Evidence';
import { componentTone, label, queryRetrying, reportTone } from './labels';
import { GpuCard } from './GpuCard';
import { WorkloadsPanel } from './WorkloadsPanel';
import { HierarchyIcon, PresenterControls, usePresenterHighlight } from './Hierarchy';
import { buildReplicaView } from './replicas';
import { useReplicaMotion } from './replicaMotion';
import { ArchitectureOverlay } from './Architecture';
import { WorkloadPolicyBinding, WorkloadPolicyContext, policyMetadata } from './WorkloadPolicy';
import { DataPolicyPanel, dataPolicyState, editorRules, ruleWorkloads, RuleCriteria } from './DataPolicy';

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
    reconnecting: online && transport.reconnecting,
    error: !online ? new Error('Browser is offline') : connection.error ?? transport.error ?? null,
  }} command={command}/>;
}

export function Lab({ views: feeds, connection, command, demoPanel }: {
  views: Views;
  connection: { initialized: boolean; error: Error | null; reconnecting?: boolean; retry: () => void };
  command: Commands;
  demoPanel?: React.ReactNode;
}) {
  const { 'ui-gpus': gpus, 'ui-workloads': workloads, 'ui-placements': placements,
    'ui-status': status, 'ui-clusters': clusters, 'ui-policy': policy } = feeds;
  const views = Object.values(feeds);
  const [dialog, setDialog] = useState<'workload' | 'host' | 'policy' | 'gpu' | 'workload-edit' | null>(null);
  const [selectedRow, setSelectedRow] = useState<ResultRow | null>(null);
  const workloadEditorButton = useRef<HTMLButtonElement | null>(null);
  const [selectedWorkload, selectWorkload] = useState<string | null>(null);
  const [policyDetails, setPolicyDetails] = useState<{ workloadId: string; clusterId?: string } | null>(null);
  const policyButton = useRef<HTMLElement | null>(null);
  const dataPolicyPanel = useRef<HTMLElement>(null);
  const [sidePanel, setSidePanel] = useState<'analysis' | 'policy' | null>(null);
  const analysisOpen = sidePanel === 'analysis', dataPolicyOpen = sidePanel === 'policy';
  const setAnalysisOpen = useCallback((open: boolean) => setSidePanel(open ? 'analysis' : null), []);
  const setDataPolicyOpen = useCallback((open: boolean) => setSidePanel(open ? 'policy' : null), []);
  const dataPolicyButton = useRef<HTMLButtonElement>(null);
  const [decisionRequest, setDecisionRequest] = useState<{ id: string } | null>(null);
  const [systemOpen, setSystemOpen] = useState(false);
  const [architectureOpen, setArchitectureOpen] = useState(false);
  const architectureButton = useRef<HTMLButtonElement>(null);
  const appRoot = useRef<HTMLElement>(null);
  const presenter = usePresenterHighlight(appRoot);
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
  const replicaView = buildReplicaView(feeds, connectionUnavailable);
  const [motionEnabled, setMotionEnabled] = useState(true);
  const replicaCues = useReplicaMotion(appRoot, replicaView, motionEnabled);
  const stale = !currentFeeds(feeds) || connectionUnavailable;
  const failedUpdates = (!!connection.error && !connection.reconnecting)
    || views.some(view => view.error && !queryRetrying(view.status));
  const systemView = { ...status, stale: status.stale || connectionUnavailable };
  const system = systemSummary(systemView);
  const disabled = command.pending || stale || !inputsReady;
  const run = (action: Promise<void>) => { void action.catch(() => { /* useCommand renders the failure. */ }); };
  const gpuRows = gpus.data ?? [];
  const patchGpu = (gpu: ResultRow, changes: Record<string, unknown>) => run(command.send(
    `/api/gpus/${encodeURIComponent(text(gpu, 'gpu_id'))}/telemetry`, 'PATCH',
    { expected_revision: text(gpu, 'telemetry_revision'), changes }));
  const activeWorkload = workloads.data?.find(w => w.workload_id === selectedWorkload);
  const openPolicy = (workloadId: string, trigger: HTMLElement, clusterId?: string) => {
    policyButton.current = trigger;
    setPolicyDetails({ workloadId, clusterId });
  };
  const editPolicy = (row: ResultRow | null = null) => {
    if (!policyDetails) policyButton.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setPolicyDetails(null); setSelectedRow(row); setDialog('policy');
  };
  const showDataPolicy = () => {
    setPolicyDetails(null);
    setDataPolicyOpen(true);
    requestAnimationFrame(() => {
      dataPolicyPanel.current?.focus({ preventScroll: true });
      dataPolicyPanel.current?.scrollIntoView({ block: 'start' });
    });
  };
  const setRegionPower = (region: string, members: ResultRow[], powered_on: boolean) => run(command.send(
    `/api/regions/${encodeURIComponent(region)}/telemetry`, 'PATCH',
    { gpu_revisions: Object.fromEntries(members.map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on } }));
  return <main className="lab-app lab-page" ref={appRoot} data-presenter-highlight={presenter.target ?? undefined}>
    <header className="lab-header">
      <div className="lab-title"><h1>GPU Cluster Lab</h1>
        <button type="button" ref={architectureButton} className="architecture-toggle"
          aria-label="Information about this demo" title="Architecture Overview" aria-haspopup="dialog"
          aria-expanded={architectureOpen} aria-controls={architectureOpen ? 'gpu-architecture-overlay' : undefined}
          onClick={() => setArchitectureOpen(true)}><span aria-hidden="true">i</span></button>
      </div>
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
        title="View fleet-wide placement progress and recovery assessment" onClick={() => { setDecisionRequest(null); setAnalysisOpen(!analysisOpen); }}>Global analysis</button>
      <button type="button" ref={dataPolicyButton} className="analysis-toggle" aria-controls="data-policy" aria-expanded={dataPolicyOpen}
        title="View and edit the shared data-policy rules" onClick={() => setDataPolicyOpen(!dataPolicyOpen)}>Data policy</button>
      <details className="reading-guide" onToggle={event => { if (!event.currentTarget.open) presenter.clear(); }}><summary>Help</summary><div>
      <PresenterControls presenter={presenter}/>
      <label className="replica-motion-setting"><input type="checkbox" checked={motionEnabled} onChange={event => setMotionEnabled(event.target.checked)}/>
        Animate observed replica changes</label>
      <details className="domain-guide"><summary>How to read this demo</summary><div>
      <p><strong>Fleet → region → VM → GPU.</strong> The fleet is the whole demo. Each regional cluster groups VMs in one region.
        A GPU VM (virtual machine) models Standard_NC80adis_H100_v5 with two H100 NVL GPUs. Losing a VM loses both GPUs. Different VMs do not imply different physical hosts or availability zones.</p>
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
      <p><strong>Status colors.</strong> Red identifies off, stopped, overdue, denied, or failed states.
        Amber identifies pending, paused, unknown, or partially complete states. Green identifies current successful evidence.
        Report freshness and power are separate: an off GPU can still have a recent last report.</p>
      <p><strong>Regional policy.</strong> The policy represents an example customer agreement, not a general GDPR requirement.
        This local simulation does not keep real data in the named regions.</p>
      <p>After a connection interruption, displayed information may remain out of date until a fresh snapshot arrives.</p>
      </div></details>
      </div></details>
      </div>
    </header>
    {architectureOpen && <ArchitectureOverlay onClose={() => setArchitectureOpen(false)} returnFocus={architectureButton}/>}
    {demoPanel}
    {(connection.error || views.some(v => v.error || v.stale)) && <section className={`status-alert ${failedUpdates ? 'error' : 'warning'}`} role="alert">
      <strong>Updates are unavailable or out of date. Previously received data does not confirm current policy permission.</strong>
      <p>{connection.error?.message ?? views.find(v => v.error)?.error?.message ?? 'Waiting for the latest data.'}</p>
      <button onClick={connection.retry}>Reconnect updates</button>
      {Object.entries(feeds).filter(([, v]) => v.error || v.stale).map(([id, v]) => <p key={id}>{label('query', id)}: {v.error?.message ?? 'Out-of-date data'}
        <button onClick={v.retry}>Retry {label('query', id).toLowerCase()} updates</button></p>)}
    </section>}
    {command.error && <div className="error" role="alert">{command.error}</div>}
    {command.notice && <div className="notice" role="status">{command.notice}</div>}
    <div className={`lab-layout${sidePanel ? ' analysis-open' : ''}${dataPolicyOpen ? ' policy-open' : ''}${analysisOpen && decisionRequest ? ' decision-navigation' : ''}`}>
    <div className="lab-content">
    <WorkloadsPanel query={workloads} views={feeds} replicaView={replicaView} stale={stale} disabled={disabled} selectedWorkload={activeWorkload ? text(activeWorkload, 'workload_id') : null}
      onToggleWorkload={toggleWorkload} onAdd={() => setDialog('workload')}
      onInspectPolicy={(workload, trigger) => openPolicy(text(workload, 'workload_id'), trigger)}
      onEdit={(w, trigger) => { workloadEditorButton.current = trigger; setSelectedRow(w); setDialog('workload-edit'); }}
      onDelete={w => {
        if (window.confirm(`Delete ${text(w, 'name')}? Its replicas will stop after the demo processes this change.`)) {
          run(command.send(`/api/workloads/${encodeURIComponent(text(w, 'workload_id'))}`, 'DELETE', undefined, undefined, text(w, 'revision')));
        }
      }}/>
    <section className="fleet" aria-label="Regions">
      {!clusters.data?.length && <Empty loading={clusters.loading}/>}
      {(clusters.data ?? []).map(cluster => {
        const rows = gpuRows.filter(g => g.cluster_id === cluster.cluster_id);
        const regionRows = gpuRows.filter(g => clusters.data?.some(c => c.cluster_id === g.cluster_id && c.region === cluster.region));
        const hosts = [...new Set(rows.map(g => text(g, 'host_id')))];
        const replicas = stale ? { planned: null, running: null, ready: null } : clusterReplicaCounts(feeds, text(cluster, 'cluster_id'));
        return <article className="cluster" key={text(cluster, 'cluster_id')}
          data-hierarchy-kind="region" data-hierarchy-id={text(cluster, 'cluster_id')}>
          <div className="cluster-title"><div className="region-heading"><span className="region-label"><HierarchyIcon kind="region"/>REGION</span>
            <h3>{text(cluster, 'name')} <small>· {label('region', text(cluster, 'region'))}</small></h3></div>
            <button disabled={disabled} aria-label={`Add ready VM to ${text(cluster, 'name')}`}
              onClick={() => { setSelectedRow(cluster); setDialog('host'); }}>Add ready VM</button>
            <button disabled={disabled || !rows.length} onClick={() => {
              if (window.confirm('Simulate every VM in this region losing power? Processing stops. Placement decisions detect the loss after GPU reports have been missing for five seconds.')) setRegionPower(text(cluster, 'region'), regionRows, false);
            }}>Simulate region failure</button>
            <button disabled={disabled || !regionRows.length} onClick={() => setRegionPower(text(cluster, 'region'), regionRows, true)}>Restore region</button></div>
          <details className="cluster-body" data-hierarchy-container="region"><summary><span className="cluster-summary">
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
                {hosts.map(host => <span className="vm-preview" key={host} data-hierarchy-kind="vm" data-hierarchy-id={host}>
                  <span className="vm-preview-title"><HierarchyIcon kind="vm"/><span className="hierarchy-type">VM</span><strong>{host}</strong></span><span className="vm-preview-gpus">
                  {rows.filter(g => g.host_id === host).map(g => {
                    const health = stale ? 'unknown' : text(g, 'health');
                    const settings = stale ? [] : [
                      ...(!boolean(g, 'powered_on') ? ['Power off'] : []),
                      ...(!boolean(g, 'reporting_enabled') ? ['Reports paused'] : []),
                      ...(!boolean(g, 'scheduling_enabled') ? ['Excluded from plans'] : []),
                    ];
                    const description = `${text(g, 'name')}: ${label('health', health)}${settings.length ? ` · ${settings.join(' · ')}` : ''}${stale ? ' · last received inventory' : ''}`;
                    const tone = stale ? 'warning' : !boolean(g, 'powered_on') || health === 'unreachable' ? 'danger'
                      : !boolean(g, 'reporting_enabled') || !boolean(g, 'scheduling_enabled') ? 'warning' : reportTone(health);
                    return <span key={text(g, 'gpu_id')} className={`mini-gpu report-${health} ${tone}`} role="img" aria-label={description} title={description}
                      data-hierarchy-kind="gpu" data-hierarchy-id={text(g, 'gpu_id')}>
                      <HierarchyIcon kind="gpu"/>GPU {number(g, 'slot')}{settings.length > 0 && <span className="mini-gpu-setting"> · {settings[0]}</span>}
                    </span>;
                  })}
                </span></span>)}</> : <span className="muted">No VMs registered</span>}
            </span>
          </span></summary>
          <div className="vm-grid">{hosts.map(host => <details className="worker" key={host} open data-hierarchy-kind="vm" data-hierarchy-id={host} data-hierarchy-container="vm">
            <summary className="section-title vm-heading"><h4><HierarchyIcon kind="vm"/><span className="hierarchy-type">VM</span> {host}
              <small> · {rows.filter(g => g.host_id === host).length} GPU{rows.filter(g => g.host_id === host).length === 1 ? '' : 's'}</small></h4><HierarchyIcon kind="chevron"/></summary>
            <div className="vm-controls"><button disabled={disabled} onClick={() => run(command.send(`/api/hosts/${encodeURIComponent(host)}/telemetry`, 'PATCH',
              { gpu_revisions: Object.fromEntries(rows.filter(g => g.host_id === host).map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on: false } }))}>Power off VM</button>
            <button disabled={disabled} onClick={() => run(command.send(`/api/hosts/${encodeURIComponent(host)}/telemetry`, 'PATCH',
              { gpu_revisions: Object.fromEntries(rows.filter(g => g.host_id === host).map(g => [text(g, 'gpu_id'), text(g, 'telemetry_revision')])), changes: { powered_on: true } }))}>Restore VM</button></div>
            <div className="gpu-grid">{rows.filter(g => g.host_id === host).map(g => {
              const pair = policy.data?.find(p => p.workload_id === activeWorkload?.workload_id && p.cluster_id === g.cluster_id);
              const authorization = pair && policyCurrent(pair, feeds, stale) ? pair.authorization : 'unknown';
              return <GpuCard key={text(g, 'gpu_id')} gpu={g} replicaView={replicaView} cues={replicaCues}
                selectedWorkload={activeWorkload ? text(activeWorkload, 'workload_id') : null} onToggleWorkload={toggleWorkload}
                stale={stale}
                permission={activeWorkload ? {
                  workloadName: text(activeWorkload, 'name'), authorization: String(authorization),
                  inspect: trigger => openPolicy(text(activeWorkload, 'workload_id'), trigger, text(g, 'cluster_id')),
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
            })}</div></details>)}</div>
          {!hosts.length && <p className="muted">No VMs in this regional cluster.</p>}
          </details>
        </article>;
      })}
    </section>
    <Activity feeds={feeds} connectionUnavailable={connectionUnavailable} onDecision={openDecision}/>
    </div>
    <AnalysisDrawer open={analysisOpen} onOpenChange={setAnalysisOpen} returnFocus={analysisButton} feeds={feeds} stale={stale}
      decisionRequest={decisionRequest} onDecision={openDecision}/>
    <SideDrawer id="data-policy" title="Data policy" open={dataPolicyOpen} onOpenChange={setDataPolicyOpen}
      returnFocus={dataPolicyButton} focusRef={dataPolicyPanel}>
      <DataPolicyPanel views={feeds} stale={stale} disabled={disabled} onEdit={editPolicy}/>
    </SideDrawer>
    </div>
    {dialog && <CommandDialog kind={dialog} clusters={clusters.data ?? []} views={feeds}
      returnFocus={dialog === 'policy' ? policyButton : dialog === 'workload-edit' ? workloadEditorButton : undefined}
      record={selectedRow} disabled={disabled || (dialog === 'host' && !clusters.data?.some(c => c.cluster_id === selectedRow?.cluster_id))}
      pending={command.pending} send={command.send} close={() => { setDialog(null); setSelectedRow(null); }}/>}
    {policyDetails && <PolicyDetailsDialog selection={policyDetails} feeds={feeds} stale={stale}
      returnFocus={policyButton} onShowRules={showDataPolicy} onShowAll={() => setPolicyDetails({ workloadId: policyDetails.workloadId })}
      close={() => setPolicyDetails(null)}/>}
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
  const failed = !!query.error && !queryRetrying(query.status);
  return <section className={`panel${title ? '' : ' inset-panel'}`}>{(title || unavailable || headerAction) && <div className="section-title">{title && <h3>{title}</h3>}{unavailable &&
    <span className={failed ? 'danger' : 'warning'}>{query.error ? queryRetrying(query.status) ? 'Retrying' : 'Unavailable' : query.stale ? 'Out of date' : query.loading ? 'Loading' : 'Waiting for data'}</span>}{headerAction}</div>}
    {query.error && <div role="alert" className={failed ? 'error' : 'warning'}>{query.error.message}<button onClick={query.retry}>Retry updates</button></div>}
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
  if (components.some(c => componentTone(text(c, 'status'), !!c.error) === 'danger')) {
    return { kind: 'issue', note: 'Issue', detail: 'A reported component needs attention. Open system status for details.' };
  }
  if (components.some(c => componentTone(text(c, 'status'), false) !== 'good' && ![...current, ...working, 'starting'].includes(text(c, 'status')))) {
    return { kind: 'unknown', note: 'Unknown', detail: 'Some components have not reported a ready or running state.' };
  }
  if (components.some(c => c.status === 'starting')) return { kind: 'starting', note: 'Starting', detail: 'Some reported components are starting.' };
  if (components.some(c => working.includes(text(c, 'status')))) {
    return { kind: 'starting', note: 'Working', detail: 'Components are processing or awaiting current inputs.' };
  }
  return { kind: 'reported', note: '', detail: `${components.length} reported components are ready, running or up to date. This is not GPU or replica health.` };
}

function SideDrawer({ id, title, open, onOpenChange, returnFocus, focusRef, children }: {
  id: string; title: string; open: boolean; onOpenChange: (open: boolean) => void;
  returnFocus: React.RefObject<HTMLButtonElement>; focusRef?: React.RefObject<HTMLElement>; children: React.ReactNode;
}) {
  const closeButton = useRef<HTMLButtonElement>(null);
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
  return <aside id={id} ref={focusRef} tabIndex={-1} className="analysis-drawer" hidden={!open} aria-labelledby={`${id}-title`}>
    <div className="analysis-heading"><h2 id={`${id}-title`}>{title}</h2>
      <button type="button" ref={closeButton} onClick={close} aria-label={`Close ${title.toLowerCase()}`}>×</button>
    </div>
    <div className="analysis-content">{children}</div>
  </aside>;
}

function AnalysisDrawer({ open, onOpenChange, returnFocus, feeds, stale, decisionRequest, onDecision }: {
  open: boolean; onOpenChange: (open: boolean) => void; returnFocus: React.RefObject<HTMLButtonElement>; feeds: Views; stale: boolean;
  decisionRequest: { id: string } | null; onDecision: (id: string) => void;
}) {
  const decisionDetails = useRef<HTMLDetailsElement>(null);
  const missingDecision = useRef<HTMLParagraphElement>(null);
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
  return <SideDrawer id="global-analysis" title="Global analysis" open={open} onOpenChange={onOpenChange} returnFocus={returnFocus}>
      <Panel title="Placement plan" queryId="ui-placements" query={displayed['ui-placements']} feeds={displayed} onDecision={onDecision}>
        <details className="decision-details" ref={decisionDetails}><summary>Decision details</summary>
          {decisionRequest && !displayed['ui-decisions'].data?.some(row => row.decision_id === decisionRequest.id) &&
            <p ref={missingDecision} tabIndex={-1} className="warning" role="status">The requested decision is not in the available history.</p>}
          <Panel queryId="ui-decisions" query={displayed['ui-decisions']} feeds={displayed}/>
        </details>
      </Panel>
      <Panel title="Recovery after failures" queryId="ui-resilience" query={displayed['ui-resilience']} feeds={displayed}/>
  </SideDrawer>;
}

function useModalDialog(returnFocus?: React.RefObject<HTMLElement | null>) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => {
      element?.close();
      if (returnFocus?.current?.isConnected) returnFocus.current.focus({ preventScroll: true });
    };
  }, [returnFocus]);
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

function PolicyDetailsDialog({ selection, feeds, stale, returnFocus, onShowRules, onShowAll, close }: {
  selection: { workloadId: string; clusterId?: string }; feeds: Views; stale: boolean;
  returnFocus: React.RefObject<HTMLElement | null>; onShowRules: () => void; onShowAll: () => void; close: () => void;
}) {
  const dialog = useModalDialog(returnFocus), policy = { ...feeds['ui-policy'], stale: stale || feeds['ui-policy'].stale };
  const workload = feeds['ui-workloads'].data?.find(w => w.workload_id === selection.workloadId);
  const destinationOrder = new Map((feeds['ui-clusters'].data ?? []).map((cluster, index) => [text(cluster, 'cluster_id'), index]));
  const query = { ...policy, data: policy.data?.filter(p => p.workload_id === selection.workloadId
    && (selection.clusterId === undefined || p.cluster_id === selection.clusterId)).sort((left, right) =>
      (destinationOrder.get(text(left, 'cluster_id')) ?? destinationOrder.size) - (destinationOrder.get(text(right, 'cluster_id')) ?? destinationOrder.size)) ?? null };
  const missing = selection.clusterId === undefined ? (feeds['ui-clusters'].data ?? []).filter(c => !query.data?.some(p => p.cluster_id === c.cluster_id)) : [];
  return <dialog ref={dialog} aria-label="Policy details" className="modal policy-details-dialog"
    onCancel={event => { event.preventDefault(); close(); }}>
    <div className="section-title"><h3>Policy &amp; allocation{workload && `: ${text(workload, 'name')}`}</h3>
      <button type="button" onClick={close} aria-label="Close policy details">×</button></div>
    {workload ? <WorkloadPolicyContext workload={workload} views={feeds} stale={stale} onShowRules={onShowRules}/>
      : <p className="status-text warning">This workload is no longer in the available configuration.</p>}
    {selection.clusterId && <button type="button" onClick={onShowAll}>Show all destinations for this workload</button>}
    <Panel title="Policy decisions by destination" queryId="ui-policy" query={query} feeds={{ ...feeds, 'ui-policy': policy }}/>
    {missing.map(cluster => <p className="status-text warning" key={text(cluster, 'cluster_id')}>
      {text(cluster, 'name')} / {label('region', text(cluster, 'region'))}: policy result unavailable.</p>)}
    <p>The saved plan can lag a policy change. A denial stops affected processing; unknown permission pauses it.
      Running and confirmed counts show the observed result, separately from permission.</p>
  </dialog>;
}

function CommandDialog({ kind, clusters, views, returnFocus, record, disabled, pending, send, close }: {
  kind: 'workload' | 'host' | 'policy' | 'gpu' | 'workload-edit'; clusters: ResultRow[]; views: Views;
  returnFocus?: React.RefObject<HTMLElement | null>;
  record: ResultRow | null; disabled: boolean; pending: boolean; send: Command; close: () => void;
}) {
  const policies = editorRules(views), catalog = dataPolicyState(views);
  const policyIds = [...new Set(policies.map(policy => text(policy, 'policy_id')))];
  const workloads = views['ui-workloads'].data ?? [];
  const dataIds = catalog.profiles?.map(profile => text(profile, 'data_profile_id'))
    ?? [...new Set(workloads.map(workload => text(workload, 'data_profile_id')))];
  const [key] = useState(() => crypto.randomUUID());
  const [failure, setFailure] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState<{ path: string; body: unknown } | null>(null);
  const initialPolicy = record?.policy_id ? policies.find(p => p.policy_id === record.policy_id)
    : policies.find(p => ruleWorkloads(text(p, 'policy_id'), views).length > 0) ?? policies[0];
  const [policyId, setPolicyId] = useState(() => record?.policy_id ? text(record, 'policy_id') : initialPolicy ? text(initialPolicy, 'policy_id') : '');
  const [policyRevision, setPolicyRevision] = useState(() => initialPolicy ? text(initialPolicy, 'policy_revision') : null);
  const [allowedRegions, setAllowedRegions] = useState(() => initialPolicy ? strings(initialPolicy, 'allowed_regions') : []);
  const [dataId, setDataId] = useState(() => record?.data_profile_id ? text(record, 'data_profile_id')
    : workloads[0] ? text(workloads[0], 'data_profile_id') : dataIds.includes('demo-open') ? 'demo-open' : dataIds[0] ?? '');
  const [purpose, setPurpose] = useState(() => record?.purpose ? text(record, 'purpose')
    : workloads[0] ? text(workloads[0], 'purpose') : 'demo');
  const dialog = useModalDialog(returnFocus);
  const affected = ruleWorkloads(policyId, views);
  const selectedRule = catalog.rules?.find(rule => rule.policy_id === policyId);
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
      const policy = policyMetadata(policies.filter(p => p.policy_id === policyId));
      if (!policy || typeof policy.policy_revision !== 'string') { setFailure('Current policy revision is unavailable.'); return; }
      if (policy.policy_revision !== policyRevision) { setFailure('This policy changed while the editor was open. Close and reopen it to review the latest rules.'); return; }
      if (!window.confirm(`Change shared data-policy region rules for ${affected.map(w => text(w, 'name')).join(', ') || 'this data scope'}? This can pause or stop their processing. Continue?`)) return;
      path = `/api/policies/${encodeURIComponent(String(policy.policy_id))}`;
      body = { expected_revision: policy.policy_revision, changes: { allowed_regions: data.getAll('regions') } };
    }
    setSubmitted({ path, body });
    const creating = kind === 'workload' || kind === 'host';
    try { await send(path, creating ? 'POST' : 'PATCH', body, creating ? key : undefined); close(); }
    catch (error) { setFailure(error instanceof Error ? error.message : String(error)); }
  };
  return <dialog ref={dialog} onCancel={event => { event.preventDefault(); if (!pending) close(); }} aria-labelledby="dialog-title" className="modal">
    <div className="section-title"><h3 id="dialog-title">{kind === 'host' ? 'Add ready VM' : kind === 'policy' ? 'Edit shared data policy' : kind === 'gpu' ? 'Edit background load' : kind === 'workload-edit' ? 'Edit workload' : 'Add workload'}</h3><button disabled={pending} onClick={close} aria-label="Close dialog">×</button></div>
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
        <label>Data being processed<select name="data" required value={dataId} onChange={event => setDataId(event.target.value)}>
          {!dataIds.includes(dataId) && <option value={dataId}>{dataId ? `${label('data', dataId)} (unavailable)` : 'Data profiles unavailable'}</option>}
          {dataIds.map(id => {
            const profile = catalog.profiles?.find(profile => profile.data_profile_id === id);
            return <option value={id} key={id}>{label('data', id)}{profile ? ` · ${label('classification', text(profile, 'classification'))}` : ''}</option>;
          })}</select></label>
        <label>Purpose<select name="purpose" value={purpose} onChange={event => setPurpose(event.target.value)}><option value="demo">Demo</option><option value="customer-support">Customer support</option></select></label>
        <WorkloadPolicyBinding dataId={dataId} purpose={purpose} views={views}/>
        <label className="checkbox"><input name="spread" type="checkbox" defaultChecked={kind === 'workload-edit' && record ? boolean(record, 'spread_across_domains') : true}/>Place replicas on different VMs</label>
      </>}
      {kind === 'gpu' && record && <>
        <p>Background load represents other simulated activity on this GPU, separate from the workloads listed in the demo.</p>
        <label>Background compute demand (demand units)<input autoFocus name="demand" type="number" min={0} max={200} step={1} required defaultValue={number(record, 'background_compute_units')}/></label>
        <label>Background memory requested (MiB; 1,024 MiB = 1 GiB)<input name="memory" type="number" min={0} max={81920} step={1} required defaultValue={number(record, 'background_memory_mib')}/></label>
        <label>Report interval (milliseconds)<input name="interval" type="number" min={250} max={2000} step={1} required defaultValue={number(record, 'interval_ms')}/></label>
        <p>Saving changes updates the simulator settings. A later GPU report confirms their effect; saving alone does not.</p>
      </>}
      {kind === 'policy' && <>{policyIds.length > 1 && <label>Shared rule scope<select name="policy" value={policyId} onChange={e => {
        setPolicyId(e.target.value);
        const selected = policies.find(p => p.policy_id === e.target.value);
        setPolicyRevision(selected ? text(selected, 'policy_revision') : null);
        setAllowedRegions(selected ? strings(selected, 'allowed_regions') : []);
      }}>{policyId && !policies.some(p => p.policy_id === policyId) && <option value={policyId}>{label('policy', policyId)} (unavailable)</option>}
        {policyIds.map(p => <option key={p} value={p}>
          {catalog.profiles?.filter(profile => profile.policy_id === p).map(profile => label('data', text(profile, 'data_profile_id'))).join(', ') || label('policy', p)}
        </option>)}</select></label>}
        <p className="policy-edit-scope"><strong>Shared rules, not a workload override.</strong> These region rules apply to:{' '}
          {affected.length ? affected.map(w => text(w, 'name')).join(', ') : 'no currently observed workloads'}.
          {' '}All workloads in this data scope are re-evaluated after saving; other rule scopes are unchanged.</p>
        {selectedRule ? <><h4>Saved rule conditions</h4><RuleCriteria rule={selectedRule}/></>
          : <p className="warning">Full customer, classification and purpose criteria are unavailable in this runtime.</p>}
        <p>This editor changes permitted destinations for the shared rule. Customer, classification and purpose conditions remain unchanged.</p>
        <p>Allowing a region makes its GPUs eligible for consideration; it does not assign workloads there.
          Drasi still evaluates data and purpose, and the optimizer checks capacity and replica separation.</p>
        <p>If policy information is unavailable, processing pauses and memory stays reserved. If a region is not allowed, processing stops and releases resources once the stop is confirmed.</p>
        <label className="checkbox"><input type="checkbox" name="regions" value="*" checked={allowedRegions.includes('*')}
          disabled={policyId !== 'demo-permissive'}
          onChange={e => setAllowedRegions(e.target.checked ? ['*'] : [])}/>Allow all regions</label>
        {['westeurope', 'northeurope', 'eastus'].map(r => <label key={r} className="checkbox"><input type="checkbox" name="regions" value={r}
          disabled={allowedRegions.includes('*')} checked={allowedRegions.includes(r)}
          onChange={e => setAllowedRegions(e.target.checked ? [...allowedRegions, r] : allowedRegions.filter(v => v !== r))}/>{label('region', r)}</label>)}
        <p>{allowedRegions.includes('*') ? 'Clear "Allow all regions" to choose individual regions.' : allowedRegions.length
          ? 'Select the permitted destinations for this rule scope.' : 'No regions selected: processing will not be allowed anywhere.'}</p>
        <p className="muted">The all-regions wildcard is restricted to the demo-only synthetic-data rule; the backend validates all edits.</p></>}
    </fieldset>{failure && <p className="error" role="alert">{failure}</p>}
      {disabled && !pending && <p role="status">Actions are unavailable until current data is ready. You can close this dialog without changing the fleet.</p>}
      {submitted && <p>Retry sends the same request with the same request ID. Close this dialog to make a different change.</p>}
      <button className="primary" type="submit" disabled={disabled}>{pending ? 'Saving…' : submitted ? 'Retry same change' : 'Save change'}</button></form>
  </dialog>;
}
