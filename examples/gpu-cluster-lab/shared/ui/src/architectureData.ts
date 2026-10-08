// Explanatory copies are checked against shared/queries by architectureData.test.ts.
const simulationQuery = `MATCH (f:FleetConfiguration)
OPTIONAL MATCH (f)-[:POLICY_CONFIG]->(p:PolicyAssessment)
RETURN {epoch:f.epoch, configuration:f.configuration, settings:f.settings, plan:f.plan} AS snapshot,
       p.assessment AS policy, p.epoch AS policy_epoch
`;
const schedulingQuery = `MATCH (f:FleetConfiguration)-[:POLICY_CONFIG]->(p:PolicyAssessment)
OPTIONAL MATCH (g:gpu_inventory)
OPTIONAL MATCH (g)-[:GPU_SAMPLE]->(s:GpuSample)
WITH f, p, g, s,
     CASE WHEN s IS NULL THEN false
          WHEN s.observation_epoch <> f.epoch THEN false
          ELSE NOT drasi.trueNowOrLater(
              datetime.realtime().epochMillis >= s.report_time_ms + 5000,
              datetime({epochMillis: s.report_time_ms + 5000})) END AS fresh
RETURN f.epoch AS epoch, f.configuration AS configuration, f.plan AS plan,
       p.assessment AS policy, p.epoch AS policy_epoch,
       collect(CASE WHEN g IS NULL THEN null ELSE {
           gpu_id:g.gpu_id, fresh:fresh,
           background_memory_mib:s.background_memory_requested_mib,
           background_compute_units:s.background_compute_units
       } END) AS capacities
`;

export type ArchitectureNode = {
  id: string;
  kind: 'client' | 'service' | 'source' | 'query' | 'transformer';
  label: string;
  title: string;
  summary: string;
  x: number;
  y: number;
  implementation: string;
  detail: string;
  input: string;
  output: string;
  example: string;
  note: string;
  queryIds?: readonly string[];
  schema?: string;
};

export type ArchitectureEdge = {
  id: string;
  kind: 'graph' | 'rows' | 'transport' | 'lifecycle';
  sources: string[];
  targets: string[];
  path: string;
  branches?: string[];
  title: string;
  implementation: string;
  detail: string;
  schema: string;
  changes: string;
  note: string;
};

export const databaseInputs = [
  ['input-clusters', 'regional_clusters'],
  ['input-policies', 'placement_policies'],
  ['input-data', 'data_profiles'],
  ['input-gpus', 'gpu_inventory'],
  ['input-settings', 'gpu_telemetry'],
  ['input-workloads', 'workload_requirements'],
  ['input-plan', 'gpu_placements'],
] as const;

const candidate = `Candidate {
  decision_id: UUID
  expected_plan_version: decimal string
  config_fingerprint: string
  policy_signature: string
  policy_bundle_hash: string
  scheduling_signature: string
  assignments: Assignment[] // complete fleet
  decision_details: JSON
}
Assignment (selected fields) {
  workload_id: UUID; replica_index: integer
  gpu_id: UUID; profile_id: string
  data_profile_id: string; purpose: string
  memory_mib: integer; compute_units: integer
}`;

const samples = `GpuSample (selected properties) {
  gpu_id: UUID; observation_epoch: UUID
  report_sequence: decimal string
  report_time_ms: integer
  applied_plan_version: decimal string | null
  background_memory_requested_mib: integer
  background_compute_units: integer
  managed_memory_mib: integer
  managed_compute_units: integer
  busy_percent: integer
}`;

const graphNote = 'Native drasi.graph-change v1 envelopes carry inserts, updates and identity-only deletes. These are selected decoded properties, not a JSON wire envelope.';
const rowNote = 'Native drasi.query-row v1 envelopes carry query identity, row signature, sequence and snapshot/progress information. The schema shows row values, not the envelope. Independent feeds are not an atomic multi-query snapshot.';

export const architectureNodes: ArchitectureNode[] = [
  {
    id: 'browser', kind: 'client', label: 'BROWSER', title: 'React dashboard',
    summary: 'Workloads, GPUs and allocations.', x: 135, y: 95,
    implementation: 'LiveApp · @drasi/react · http://localhost:5400',
    detail: 'The dashboard subscribes once to each of nine UI queries. It displays reported state and sends user commands; it does not calculate policy, placement or recovery.',
    input: 'Initial query snapshots and subsequent SSE result differences, through control.',
    output: 'Revision-checked HTTP commands for workloads, inventory, report settings and policy.',
    example: 'Pause GPU reports: the last sample remains visible, then the query marks it overdue after five seconds.',
    note: 'This overlay is static explanation, not live topology. Opening it does not add subscriptions or change the scenario.',
  },
  {
    id: 'control', kind: 'service', label: 'HTTP SERVICE', title: 'Control: commands',
    summary: 'Validate and commit inputs.', x: 405, y: 95,
    implementation: 'gpu-control · Axum + SQLx · port 5400',
    detail: 'Validates browser commands, checks origin/CSRF and revisions, and commits authoritative input changes. A separate authenticated internal endpoint accepts complete candidate plans. This same service serves the dashboard and proxies its read APIs.',
    input: 'Browser commands; POST /internal/placement-plans from the plan writer.',
    output: 'SQL transactions and acknowledgements. An acknowledgement is not execution confirmation.',
    example: 'A telemetry PATCH checks expected_revision before changing background_compute_units.',
    note: 'The plan endpoint rechecks plan version, configuration, policy and static assignment constraints. Fresh observed capacity is a solver input, not a live telemetry check inside that SQL transaction.',
  },
  {
    id: 'database', kind: 'service', label: 'AUTHORITATIVE STORE', title: 'PostgreSQL',
    summary: 'Configuration + saved plan.', x: 675, y: 95,
    implementation: 'PostgreSQL 16 · database gpu_demo',
    detail: 'Stores cluster, policy, data-profile, GPU, telemetry-generator and workload settings, plus one complete fleet plan. Durable command receipts and reset state also live here but are not the seven published input tables.',
    input: 'Validated configuration writes and guarded complete-plan commits.',
    output: 'Snapshot/bootstrap data and committed row changes for the replication source.',
    example: 'gpu_placements contains fleet demo, plan_version and the entire assignments array, rather than one independent write per replica.',
    note: 'gpu_telemetry stores generator settings, not measured GPU samples. The simulator produces the samples.',
    schema: databaseInputs.map(([, table]) => `${table} → input-configuration`).join('\n'),
  },
  {
    id: 'postgres', kind: 'source', label: 'SOURCE + BOOTSTRAP', title: 'Database changes',
    summary: 'Snapshot, then PostgreSQL CDC.', x: 945, y: 95,
    implementation: 'postgres · drasi/postgres-transactions + coordinated PostgresSnapshot',
    detail: 'Imports one exported PostgreSQL snapshot, then emits complete committed transactions. Its single immediate query owner persists the input state and committed cursor in RocksDB. JSON middleware keeps configuration and assignments typed.',
    input: 'PostgreSQL snapshot and replication stream.',
    output: 'Complete drasi.source-transaction envelopes to input-configuration only.',
    example: 'A new inventory row becomes a gpu_inventory node keyed by gpu_id.',
    note: 'WAL acknowledgement follows the atomic query commit, not queue acceptance. The slot and RocksDB state survive restart and scenario reset. This is not an atomic snapshot of every downstream UI query.',
  },
  {
    id: 'inputs', kind: 'query', label: '1 ATOMIC QUERY', title: 'Input context',
    summary: 'One committed configuration.', x: 1215, y: 95,
    implementation: 'input-configuration · drasi/continuous-query · RocksDB',
    detail: 'One aggregate covers configuration, generator settings and the saved plan across all seven tables. A source transaction produces only its final query changes; policy never assembles independent table feeds.',
    input: 'Complete PostgreSQL transactions with typed JSON normalization.',
    output: 'One complete configuration row stream into policy.in.',
    example: 'Deleting the last workload removes its tagged record from the complete aggregate; no obsolete workload remains cached.',
    note: 'A keyed bootstrap snapshot seeds volatile policy state after restart. Its watermark fences older replayed query output.',
    queryIds: ['input-configuration'],
    schema: 'Row { records: { table: string, value: table-specific record }[] }\n\nMATCH (n)\nRETURN collect({table:n.`__gpu_table`, value:<selected typed columns>}) AS records',
  },
  {
    id: 'policy', kind: 'transformer', label: 'NATIVE TRANSFORMER', title: 'Policy evaluator',
    summary: 'Regional placement rules.', x: 1215, y: 335,
    implementation: 'policy · gpu.lab/regorus-policy · Regorus 0.12.0',
    detail: 'Uses Regorus to evaluate shared Rego rules against each workload and candidate destination. Data-profile scope identifies the configured conditions; customer match, classification, purpose and region must all pass. Authority is recorded provenance, not an extra Rego permission check. Publishes rule configuration alongside fingerprinted authorization evidence.',
    input: 'The complete input-configuration stream and its keyed bootstrap snapshot.',
    output: 'FleetConfiguration, PolicyAssessment and per-pair PlacementEligibility graph changes.',
    example: 'Healthy eastus capacity cannot host a workload whose customer policy allows only the configured EU regions.',
    note: 'A fictional customer agreement, not a general GDPR rule. Unknown permission is not allow. Policy failures and stale inputs remain explicit.',
  },
  {
    id: 'simulator', kind: 'transformer', label: 'QUERY + NATIVE TRANSFORMER', title: 'Telemetry simulator',
    summary: 'Simulated execution + telemetry.', x: 945, y: 335,
    implementation: 'simulation-inputs → simulator · gpu.lab/telemetry-simulator',
    detail: 'A context query joins configuration to policy. The timed simulator applies the saved plan and emits simulated measurements plus application and per-replica enforcement evidence. Drasi-owned wakeups generate reports even when no input changes.',
    input: 'simulation-inputs rows: snapshot, optional policy and policy_epoch.',
    output: 'GpuSample, AppliedPlan and PolicyEnforcement graph changes.',
    example: 'Turning reporting off preserves execution and the last sample. Power-off stops execution and reports; the scheduler still learns unavailability through report expiry.',
    note: 'Unknown policy suspends processing while retaining memory; denial fences execution and releases memory after acknowledgement. No real model serving, GPU or live memory migration.',
    queryIds: ['simulation-inputs'], schema: simulationQuery,
  },
  {
    id: 'scheduling', kind: 'query', label: 'CONTINUOUS QUERY', title: 'Scheduling context',
    summary: 'Policy + fresh observed capacity.', x: 675, y: 335,
    implementation: 'scheduling-inputs · drasi.trueNowOrLater',
    detail: 'Joins policy/configuration with actual simulated reports. A scheduled time condition marks a report stale after five seconds, even if no later report arrives. Both solver consumers receive this same context.',
    input: 'policy.out and simulator.out graph changes.',
    output: 'Query rows containing epoch, configuration, saved plan, policy and per-GPU observed capacities.',
    example: 'A report expires: fresh becomes false and that GPU is no longer eligible for a new placement.',
    note: 'Planning capacity subtracts reported background memory and compute demand. The scheduler does not read the simulator power switch as a shortcut to detect loss.',
    queryIds: ['scheduling-inputs'], schema: schedulingQuery,
  },
  {
    id: 'placement', kind: 'transformer', label: 'NATIVE TRANSFORMER', title: 'Placement solver',
    summary: 'Allocate replicas across GPUs.', x: 405, y: 335,
    implementation: 'placement · gpu.lab/placement-solver · good_lp 1.15.0 + microlp',
    detail: 'Uses a binary MILP with per-GPU memory/demand, model, policy and VM-spread constraints. Retains an already-valid allocation; otherwise first minimizes existing replica moves, then peak normalized managed demand.',
    input: 'scheduling-inputs query rows with current policy and observed capacity.',
    output: 'CandidatePlan, DecisionExplanation, SchedulingContext and capacity diagnostics.',
    example: 'A 76-GiB assistant cannot use two 56-GiB gaps. Rearranging chat replicas can create one sufficiently large gap without adding a VM.',
    note: 'Bounded workers reject stale completions. Infeasible and unknown outcomes do not become partial successful plans. Capacity-only diagnostics cannot reach the writer.',
  },
  {
    id: 'writer', kind: 'service', label: 'QUERY + NATIVE REACTION', title: 'Plan output + writer',
    summary: 'Save the placement plan.', x: 135, y: 335,
    implementation: 'plan-output → plan-writer · gpu.lab/plan-writer',
    detail: 'The query selects only CandidatePlan payloads. The native sink decodes the singleton row and submits the candidate over internal HTTP, with a stable decision ID for guarded retry/receipt handling.',
    input: 'placement.out → plan-output.in; plan-output.out → plan-writer.in.',
    output: 'POST /internal/placement-plans and write outcomes through the plugin status hub.',
    example: 'A changed expected_plan_version yields a 409 conflict, not an overwrite. The writer waits for a newer candidate instead of retrying that conflict.',
    note: 'A receipt means the database accepted the plan, not that replicas are running. Confirmation requires the CDC round trip, simulator application and fresh matching reports. A removed candidate cancels pending intent; it cannot undo an already committed request.',
    queryIds: ['plan-output'],
    schema: `MATCH (p:CandidatePlan) RETURN p.payload AS payload\n\nRow { payload: JSON-encoded Candidate string }\n\n${candidate}`,
  },
  {
    id: 'resilience', kind: 'transformer', label: 'NATIVE TRANSFORMER', title: 'Resilience assessor',
    summary: 'VM and region failure analysis.', x: 675, y: 535,
    implementation: 'resilience · gpu.lab/resilience-assessor · shared gpu-placement library',
    detail: 'Removes each VM or region hypothetically and checks whether all replicas can still fit using the same model, memory, demand, spread and policy constraints as real placement.',
    input: 'The same scheduling-inputs rows used by placement.',
    output: 'ResilienceAssessment graph changes, correlated by scheduling_signature and policy_signature.',
    example: 'The fleet may be running now but unable to recover from another VM loss; healthy capacity in a denied region does not count as recovery capacity.',
    note: 'Read-only what-if evidence. There is no connection from resilience to plan-writer. Timeout/error is unknown, not proof of infeasibility.',
  },
  {
    id: 'status', kind: 'source', label: 'HOST OBSERVER + NATIVE SOURCE', title: 'Runtime evidence',
    summary: 'Readiness, outcomes and timeline.', x: 405, y: 535,
    implementation: 'runtime-context · gpu-runtime observer · gpu.lab/runtime-status',
    detail: 'The custom host observes query bootstrap, component lifecycle and selected query snapshots. It sends explicit control notifications; the plugin status hub also receives component/write events. The runtime-status source turns these observations into queryable evidence.',
    input: 'runtime-context and UI snapshots, observed graph lifecycle, bootstrap watermarks, and the in-process status hub.',
    output: 'DemoReadiness, ComponentHealth, PlanWriteOutcome and TimelineEvent graph changes.',
    example: 'An HTTP write receipt and a subsequently confirmed plan are different timeline events.',
    note: 'This is custom embedded orchestration, not stock Server configuration. The host sends policy the transactional query snapshot and watermark; a reset additionally waits for its saved-plan identity.',
    queryIds: ['runtime-context'],
    schema: 'runtime-context row {\n  scheduling_signature: string; policy_signature: string\n  required_replicas: integer; current: boolean\n}',
  },
  {
    id: 'views', kind: 'query', label: '9 CONTINUOUS QUERIES', title: 'Dashboard projections',
    summary: 'Join intent with observed results.', x: 945, y: 730,
    implementation: 'ui-* · drasi/continuous-query',
    detail: 'Joins database intent with policy, placement, simulation, resilience and lifecycle evidence. ui-status also projects configured shared rules and data profiles with epoch/evaluation currentness; ui-policy supplies actual destination decisions. Query logic distinguishes saved, applied and confirmed placement and exposes overdue reports, unknown permission and rejected writes.',
    input: 'Committed database projections from policy plus simulation, placement, resilience and runtime-status graph changes.',
    output: 'Nine query result streams, published through the pipeline result outlet/catalog.',
    example: 'ui-placements confirms only matching epoch, configuration, policy, assignment, applied version and a fresh report after execution acknowledgement.',
    note: 'These queries produce the read model. The browser formats it; the overlay does not create another one. Different query snapshots need not represent one instant.',
    queryIds: ['ui-gpus', 'ui-workloads', 'ui-placements', 'ui-resilience', 'ui-decisions', 'ui-status', 'ui-timeline', 'ui-clusters', 'ui-policy'],
    schema: 'ui-placements (selected columns) {\n  fleet_id: string; status: string\n  desired_plan_version: string | null\n  applied_plan_version: string | null\n  confirmed_plan_version: string | null\n  desired: assignment[]; actual: execution[]\n  reason: string\n}',
  },
  {
    id: 'delivery', kind: 'service', label: 'CATALOG + API + SSE', title: 'Query delivery',
    summary: 'Snapshots and live differences.', x: 1215, y: 730,
    implementation: 'Query result outlet · Server API · drasi.network/sse-sink · control proxy',
    detail: 'The host exposes snapshots through Server routes on 8080 and a native SSE sink on internal port 8081. Control proxies the nine selected queries, read-only topology validation and SSE to the dashboard origin.',
    input: 'Query catalog/readers for snapshots; nine direct query-output edges into gpu-demo-ui.in.',
    output: 'GET /api/v1/instances/gpu-demo/queries/:id/results and GET /events/gpu-demo on port 5400.',
    example: 'A ui-gpus result update reaches the existing React subscription; the card refreshes without asking the solver to recalculate.',
    note: 'Grouped delivery infrastructure, not one transformer or a graph output from the catalog. The browser uses the pinned SDK result adapter. The separate admin UI on 8080 is not this dashboard.',
  },
];

export const architectureEdges: ArchitectureEdge[] = [
  {
    id: 'commands', kind: 'transport', sources: ['browser'], targets: ['control'], path: 'M243 95 H297',
    title: 'Dashboard → validated commands',
    implementation: 'HTTP POST / PATCH / DELETE · /api/*',
    detail: 'User input travels as a command, not a query result or a computed placement. The control service replies with success only for the command commit.',
    schema: 'PATCH /api/gpus/:id/telemetry\n{\n  expected_revision: decimal string,\n  changes: { reporting_enabled: false }\n}\n\nOther commands edit workloads, inventory or policy.\nScenario reset is a separate explicit operation.',
    changes: 'Only a deliberate command changes the database. Opening this overlay does not send one. Stale revisions are rejected.',
    note: 'Browser commands use the existing origin/CSRF checks. No UI recalculate endpoint is involved.',
  },
  {
    id: 'commits', kind: 'transport', sources: ['control'], targets: ['database'], path: 'M513 95 H567',
    title: 'Control → PostgreSQL commits',
    implementation: 'SQLx transactions · separate configuration / placement roles',
    detail: 'Validated input changes and complete-plan writes become authoritative database records. Plan commits and receipts are guarded in the same writer transaction.',
    schema: 'Configuration example:\ngpu_telemetry { gpu_id, reporting_enabled, revision }\n\nSaved plan (selected columns):\ngpu_placements {\n  fleet_id: "demo", plan_version,\n  decision_id, config_fingerprint, policy_signature,\n  policy_bundle_hash, assignments, decision_details\n}',
    changes: 'Insert/update/delete of inputs feeds CDC. A successful candidate advances the saved plan version. A rejected candidate does not replace the fleet plan.',
    note: 'SQL transaction safety does not imply atomic observation across independent continuous queries.',
  },
  {
    id: 'replication', kind: 'transport', sources: ['database'], targets: ['postgres'], path: 'M783 95 H837',
    title: 'PostgreSQL → snapshot and CDC source',
    implementation: 'PostgreSQL bootstrap + logical replication',
    detail: 'The source loads existing published rows before following committed row changes, using configured table keys.',
    schema: databaseInputs.map(([, table]) => table).join('\n'),
    changes: 'Committed row inserts, updates and deletes enter the source. Generator settings are input rows; telemetry reports are not written to these tables.',
    note: 'Snapshot/live handover uses a dedicated slot paired with the persistent query owner. Missing retained history fails rather than silently taking a newer snapshot.',
  },
  {
    id: 'input-rows', kind: 'graph', sources: ['postgres'], targets: ['inputs'], path: 'M1053 95 H1107',
    title: 'Database source → atomic configuration query',
    implementation: 'postgres.out → input-configuration.in',
    detail: 'A complete source transaction contains ordered row changes. One atomic query evaluates all operations and publishes its final aggregate.',
    schema: 'gpu_telemetry node (selected properties) {\n  gpu_id: UUID; revision: integer\n  reporting_enabled: boolean; powered_on: boolean\n  interval_ms: integer\n  background_compute_units: integer\n  background_memory_mib: integer\n}',
    changes: 'Multiple changes to one row or several tables are applied in one query transaction. Deleted records disappear from the aggregate.',
    note: 'This edge uses drasi.source-transaction, not ordinary graph-change envelopes. Limits reject oversized or unsupported input without acknowledging it.',
  },
  {
    id: 'context-policy', kind: 'rows', sources: ['inputs'], targets: ['policy'], path: 'M1215 143 V287',
    title: 'Configuration query → policy context',
    implementation: 'input-configuration.out → policy.in',
    detail: 'One query aggregate reconstructs all configuration, generator settings and the saved allocation. The policy component also projects these committed records for dashboard joins.',
    schema: 'input-configuration: { records: { table, value }[] }\nTables:\n' + databaseInputs.map(([, table]) => table).join('\n'),
    changes: 'A complete aggregate replaces the previous context. Empty tables cannot preserve obsolete records. Bootstrap seeds this same query identity and watermark.',
    note: rowNote,
  },
  {
    id: 'policy-simulation', kind: 'graph', sources: ['policy'], targets: ['simulator'], path: 'M1107 335 H1053',
    title: 'Policy → simulation context and simulator',
    implementation: 'policy.out → simulation-inputs.in; query.out → simulator.in',
    detail: 'The graph-change leg feeds the context query. Its query-row output then gives the simulator database intent and optional current authorization. The saved plan returns through this route after CDC.',
    schema: 'simulation-inputs row {\n  snapshot: { epoch, configuration, settings, plan }\n  policy: Assessment | null\n  policy_epoch: UUID | null\n}\n\nJoin: FleetConfiguration.config_fingerprint\n    = PolicyAssessment.config_fingerprint',
    changes: 'Configuration, saved-plan or assessment changes update the context. Missing/unknown policy is not permission to keep processing.',
    note: `${graphNote} This arrow groups two real connections; the second is drasi.query-row v1, not graph changes.`,
  },
  {
    id: 'policy-scheduling', kind: 'graph', sources: ['policy'], targets: ['scheduling'], path: 'M1150 287 V215 H675 V287',
    title: 'Policy → scheduling query',
    implementation: 'policy.out → scheduling-inputs.in',
    detail: 'Configuration and matching policy assessment define the workloads and authorized destination pairs for scheduling.',
    schema: 'FleetConfiguration (selected properties) {\n  epoch, config_fingerprint, configuration, plan\n}\nPolicyAssessment (selected properties) {\n  epoch, config_fingerprint, assessment\n}\nassessment includes policy_signature and pairs.',
    changes: 'Changed inputs invalidate old assessments. The consumer requires matching epochs and current policy before it schedules.',
    note: graphNote,
  },
  {
    id: 'reports', kind: 'graph', sources: ['simulator'], targets: ['scheduling'], path: 'M837 335 H783',
    title: 'Simulator → observed capacity query',
    implementation: 'simulator.out → scheduling-inputs.in',
    detail: 'Actual generated samples, not configured power/report switches, provide background demand and memory. The query supplies time-aware freshness.',
    schema: samples,
    changes: 'Timers insert/update samples. Pausing reports or losing power preserves the last sample; five-second expiry changes freshness without a new event. Removing inventory can delete its sample.',
    note: graphNote,
  },
  {
    id: 'schedule-placement', kind: 'rows', sources: ['scheduling'], targets: ['placement'], path: 'M567 335 H513',
    title: 'Scheduling query → placement solver',
    implementation: 'scheduling-inputs.out → placement.in',
    detail: 'The solver receives one joined scheduling context, then derives eligible per-GPU budgets from the reported background load.',
    schema: 'Row {\n  epoch, configuration, plan, policy, policy_epoch,\n  capacities: [{ gpu_id, fresh,\n    background_memory_mib, background_compute_units } | null]\n}\n\nExample baseline GPU budget:\n81920 MiB - reported background memory\n85 demand units - 10 background = 75 managed',
    changes: 'Intent, policy, background load and report expiry can change the scheduling signature. Old worker completions are rejected when the input generation changes.',
    note: rowNote,
  },
  {
    id: 'schedule-resilience', kind: 'rows', sources: ['scheduling'], targets: ['resilience'], path: 'M675 383 V487',
    title: 'Scheduling query → resilience assessor',
    implementation: 'scheduling-inputs.out → resilience.in',
    detail: 'This is the same context shape as the placement input, not a hypothetical command to the simulator. Each what-if removes a failure domain and asks the shared solver for feasibility.',
    schema: 'Row {\n  epoch, configuration, plan, policy, policy_epoch,\n  capacities: [{ gpu_id, fresh,\n    background_memory_mib, background_compute_units } | null]\n}\n\nOutput evidence is keyed by scheduling_signature\nand policy_signature.',
    changes: 'New scheduling inputs invalidate the old assessment and start a bounded new analysis. Results may be feasible, infeasible or unknown.',
    note: `${rowNote} No hypothetical plan is submitted to the writer.`,
  },
  {
    id: 'candidate', kind: 'graph', sources: ['placement'], targets: ['writer'], path: 'M297 335 H243',
    title: 'Placement → candidate query and writer',
    implementation: 'placement.out → plan-output.in; query.out → plan-writer.in',
    detail: 'Only a complete feasible CandidatePlan is selected by plan-output. Decision explanations and capacity-only diagnostics are separate labels and cannot become writable candidates.',
    schema: `CandidatePlan.payload: JSON-encoded Candidate\nplan-output row { payload: string }\n\n${candidate}`,
    changes: 'A new candidate inserts/updates the singleton. Changed or unavailable inputs can remove it. The writer treats row removal as withdrawal of pending intent, not a delete of the saved database plan.',
    note: `${graphNote} The grouped second connection is a query-row stream. Sink acceptance is not database commit or execution acknowledgement.`,
  },
  {
    id: 'write-plan', kind: 'transport', sources: ['writer'], targets: ['control'], path: 'M135 287 V200 H405 V143',
    title: 'Plan writer → guarded plan commit',
    implementation: 'POST /internal/placement-plans · authenticated internal HTTP',
    detail: 'Sends one complete Candidate to control. The endpoint checks the expected saved version and configuration/policy fingerprints before committing.',
    schema: candidate,
    changes: 'Durable decision receipts support replay of the same request; conflicts are rejected. Transient delivery uncertainty is reported, not presented as successful application.',
    note: 'This closes the loop through PostgreSQL, CDC, policy and the simulator. It is not a direct ComputationGraph cycle or an exactly-once transport guarantee.',
  },
  {
    id: 'runtime-observation', kind: 'lifecycle', sources: ['placement'], targets: ['status'], path: 'M405 383 V487',
    title: 'Placement context → host observation',
    implementation: 'placement.out → runtime-context; host snapshot read → control notification',
    detail: 'The query exposes the active scheduling context. The custom host reads it along with lifecycle and UI observations, then notifies runtime-status. The dotted arrow groups observation steps, not a graph pipe to the status source.',
    schema: 'runtime-context row {\n  scheduling_signature: string\n  policy_signature: string\n  required_replicas: integer; current: boolean\n}\n\nRuntimeObservation (selected fields) {\n  observation_epoch, scenario, components,\n  source_bootstrap_complete, query_bootstrap_complete,\n  query_results_current\n}',
    changes: 'Lifecycle or context changes update readiness observations. Unavailable current context must not be reported as confirmed readiness.',
    note: 'The observer also sends the initial bootstrap boundary to policy through host control. These signals do not carry GPU samples or execute a placement.',
  },
  {
    id: 'write-outcomes', kind: 'lifecycle', sources: ['writer'], targets: ['status'], path: 'M135 383 V535 H297',
    title: 'Plan writer → runtime evidence hub',
    implementation: 'In-process plugin Hub calls · not an out port',
    detail: 'The sink records write receipts, conflicts, supersession and uncertainty in the plugin hub. The runtime-status source publishes queryable outcomes and semantic timeline events.',
    schema: 'PlanWriteOutcome {\n  decision_id: UUID; observation_epoch: UUID\n  outcome: string; detail: string\n}',
    changes: 'A write attempt can produce receipt, rejected, superseded or unknown evidence. This status is distinct from later observation of the committed allocation.',
    note: 'Other native components also report status/events through this hub. Those reporting calls are omitted for clarity; they are not dataflow edges.',
  },
  {
    id: 'evidence', kind: 'graph',
    sources: ['policy', 'simulator', 'placement', 'resilience', 'status'], targets: ['views'],
    path: 'M1215 383 V635 H945 V682',
    branches: ['M945 383 V635', 'M460 383 V435 H810 V635', 'M730 583 V635', 'M460 583 V635 H945'],
    title: 'Five evidence producers → dashboard queries',
    implementation: 'policy / simulator / placement / resilience / runtime-status .out → every ui-* .in',
    detail: 'This bus groups five independent graph streams, each connected to all nine UI queries. Queries filter the labels they need and join matching identities, epochs and signatures.',
    schema: 'policy: PlacementEligibility + configuration/assessment\nsimulator: GpuSample, AppliedPlan, PolicyEnforcement\nplacement: DecisionExplanation, SchedulingContext,\n           CandidatePlan, CapacityDiagnostic\nresilience: ResilienceAssessment\nruntime-status: DemoReadiness, ComponentHealth,\n                PlanWriteOutcome, TimelineEvent',
    changes: 'Evidence inserts/updates change projections. Removed or invalidated evidence must not leave a previous success current. Scheduled query conditions also expire reports without a new sample.',
    note: `${graphNote} This is not a merged atomic event, a new component or a guarantee of ordering across producers.`,
  },
  {
    id: 'database-views', kind: 'graph', sources: ['policy'], targets: ['views'], path: 'M1323 335 H1342 V660 H990 V682',
    title: 'Committed configuration → dashboard joins',
    implementation: 'policy.out → nine ui-* queries',
    detail: 'Policy projects the committed input records into graph changes. UI queries compare that saved intent with execution evidence and join GPU/workload/cluster metadata; they no longer subscribe to PostgreSQL.',
    schema: 'Selected labels:\ngpu_placements · gpu_inventory · gpu_telemetry\nworkload_requirements · regional_clusters\nplacement_policies · data_profiles\n\nExample join: PolicyEnforcement.gpu_id\n            = gpu_inventory.gpu_id',
    changes: 'Input and saved-plan changes arrive through CDC independently of simulator acknowledgement. The UI must not equate a new saved plan with confirmed execution.',
    note: graphNote,
  },
  {
    id: 'results', kind: 'rows', sources: ['views'], targets: ['delivery'], path: 'M1053 730 H1107',
    title: 'UI queries → result catalog and delivery',
    implementation: 'Query outputs → result outlets and gpu-demo-ui.in',
    detail: 'Each query produces keyed row changes. Independent direct edges feed the snapshot catalog and native SSE sink. There is no legacy reaction queue or SSE catalog subscription.',
    schema: 'ui-placements row (selected columns) {\n  fleet_id: "demo"; status: string\n  desired_plan_version: string | null\n  applied_plan_version: string | null\n  confirmed_plan_version: string | null\n  desired: assignment[]; actual: execution[]\n  reason: string\n}\n\nEach of the other eight queries has its own row schema.',
    changes: 'Query additions, updates and removals become result differences. A confirmed_plan_version is present only when the query can prove the matching evidence.',
    note: rowNote,
  },
  {
    id: 'browser-results', kind: 'transport', sources: ['delivery'], targets: ['browser'], path: 'M1215 778 V825 H15 V95 H27',
    title: 'Snapshots and SSE → React dashboard',
    implementation: 'GET query results + GET /events/gpu-demo · control read proxy',
    detail: 'The existing SDK loads query snapshots and applies SSE result differences. The proxy forwards the runtime feed; no new subscriptions, polling or retry policy are introduced by the overlay.',
    schema: 'Snapshot response { success: true, data: Row[], error: null }\n\nSSE result message {\n  queryId: string; results: ResultDiff[]\n  timestamp: integer // milliseconds\n}\nResultDiff (selected fields) =\n  { type: "ADD" | "DELETE", data: Row }\n  | { type: "UPDATE", data: Row, before: Row, after: Row }\n  | { type: "aggregation", before: Row | null, after: Row }\n  | { type: "noop" }\n\nRow shape is selected by queryId, not a graph node.\nHeartbeat { type: "heartbeat", ts: integer }',
    changes: 'Initial/reconnecting reads reconcile the dashboard with query snapshots; live differences update the same existing read model. Deleted result rows must disappear.',
    note: 'Uses the existing sse034ResultAdapter. A transport heartbeat is not a GPU report. Independent query feeds are not one atomic fleet snapshot.',
  },
];
