# GPU Cluster Lab

A local simulation of a GPU fleet: PostgreSQL commands, Regorus processing policy,
minimum-movement placement with `good_lp`/`microlp`, and measured feedback through Drasi.
No GPU, model download, Azure subscription, or live model migration is required.

**Operational under PostgreSQL's existing committed row-by-row delivery contract.**
The real command/database/query/policy/solver/plan-writer/simulator/SSE/UI loop
passes the complete ordinary scenario sequence, including regional recovery,
fragmentation, resilience, five-second report expiry, resets, writer failures,
concurrency, source loss and genuine graceful restart. The matching release
runtime and plugin use Core's shared direct-memory default and qualified
functional/temporal repairs, without an example-only provider override.

**The stronger complete-transaction policy-context guarantee is deferred.**
Committed rows may be observed individually; the demo does not guarantee that
every policy decision sees an entire multi-row database transaction together.
`/health/ready` continues to report `transaction_completion: "not-supported"`.
`./demo check --acceptance` still exits nonzero at its final `supported` assertion,
after the ordinary scenarios pass. This is not an all-tests-green or stronger
transaction-consistency claim. That design remains reserved for user review;
no inferred boundaries, connector redesign or relaxed assertions were introduced.

The browser-facing release is running with preserved business configuration,
saved plan and receipts, and fresh volatile query state. Live mode never
substitutes fake data. The development-only mock preview remains separate.

## UI preview with mock data

With the existing UI dependencies installed, run from this example:

```sh
cd ui
npm run dev:mock
```

Open **http://127.0.0.1:5173**. The same React views display the baseline,
memory-fragmentation, and regional-policy scenarios. A purple **Demo preview**
panel above Workloads contains the **MOCK DATA** label, preview warnings,
example-state controls and preview-only notices.
GPU/VM controls and workload/policy forms edit only this tab's in-memory
preview. Edits invalidate displayed measurements, execution counts, placement,
policy and resilience evidence rather than pretending to run the backend.
Use **Example state** in that panel to select a prepared stage, or **Reload**
to discard local edits. The baseline, fragmentation and regional-boundary fixtures
share the same views and projection validators. The snapshots include:

- Reporting paused before/after the five-second deadline, power-off before expiry,
  and complete recovery on surviving VMs.
- A one-move fragmentation candidate, stale-candidate rejection, database commit,
  application acknowledgement, and fresh measurement confirmation.
- Tenant demand that fits now but exhausts one-VM recovery capacity; adding an
  already-ready VM restores the margin.
- Regional recovery into North Europe, policy-constrained infeasibility despite
  healthy US capacity, and restoration by adding ready permitted capacity.
- Unknown policy with suspended execution, requested stops awaiting acknowledgement,
  acknowledged fencing, solver timeout, startup, stale assessments and failed feeds.

These are **hand-authored expected outcomes**, not captured live output or a
JavaScript optimizer/policy implementation. Sample ages are frozen; time does not
advance automatically. Sample demand/memory is explicitly last-reported, so a
new stop acknowledgement does not rewrite the last sample. A suspended replica
retains memory; a fenced replica releases it; a pending stop is not an
acknowledged fence. Click a workload to inspect its destinations independently
of GPU report status. Policy forms prefill the current parameters, including `*`
(shown as **Allow all regions**).

### UI terminology

Use **VM** for the modeled Azure infrastructure unit and **GPU** for a device
within it. A VM starts with two GPUs; separate VMs do not imply separate physical
hosts or availability zones. Do not use "worker" as a second name for a VM in
presenter-facing text. A **workload** is an AI service; a **replica** is one copy,
which must fit on one GPU. **Help** in the compact header opens **How to read this
demo**, which explains
these terms, memory units, demand units, report freshness and policy actions.
The page header is identical in preview and live mode: **Starting scenario** and
**Reset scenario**, plus **System status**, **Global analysis**, then **Help**.
The starting-scenario group is centered between balanced header columns, with
an intrinsically sized, compact dropdown; narrow screens place it on its own row.
The starting scenario selects Baseline fleet, Memory fragmentation, or Regional
policy. Selecting an option does not change settings; resetting requires
confirmation. In mock preview, reset loads the corresponding local fixture.
In the live demo, it sends `POST /api/demo/presets/{name}` through the control
service. The live endpoint gates and drains writes, awaits graph shutdown,
transactionally seeds the selected fixture, recreates the runtime, and reopens
writes only after observed bootstrap/readiness. Failures leave writes gated.
Reset intent is persisted before stopping the graph and cleared only after observed
readiness, so a control-process crash cannot reopen an incomplete reset. Restarted
control services retain the gate until an explicit reset retry succeeds. This
lifecycle record is not a query input or a PostgreSQL transaction-completion marker.
Reset retains durable command and plan receipts. Retrying an old successful
creation acknowledges its original identity without recreating deleted work in
the new fixture; reusing its key for different content remains a conflict.
Reset closes prior proxy-stream generations and temporarily rejects query/SSE
opens with 503 so an old connection cannot remain falsely live across graph
replacement. The recovery reset remains available when feeds are unavailable;
it still requires confirmation and reinitializes the existing SDK subscriptions
after a successful reset.
Canonical scenario IDs are `baseline`, `fragmentation`, and `regional-boundary`;
the CLI also accepts `regional`. The selector adopts the asynchronously reported
starting scenario without overwriting a pending choice on subsequent status updates.
Mock-only UI is supplied as a single optional
demo panel; live mode omits that panel rather than substituting header controls,
warnings or form button labels. Ignoring the demo panel shows the same shared
interface, including **Save change** in dialogs. In preview, that button still
changes only local state, as the panel explains.
Readiness failures and command errors remain in the shared interface and are
not hidden with the demo panel. The purple **Demo preview** panel is never
shown in the live demo; only its mock snapshot controls and warnings disappear.
The live view observes the SDK's connection-status hook as well as initialization,
so backend errors appear during automatic retries, including before the first
query snapshot. Retained rows cannot enable edits while transport is disconnected.
When the browser reports it is offline, live edits are disabled immediately even
if an existing stream has not yet failed. Returning online reinitializes the
existing SDK connection and query subscriptions before edits can resume; retained
rows alone do not establish freshness.
There is no separate toolbar or workload/policy inspection dropdown:
**Edit policy** sits beside **Add workload** in the Workloads header.
Add workload uses the same neutral styling as other panel actions, and action
buttons do not use leading plus signs.
Each region panel has a small **REGION**
label beside its name instead of a separate fleet section heading.
There is no global totals strip, introductory hero, duplicate scenario picker or
persistent footer. Regional-policy and connection-freshness caveats live in Help.

Shared visual styles in `ui/src/style.css` define panel, group, body, label and
dense-summary typography, consistent panel gutters/corners, and control styling.
Green identifies positive evidence, amber identifies caution or unknown state,
and red identifies explicit errors or policy denial; text and icons retain the
meaning without relying on colour alone. Cyan identifies selection, links and
keyboard focus. Report freshness and policy permission remain separate signals,
so a denied destination can still have a recent-report badge.

There is one **Workloads** panel at the top of the left-hand stack, collapsed by
default. It uses the same native disclosure expander as the region panels.
Its title and action buttons remain in a fixed header; the disclosure,
summary and expanded table sit below it, like the region panels. Its thin summary
strip shows selectable workload cards with
**confirmed / required** replica counts and a small confirmation bar; hover text
also identifies running counts. Unknown or stale counts never produce a green
confirmation bar. The strip scrolls horizontally when necessary rather than
growing into a tall dashboard. Expand Workloads for the full table, including
model requirements, execution/policy counts and edit/delete actions.
**Add workload** and **Edit policy** stay in this panel's header and work without
expanding it.
Selecting a GPU replica highlights its workload in either view without expanding
the panel or changing the layout.

Each region owns an **Add ready VM** button. The form displays and uses that
region; it does not silently fall back to the first region or offer a separate
region picker. Empty regions can receive VMs. If the selected region disappears
while the form is open, saving is blocked with an explicit message.
All panel disclosures start collapsed on initial load, including regions and
GPU details; Global analysis also starts closed. Expanding panels is a local
view preference that is retained across feed updates, not a new computation.
Collapsed regions retain their replica counts and show a compact strip of VM
cards with a badge for each registered GPU. Badge colours describe **report
freshness**, not execution or permission to run workloads. Power-off, paused
reporting and exclusion settings are identified separately; stale feeds show
unknown report status. Expanding the region replaces this strip with its full
VM/GPU cards without changing the region header.

**Global analysis**, in the page header, opens a right-side drawer
containing **Placement plan** and **Recovery after failures**. It starts closed,
leaving the fleet full-width. The drawer opens in its own right-hand column,
aligned with the first region. Regions, workloads and the
other left-side panels narrow to make room; nothing is overlaid. The analysis
column grows to fit its content, including expanded assessments and technical
details, and scrolls with the page rather than inside a height-capped panel. Closing it
returns that space to the left column. Close it with its close button,
Escape, or the header toggle. Escape dismisses an open command/policy dialog
before the drawer. Reduced-motion preferences disable the slide animation.
Opening the drawer only reveals already-subscribed results: it does not invoke
the optimizer, start a new assessment, or create additional query subscriptions.

Supporting evidence is contextual rather than three permanent bottom panels:
**Decision details** is a closed disclosure inside **Placement plan**, separating
the saved plan's evidence from unsaved attempts and diagnostic assessments.
**Activity**, beneath the regions, starts collapsed with the latest event and
expands to show history, newest first. Events are ordered by sequence within a
run, with the current run ahead of older runs. Decision links open Global analysis,
expand Decision details and focus the matching record; missing history is explicit.
These reference jumps skip the width animation so the destination can be read
immediately, while the header toggle still opens and closes the column normally.

**System status** in the header opens read-only reported component statuses.
Actual component problems are highlighted; absent, stale or unrecognized status
is unknown, never assumed healthy. Runtime status is separate from connection
status, GPU health and replica confirmation. Activity and System status remain
inspectable when mutation controls are disabled. None of these controls adds
subscriptions or invokes backend processing; the live feed gaps below still apply.

Each regional cluster's collapsible summary shows **planned / running /
confirmed** replica counts alongside its local inventory/report counts.
**Replicas required** remains on each workload: a workload can move between
permitted regions, so it has no fixed per-region requirement. **Planned**
counts assignments in the saved plan, not a proposed plan; its tooltip identifies
the saved version and explains that it can lag configuration changes. Saving a
new destination updates this target without moving running counts. A saved
target can be shown before execution is known; a missing plan or unresolvable
destination is **Unknown**, not zero.

Running counts follow actual execution, and confirmed counts are the subset
with current policy permission and matching fresh GPU reports. All three counts
remain visible when a cluster is collapsed. Confirmation checks each replica
against current requirements, saved/applied plan versions,
policy permission and matching fresh GPU reports after application; these checks do not
require the entire fleet to have converged. Unavailable or mismatched evidence
shows **Unknown**, not zero; an unconfirmed stop makes the affected region's
running count unknown. A fully observed empty region shows zero. Selecting a
workload highlights replicas and permitted destinations without changing these
regional totals.
The display label **Confirmed** replaces replica **Ready**; the underlying
`ready_replicas` projection field is unchanged. **Add ready VM** still means
capacity that is prepared for use, not a replica confirmation.

The **Replicas required** column states how many copies each workload needs.
Select a workload in the summary strip or expanded table to highlight its named replica tiles
on GPUs. Selecting a replica's name highlights its parent workload and its
other replica tiles; select the same name again to clear the selection.
This link also works for planned, paused, stopped and
historical replicas without changing their state or issuing a command.
Policy permission for the selected workload appears in a reserved GPU-header
slot: a green check for allowed, a cross for denied, or a question mark for
unknown. Hover text and accessible labels identify the workload and permission.
The slot stays the same size with no selection, so highlighting never inserts
a policy text row into the GPU card. Click the indicator to open read-only
**Policy details** for that workload and region: reasons, policy settings,
authority and technical evidence. The details follow current query data and
remain inspectable when results are unknown or stale; they never issue a
mutation command. There is no duplicate policy panel in the sidebar.

GPU cards start condensed, showing report freshness, last-reported memory and
compute demand, and the replicas running or planned there. Power-off, paused
reports, exclusion from plans and policy stop/pause states remain visible without
expansion. The chevron expands one GPU to show resource breakdowns, replica
requirements, action confirmations and report/settings details. The action icons
pause/resume reports, fail/restore the GPU, edit background load, exclude/include
it in plans, and remove it. Each has hover text and an accessible action name;
removal still requires confirmation. Stale data and pending commands disable
these actions, but do not prevent expanding a card to inspect its evidence.

The UI uses **proposed, saved, applied, confirmed by GPU reports** for plan stages.
It uses **Paused by policy**, **Stop requested**, and **Stopped by policy** rather
than exposing enforcement codes without explanation. GPU badges say **Recent
report**, **Report overdue**, or **Report status unknown**: they describe
observability, not a claim that the device is still powered on. Demand units are
illustrative, not utilization percentages or measured model performance.

Readable names and code labels are centralized in `ui/src/labels.ts`. Technical
IDs, signatures, raw query rows and exact state codes remain in **Technical
details**. This is a presentation change only: fields such as `host_id`,
`registered_workers`, `workers`, and `fenced`, as well as `/api/hosts` routes,
retain their existing contracts. New or unknown identifiers remain visible
rather than being translated into an invented success state.

### Demo-owned UI projections

`ui/src/rows.ts` validates the after-images consumed by the shared views. These
are CQ **projections for this demo**, not new Drasi Server management API
DTOs. All nine are wired into the live instance graph. Raw-key extraction
still accepts sparse deletes. Revisions and versions are decimal strings; absent
measurements and execution counts are `null`, not zero. Mock identifiers,
fingerprints and signatures are visibly synthetic, not cryptographic evidence.

| Query | Key | Displayed evidence |
|---|---|---|
| `ui-clusters` | `cluster_id` | Region, VMs in inventory, GPUs with recent reports; empty clusters retained |
| `ui-gpus` | `gpu_id` | Settings/revisions, independent heartbeat health, sample age/time/sequence/input revisions/plan version, reported background and managed demand/memory, busy percent |
| `ui-workloads` | `workload_id` | Profile, context, revision, requirements, running/ready/suspended/fenced/stop-pending counts |
| `ui-placements` | `fleet_id=demo` | Complete committed desired assignments, execution/enforcement acknowledgements, desired/applied/confirmed versions, decision and signatures |
| `ui-policy` | `id=<workload_id>/<cluster_id>` | Workload/cluster allow/deny/unknown, reasons/errors, authority, parameters, policy revision and input fingerprint |
| `ui-resilience` | `fleet_id=demo` | Separate per-VM/per-region additional-loss outcomes, analyzed signatures and read-only capacity-only diagnostic |
| `ui-decisions` | `decision_id` | Candidate/committed/rejected/diagnostic evidence, feasible/infeasible/unknown outcome, moves vs new replicas, pre-plan memory gaps |
| `ui-status` | `fleet_id=demo` | Fleet readiness, scenario, current observation epoch/signatures and nested component status |
| `ui-timeline` | `event_id=<epoch>/<sequence>` | Semantic transitions with timestamps and decision/plan links, not every telemetry tick |

Fleet confirmation checks matching versions, epoch, scheduling/policy signatures,
current workload requirements, applied assignments, authorization fingerprints
and fresh per-GPU samples. Query failure/staleness blocks current-success badges
and commands. An assessment with no scenarios is not a pass. Capacity-only
diagnostics never contain a replacement desired plan. Historical complete plans
and decisions remain distinguishable from current evidence after edits/failures.

No API/SSE connection, PostgreSQL, Rust build, solver, or policy evaluator is used
in this mode. Stop it with Ctrl+C in the terminal running Vite. The port is strict
so an existing service is never silently replaced. Mock loading is gated by both
Vite development mode and `--mode mock`; production builds and ordinary live
startup retain the real Drasi path. This preview does not establish integration
readiness.

## Run and stop

From this directory, with Docker and Docker Compose installed:

```sh
./demo up
./demo down
```

`up` generates local credentials, exports the pinned React package source without
switching branches, exports the current sibling Core/Server source, builds the
matching Linux runtime/native plugin and package tarball/UI/control image, runs
migrations, and starts the services. The launcher needs Python 3 for source
export; compilation runs in Docker. The export includes current untracked source
files and named license notices such as `LICENSE-MIT`, with content hashes in
the image's source manifest. When using Core's shared quiet SpookyHash module,
both runtime and diagnostic images retain its MIT notice under `/app/licenses/`.
An available host npm cache can supply
integrity-verified pinned archives, and the configured HTTPS npm registry is
forwarded without npm credentials. The UI is at **http://localhost:5400**.
Use that exact hostname: it matches the configured command origin. A
`127.0.0.1` alias can serve read-only views but is not an authorized browser command
origin; navigate existing alias tabs to `localhost` rather than weakening CSRF.
`up` waits for actual query-backed readiness and fails with the observed error
if that is not reached, then prints the published UI URL and owned Compose
project name. Operational readiness does not assert the deferred complete-
transaction policy-context guarantee described above.

`down` targets only this checkout's Compose project, works repeatedly, and
preserves its database. It does not require a responsive application API. It does
not stop other Drasi environments or delete their containers. Docker itself must
be available to stop containers.
Both binaries handle Docker's SIGTERM. The control service closes active SSE
responses before draining HTTP requests; the runtime awaits graph cleanup.
Independent control restart preserves the simulator epoch, while runtime restart
bootstraps a new epoch without replacing the saved plan or fixture.

Manual configuration, build, wait, or smoke commands are not prerequisites for
normal startup.

## What exists

| Component | Implementation |
|---|---|
| Shared contracts and fixtures | `crates/contracts`: hardware/serving catalogs, baseline, fragmentation, regional-boundary, bounded fleet, canonical fingerprints and complete-plan validation |
| Policy | `crates/policy`, `policies/placement.rego`: real Regorus, complete pair assessments, explicit unknown/deny, shared write guard |
| Placement and resilience | `crates/placement`: real MILP, minimum existing moves then peak demand, VM/region loss, read-only capacity-only evidence |
| Simulation | `crates/simulator`: reporting versus power semantics, monotonic deadlines, execution gates, policy suspension/fencing, atomic application |
| Native transformers | `crates/native`: reconstructible SDK factories, query-row/graph codecs, managed wakeups, bounded owned policy/solver jobs, placement priority between resilience scenarios, stale-result rejection |
| Persistence | `migrations`: seven published tables, unpublished durable receipts/reset recovery, revisions, fleet locking, database constraints, separate roles |
| Commands and plan commits | `crates/control`: revision-checked edits, grouped device commands, durable create/plan idempotency, complete plan validation |
| Plan-write transport | `gpu_control::writer` plus the native `gpu.lab/plan-writer` reaction: response validation, terminal 409, bounded same-decision retries, supersession, owned/awaited worker |
| Status and timeline | `gpu.lab/runtime-status`: lifecycle-owned producer, latest component status, typed plan receipts, bounded 128-event timeline |
| Browser | `ui`: React 18.3.1 and the pinned `@drasi/react` tarball, nine hoisted query hooks, raw identity validation, GPU load/reporting/power/scheduling controls, VM/region power commands, workload editing/scaling, explicit stale/errors |
| Runtime | `crates/native/src/bin/gpu-runtime.rs`: actual Server v1 router, PostgreSQL source/bootstrap, six loaded native factories, one instance graph, SSE reaction and lifecycle observer |
| Packaging | `demo`, `compose.yaml`, `ops`: matching working-tree source export, architecture-specific Linux build caches, scoped cleanup and persistent database |

Placement uses equivalent GiB-scaled memory constraints to avoid numerical
conditioning failures in the second MILP pass; final plan validation remains in
exact MiB. A second-pass solver failure is unknown, not proof of infeasibility.
Current explicit policy permission can resume a still-matching existing desired
assignment after checking target availability, current requirements, actual
resident memory (including suspended peers), running demand and VM separation.
This execution-only recovery never writes a partial plan or changes its applied
version. A changed data profile/purpose still requires a new committed assignment.

The domain libraries are wrapped by actual native Drasi `Transformer`
implementations in the separate `crates/native` workspace. Its public input
contract requires explicit coherent bootstrap/configuration messages; it does
not manufacture transaction boundaries from row offsets.
Open command dialogs also disable submission if query-backed readiness is lost;
closing a dialog remains possible without changing the fleet.
The evidence views now render allocation tiles, policy pairs, per-loss resilience,
decision moves and semantic timelines, with optional raw-row inspectors. Their
mock snapshots are exercised independently of Drasi. Seven initial processing
queries and all nine UI query definitions are checked in. All nine construct
with their declared join settings. Seven database CQs assemble authoritative
configuration/settings/plan inputs; simulation and scheduling CQs join those
inputs to native policy and actual timed measurements.

PostgreSQL currently represents JSON/JSONB columns as strings. A table-scoped
adapter reuses the existing strict `parse_json` middleware for policy sets,
workload GPU-model sets, and saved-plan assignments/decision details before query evaluation. Other properties,
including full-width integer versions, and all record identities are preserved.
No source transaction-completion behavior is changed.

### Native evidence projections

Native graph properties now preserve nested lists/objects as well as the lossless
JSON payload. Large counters remain decimal strings; an out-of-range graph
integer is rejected rather than converted to an imprecise floating-point value.

The simulator uses its explicit input observation epoch for reports and execution.
Execution records include flattened assignment fields and `acknowledged_at_ms`,
converted from monotonic time using the component's conservatively ordered UTC
clock anchor. Reports use actual UTC at generation rather than extrapolating the
anchor, avoiding future-dated samples when startup is descheduled between clock reads.
Re-observing an unchanged applied plan does not manufacture a new
acknowledgement, and configuration/application acknowledgements never refresh a
GPU report. Paused/stopped actions are real synchronous simulator acknowledgements;
this implementation does not insert an artificial pending-stop delay.

`PlacementEligibility` includes the authoritative policy context, metadata and
assessment signature needed by `ui-policy`. Pending or invalid contexts remain
explicit unknown rows; an absent policy revision is null, not an invented version.
The UI checks policy epochs and uses the same replica-confirmation predicate for
prepared workload counts and regional/whole-plan evidence, including exclusion
from scheduling.

`ui-gpus` joins the database inventory/settings/region records to actual
`GpuSample` events. Changing reporting/power settings never refreshes the sample
or directly declares its heartbeat expired. `ui-clusters` counts distinct VM
identities and healthy reports, including empty regions; missing initial reports
are unknown rather than a measured zero. Both queries schedule the actual
five-second deadline. A native timed regression proves reporting/power changes
preserve measurements until expiry and that the regional count then reaches zero.

`ui-workloads` and `ui-placements` combine saved database intent, actual simulator
execution, current policy, placement input context and fresh reports. Their
confirmation conditions check epoch/signature, exact assignments/resources,
acknowledgement ordering and current scheduling permission. `AppliedPlan` records
source availability and observed configuration; `PolicyEnforcement` records the
actually applied version; the placement transformer emits `SchedulingContext`
on accepted input and invalidation. Unknown execution is not zero. The synchronous
simulator has no pending-stop interval, so its observed pending-stop count is zero.
The pre-bootstrap workload and assignment-list lifecycle cases pass with the
current upstream repairs. All nine live read models now pass the complete
ordinary scenario sequence, including simultaneous expiry and regional recovery.

`DecisionExplanation` contains deterministic movement counts, the actual
source/destination move list, pre-plan memory facts and input correlation.
Complete committed allocation inputs promote the matching decision to committed;
an HTTP receipt alone does not. Infeasible/unknown solves publish diagnostic
decisions with null movement counts and no executable candidate. Original accepted
evidence survives in `gpu_placements.decision_details`; older plans without detailed
evidence are explicitly identified as such.

The lifecycle-owned producer emits bounded, typed semantic events with explicit
observation epochs, decimal sequence IDs, UTC times and decision/plan links.
Before an input epoch is known, component status is observable without inventing
a scenario epoch for its timeline. `PlanWriteOutcome` distinguishes a terminal
409 rejection from an unknown transport outcome or a superseded request.

`ui-status` reads the lifecycle producer's `DemoReadiness`. The in-process
`Workers::observe_runtime(RuntimeObservation)` hook accepts explicitly observed
source/query bootstrap completion, scenario/epoch/signatures and the topology's
required component statuses. `inputs_ready` preserves that input/lifecycle gate.
Final `scenario_ready` additionally requires correlated current policy and
resilience, settled enforcement, and either freshly confirmed placement or an
explicit current infeasible result. Confirmation queries use `inputs_ready`,
not final readiness, avoiding a circular readiness dependency. Ordinary edits
also use the input gate so pending or failed analysis does not prevent corrective
commands. This correlation does not establish PostgreSQL transaction completion.
Increasing observation sequences reject late updates;
reset, missing required components and current stopped/unavailable components
cannot report ready. Old-epoch component state cannot override current host
evidence. Without a lifecycle observation, the producer explicitly reports
**not ready**, unknown scenario/epoch and the statuses it actually knows.
The runtime now delivers observations through generation-bound native control
notifications to the loaded plugin's actual `Workers` instance. It reads real
query snapshots, including explicit empty results. Nonempty sequence-zero
snapshots must actually reach the input assembler before bootstrap can complete.
Runtime observations allow up to 128 components, including per-query source,
middleware and scheduled-work adapters. No browser action, HTTP receipt, quiet
interval or synthetic timer can promote this gate.

`crates/native/src/projections.rs` supplies the checked-in Cypher and its required
synthetic-join settings. `ui-decisions` joins writer outcomes by decision ID and
checks the epoch; `ui-resilience` joins read-only diagnostics by scheduling
signature and checks policy/epoch. These are real CQ projections, not a browser
optimizer or a competing HTTP read model.

**Native projection gate passed:** connected/disconnected matching, nullable
lists, exact maps and mixed numeric equality now pass with the owner's
uncommitted Core fixes. The native scenario produces freshly confirmed replicas
and fleet plans using the original query conditions. All 31 native tests pass;
the chained TypeScript validator accepts 58 actual source/native/CQ rows across
22 snapshots and all nine UI contracts. There are no substituted rows, casts
to force confirmation, browser deduplication, or weakened count assertions.
While awaiting measurements, the placement query's reason distinguishes a stale
simulator configuration from mismatched confirmed/active/saved/required counts.
These are the same predicates and aggregates used by confirmation, not browser
estimates or another query engine.
The current live qualification below separately exercises PostgreSQL, SSE and
the presenter runbook.

## Integration qualification

The qualified combined-source run on 2026-09-28 passes all ordinary live gates.
Its full acceptance command remains nonzero **only** at the unchanged final
PostgreSQL complete-transaction assertion. Evidence is retained under
`.build/gpu-cluster-lab-539223269-acceptance-7f82c9819513/` and
`target/combined-live-acceptance.log`. Control, Drasi and PostgreSQL each exited
zero before the persistence restart. The separate writer-fault project also
passed lost acknowledgement, retry, exhaustion and new-input recovery.

Current-source lifecycle probes additionally pass source loss, failed reset,
persisted reset-gate recovery after an intentional control crash, and restart
with an open SSE client. Control exited zero in 222 ms and Drasi in 328 ms;
all three services also passed the final clean-stop check. Evidence is retained
under `.build/gpu-cluster-lab-539223269-acceptance-c49edac12e01/`.
The presenter deployment preserved exact database configuration, its complete
saved plan and all receipts. Its real browser, nine query feeds, actual SSE,
command controls, CLI status/wait and smoke check pass with eight freshly
confirmed replicas. No fixture reset was used to make deployment ready.

All 20 exact queries pass separately and combined in the 42-case current-source
isolation matrix, including full due-timer draining and production UI validators.
For the actual native memory default, combined replay fell from 21.64 to 2.39
seconds wall time; paced maximum input lateness fell from 1.92 seconds to 48 ms.
These are recorded-corpus measurements, not universal performance guarantees.
The current logs contain no hot-path `m_data` diagnostic output.

The sole instance graph, actual PostgreSQL bootstrap (including empty workloads
and inventory), all nine query feeds, normal SSE, and all three fixture startups
pass live checks. API and direct-SQL background-load changes produce a measured,
saved/applied/freshly confirmed one-move plan. Workload CRUD, current denial with
authorized survivors, empty-fleet recovery, fragmentation, single-GPU report
expiry and added-VM resilience are also exercised through the actual graph.

Browser scenario resets and offline/reconnect with a missed workload rename and
measured-load change pass through the existing SDK. Failed resets, SIGKILL during
pending shutdown, control restart and concurrent resets preserve the durable
gate and generation/plan-version distinctions. A graph-shutdown timeout observed
during a native build correctly returned 503 and remained gated; it is not
counted as a successful reset. Explicit recovery after runtime restart passed.
Twelve subsequent complete resets across all three fixtures preserve fresh
epochs, monotonic versions and nine valid query feeds.

A real plan writer blocked on the fleet advisory lock rejects its candidate
after a concurrent SQL configuration change; a different current complete plan
then confirms. Actual PostgreSQL loss invalidates runtime readiness and puts
dependent queries into Error with explicit not-running responses, rather than
returning retained results as current. After the database restarts, explicit
reset rebuilds bootstrap/authorization and fresh confirmation while preserving
the prior saved plan until that reset. Query unavailability is not evidence of
a delivered per-replica stop acknowledgement.
Removing a generation-settings row through SQL is separately qualified: the
invalid input is rejected, all eight executions report suspension, and the saved
plan remains unchanged. Restoring the valid row recovers authorization and fresh
confirmation through normal query feedback without a reset.

Actual SSE backpressure qualification also passes: one paused browser stream fell
behind while a healthy subscriber received over 24 MiB of real query events.
The native reaction logged and closed the lagged subscription; the existing SDK
reconnected, fetched all nine snapshots and recovered a missed workload rename
without interrupting the healthy subscriber.

### Earlier failures and retained evidence

The earlier qualified temporal repair passed the example's 31 native tests and 58-row
projection validator. Matching-image isolated acceptance also passes real writer
faults, complete-stack restart, CRUD/empty states, policy reauthorization and
fragmentation. A later capacity-addition run stalled whole-plan confirmation;
further isolated runs reproduce stale plan/readiness/deletion projections after
creating then deleting one workload. Diagnostic predicates retain saved v2 with
application/execution/sample v3 and stale false readiness, while a sibling query
has v3 with all predicates true. Recorded parent/output pairs and bounded drain diagnostics identify substantial
processing backlog rather than proven permanent lost updates. Raw envelope
evidence remains preserved for the upstream evaluator trace; neither slower
reports nor a longer confirmation deadline is a fix.
In a bounded isolated diagnostic, the ordinary 30-second delete deadline expired
with v2 and the deleted workload still visible. Ten seconds after pausing report
generation, the original queries caught up to v3 and removed the workload.
Public native metrics recorded maximum query transactions of 2.43 seconds for
workloads and 1.92 seconds for placements. This is diagnostic evidence for the
upstream optimization, not a passing runbook or a change to reporting cadence.
Two recorder regressions and strict runtime/all-target lint pass.
The first isolated-context optimization was then rebuilt into matching runtime
and plugin binaries. All 31 native tests, both recorder tests, the 58 real rows
across 22 snapshots/all nine UI validators, and strict runtime lint pass.
The unchanged diagnostics-off full run passes writer faults, full-stack restart,
commands and empty-state recovery, then times out after revoking all regional
permission and allowing North Europe. The current policy allows North Europe
and `ui-gpus` shows all 14 fresh reports, but the current scheduling aggregate
marks every capacity stale and placement remains infeasible.
The equivalent six-GPU pause diagnostic also misses the ordinary deadline and
does not catch up within its bounded 20-second pause. Exact stream/sequence
matches show workload parent/output differences up to 41.93 seconds and
placement differences up to 40.63 seconds, while status remains within 188 ms.
Maximum outer transactions are 3.58 seconds for workloads and 2.88 seconds for
placements. These are observations under their respective host loads, not a
controlled claim that the optimization regressed performance.
Immutable image IDs, source manifests, query snapshots and raw diagnostic
envelopes are captured; the recorder is a sibling observer, not a record of each
consumer's exact cross-stream processing order.
Those processing-lag, simultaneous-expiry, regional-recovery, runbook and
matching-image deployment gates now pass on the combined qualified source above.
Keep exact query assertions and strict duplicate/count rejection; do not dedup,
clamp or substitute timer-driven snapshots.

PostgreSQL committed policy-context completion is a separate final gate. Existing
COMMIT delivery dispatches individual rows; explicit bootstrap completion does
not establish transaction completion. All other integration gates above have
passed; its stronger design/implementation remains deferred for user review. Do not
infer boundaries from LSN groups, quiet periods or a debounce.

The retained failures were investigated rather than assumed to be upstream bugs.
The core owner reports passing targeted coverage for native root queries
through ordinary listing/configuration/info/status/results APIs, QueryManager
snapshots/outbox, metrics/logs/events and stop/start. Server default and
instance-scoped GET/results and `/attach` SSE routes also pass with a native batch
query, and an ordinary ApplicationReaction consumes its real output.
The supported tranche is now published in core `61d89d95` and Server `e53a14a`.
The core owner also reports ordinary update/remove support for pipeline-created
queries, native-query rebinding after source updates, and native Source/Reaction
inventory, diagnostics, lifecycle and removal. Opaque implementations and custom
input layouts still require explicit computation operations; native roles do not
fabricate legacy plugin interfaces, and strict consumer recovery policies remain
enforced. The additive permanent-removal `deprovision` hook is not a requirement
to implement a fake-success cleanup method. These capabilities do **not** establish
full managed REST/creation-path parity or the demo's end-to-end acceptance: H03
remains partial. PostgreSQL still has no public complete-transaction event usable
by the policy context adapter, and its boundary design has not been changed.

The native probe `native_root_query_is_exposed_through_ordinary_read_apis`
uses the current singleton APIs:
`computation_pipeline()` builds a `ComponentBatch`, `batch.auto_start(false)`
disables activation, and `add_components(batch).await?` returns a
`ReconciliationReport`. Inspection/control use synchronous
`inspect_computation_graph()?` / `computation_control()?`; query construction
is awaited through `computation_component("ui-gpus")?.wait_created().await?`.
The probe verifies unchanged root identity, exactly one root scope, and that
`ui-gpus` itself is a declared `Query` node before checking positive ordinary
listing, exact configuration, info and status. A nonempty root alone is
insufficient. With activation disabled, the query has status `Added`, not
`Running`: ordinary results must return `DrasiError::InvalidState` with the
not-running message, and the QueryManager snapshot must return typed
`FetchError::NotRunning { status: Added }`, not an unknown-query error.

The positive probe was compiled and executed after the core owner released the
wider recovery/lint gate. The upstream results above remain owner-reported;
the historical table below records the separate example rerun on 2026-09-27, against core
HEAD `5b2fc6f4` plus its local read-access changes and Server HEAD `02761e6`.

The authorized native validation used only
`/Users/alljones/dev/drasi-computation-graph/drasi-server/examples/gpu-cluster-lab/target/native`,
with `CARGO_BUILD_JOBS=1` and locked dependencies:

| Check | Observed outcome |
|---|---|
| Positive native root query read-access probe | 1 passed, 0 failed, 0 ignored, 5 filtered out in 0.02 seconds; exact listing/configuration/info/status and precise not-running errors |
| `cargo build --locked --features dynamic-plugin` | Succeeded |
| `check-plugin` with `dynamic-plugin` enabled | Public host ABI loaded the library and verified all six expected factory names |
| Complete native suite: `cargo test --locked` | 6 passed, 0 failed, 0 ignored, 0 filtered out in 5.05 seconds; no doc tests |
| Strict all-target Clippy: `cargo clippy --locked --all-targets -- -D warnings` | Passed with no warnings |

The previous negative H03 probe is superseded by this positive result.
The probe deliberately leaves the query unstarted; it does not verify live
GPU rows. The ABI check verifies loading/catalog compatibility; these checks
do not establish the full PostgreSQL/query/SSE/UI loop.
That early probe alone did not establish demo readiness. The current full live
results above qualify the ordinary loop, while the stronger PostgreSQL
transaction-completion limitation remains unchanged. No production files or
core target were modified by this example validation.

| Surface | Current evidence | Verification still needed |
|---|---|---|
| PostgreSQL transaction context | `drasi-core/components/sources/postgres/src/stream.rs`, `process_wal_message`: COMMIT dispatches rows individually with LSN/offset | An exposed complete-transaction boundary usable by the policy context adapter |
| Native query read access | Positive listing/config/info/status, exact not-running errors, nine live feeds, full timed/context and runbook qualification | No claim of complete managed creation-path parity |
| Bootstrap | Actual keyed/empty snapshots and explicit completion; current-source interruption and explicit reset recovery pass | Bootstrap completion is not transaction completion |
| HTTP effects | Live exactly-once plan effects, lost acknowledgement, retries/exhaustion, stale-write and reset recovery checks pass | No cross-database distributed-transaction claim |
| SSE lag | Matching runtime logs/closes lagged subscribers; actual paused-browser test resnapshots all nine queries through the SDK while a healthy subscriber continues | No exactly-once replay or atomic snapshot/stream-cut claim |

No production server/core changes are included. If an integration probe confirms
an upstream defect, report its exact revisions, reproduction, expected/actual
behavior, and demo impact before requesting an upstream repair.

## Development and validation

### Query isolation

`ops/query-isolation.Dockerfile` builds a bounded, release-profile diagnostic
against the existing `.build/runtime-src` export. It does not refresh Core or
register a competing application graph. Its optional Cargo feature uses the
already-locked application source solely to extract the actual pipeline memory
provider. Production registration and the diagnostic share processing-query
definitions and joins.

```sh
docker build -f ops/query-isolation.Dockerfile -t gpu-lab-query-isolation .
docker build --target validate -f ops/query-isolation.Dockerfile .
image=$(docker image inspect gpu-lab-query-isolation --format '{{.Id}}')
node --experimental-strip-types ops/check-query-isolation.mjs "$image" \
  .build/<recording-project>/envelopes/<recording>.ndjson .build/query-isolation
```

Use a complete existing bounded native recording containing workload creation
and deletion, plan changes and report batches. The runner invokes all nine UI
queries, the four downstream processing CQs and seven database-input CQs
separately, then interleaved over the same corpus. It compares bare
`InMemoryComputationProvider` with the actual default pipeline provider,
asserting volatility and using the same non-atomic publication mode for both.
No disk query backend or WAL is added.
The `validate` target additionally checks the current runtime's shared query
registration with strict Clippy and native tests against the same frozen export.

Reports separate construction, native transform calls, due-timer calls, output
counts, process CPU ticks and wall-time percentiles, and record Linux load.
Run only after compilation finishes. CPU counters have the reported OS tick
resolution; wall time is not pure evaluator CPU. Neither mode includes live
graph input queue latency.

The native recorded envelopes are unchanged. Database fixture rows are
materialized from their authoritative complete `FleetConfiguration` snapshots
because a sibling recorder may miss original database bootstrap. Observer order
is not a consumer's exact cross-stream merge order. Historical input clocks
remain intact and real due timers are explicitly drained after inputs: this
unpaced diagnostic is **not** a sustained-rate or end-to-end acceptance result.
The drain sends the public futures-due envelope, acknowledges every returned
batch (including empty batches), and continues while the transformer reports
pending emissions. An obsolete timer can legitimately emit nothing before a
later due timer changes rows; empty output alone does not end the drain.
Semantic identity/count/plan checks, production TypeScript row validators and
isolated/combined/provider snapshot comparisons must pass; timings alone are
not correctness evidence. Image and corpus hashes identify each result, while
the image includes both the frozen source manifest and diagnostic overlay hashes.

Append `--paced` to the runner for a separate combined-query run in both provider
modes. It preserves recorded arrival intervals and reports input lateness instead
of sending the whole corpus as fast as possible. This mode reencodes the graph
changes with one recorded UTC shift, including node effective times, report and
acknowledgement times, timeline times and their payload copies. Identities,
monotonic times, intervals and other values are unchanged. Provider comparisons
undo only that UTC shift. After the input sequence, it waits 5.1 seconds without
reports and drains scheduled work with the same strict expiry assertions.
Timers are not driven concurrently during replay; this verifies paced input
processing and final expiry, not the complete graph scheduler or live feedback.

The Rust workspace is standalone; its target and lockfile do not modify the
parent server or sibling core workspace:

```sh
cargo test --locked --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd ui
npm ci
npm run build
npm test
```

The UI's local `npm ci` requires the ignored `ui/vendor/drasi-react.tgz`, built from
the source exported by `./demo configure`. The Docker image performs that build
and packing automatically. Host development requires Rust 1.95.0 and Node 22/24;
normal Docker startup does not.

The ignored PostgreSQL integration test
`postgres_commands_revisions_roles_and_plan_receipts` requires a **dedicated**
database named `gpu_demo_test`, the four roles from `ops/roles.sql`, and
`DEMO_TEST_OWNER_URL`, `DEMO_TEST_CONFIG_URL`, `DEMO_TEST_PLAN_URL`. It refuses another
database name, applies migrations, and replaces only that test fixture. Run:

```sh
cargo test -p gpu-control -- --ignored
```

It covers real PostgreSQL no-op revisions, immutable hardware, role separation,
concurrent idempotent registration, stale revisions/member sets, complete-plan
rejection, and durable plan receipts across fixture replacement. This is not a
live Drasi integration or a measured heartbeat/fencing latency test.

After that integration test and `cargo build -p gpu-control`, the lightweight
`node ops/check-api.mjs` smoke check uses the same dedicated configuration/plan
URLs. It starts and stops its own localhost control process, checks real HTTP
commands/CSRF/idempotency, and verifies that missing Drasi APIs/SSE fail visibly.
It does not create a substitute graph or feed browser fixture data.

The Docker UI/control image and source-matched Drasi runtime/native
plugin use release builds. The runtime and plugin previously used unoptimized
development builds, so the recorded throughput failures above need requalification
with the release packaging; stripping debug symbols alone does not optimize code.
The images build and start on this arm64 host. The actual PostgreSQL query bootstrap
is being exercised; current failures are surfaced through readiness instead of
being replaced with mock state. This does not establish complete live acceptance
or amd64 support. Builds use one-job, architecture-specific Docker caches;
stopping the demo does not discard those caches or database data.

Native integration checks use the sibling core checkout without changing it:

```sh
cd crates/native
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR="$(cd ../.. && pwd)/target/native"
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
# macOS public host-ABI check:
cargo build --locked --features dynamic-plugin
cargo run --locked --features dynamic-plugin --example check-plugin -- "$CARGO_TARGET_DIR/debug/libgpu_native.dylib"
```

Keep the same `dynamic-plugin` feature enabled for the build and ABI runner.
To rerun only the singleton registration/H03 probe, use
`cargo test --locked --lib tests::native_root_query_is_exposed_through_ordinary_read_apis -- --exact --nocapture`.
The core owner released the source-adaptation build gate before the recorded rerun.

The broader native tests pass real CQ payloads through the Regorus/simulator adapters, expire all six
stopped reporters through the actual five-second time-aware CQ within its
500-ms allowance, and verify a native fragmentation solve moves exactly one
existing replica while resilience produces no executable plan. The executed
read-access probe checks positive query discovery and stopped-query error
semantics. These tests do not replace the
remaining real PostgreSQL/bootstrap/SSE acceptance.

To exercise native evidence through the actual CQs and then the production React
row validators (not mock fixtures), run from the example directory:

```sh
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR="$(pwd)/target/native"
export GPU_LAB_NATIVE_ROWS="$CARGO_TARGET_DIR/ui-projection-rows.json"
cargo test --locked --manifest-path crates/native/Cargo.toml \
  --lib tests::projections::native_evidence_projects_into_real_ui_queries -- --exact &&
node --experimental-strip-types ops/check-native-ui.mjs "$GPU_LAB_NATIVE_ROWS"
```

The input fixtures explicitly supply coherent bootstrap/configuration messages;
this does not claim PostgreSQL transaction integration. The native projection
test now passes the complete nine-query scenario. Run the commands
together so an older output file cannot be mistaken for a successful current run.
The validator requires all nine feeds and unique stable identities in every
snapshot.
Bootstrap regressions additionally cover explicit empty results, undelivered
sequence-zero snapshots, missing plans, duplicate identities, stale sequences,
epoch fencing and actual seven-query fixture reconstruction. The UI build and
284 UI tests, placement permutation regressions, 12 simulator tests, nine-query construction,
explicit readiness/sequence checks, timed inventory projections and bounded
timeline are separate evidence, not substitutes for the failing contract.

The native checks also cover source interruption before first authorization
and repeated interruption during recovery. A delayed policy batch cannot reopen
the execution gate: recovery requires a fresh bootstrap after the latest
invalidation. Observed source failure emits suspension acknowledgements without
refreshing GPU heartbeats; inventory removal deletes the old sample. A stopped
transformer refuses same-object restart and requires fresh factory construction
and bootstrap, rather than reusing an old clock, authorization, or solver state.
An infeasible permitted placement automatically produces separate read-only
capacity evidence, never an executable policy-bypassing candidate.

The native HTTP reaction/status test also verifies that a real HTTP receipt
reaches the lifecycle-owned status producer. `SinkCompletion::Accepted` remains
explicit: accepting the query input is not a database commit. A successful
public-ABI load check finds all six native factories (four transformers,
one reaction, one status source); no production plugin registry was modified.

## Data and operations

`gpu_telemetry` contains generation settings, **not measurements**. Only actual
simulator reports may refresh heartbeat observations. Reporting off retains
execution and the last sample. Power off removes execution but still reaches
the scheduler through report expiry. Unknown authorization suspends; known deny
fences and releases reservations.

All command/configuration/placement transactions take advisory lock `778807123`.
API writers take it before reading state; database statement triggers also
participate. No-op updates preserve revision and `updated_at`. The normal
control process has configuration/placement roles, not the migration owner's
credentials. SQLX migration bookkeeping and command receipts are unpublished.

`/health/ready` now proxies the actual graph/query readiness observation.
No `/api/state` or custom business-event stream is used.
The proxy allows only the named query full views/results and the named SSE
reaction, never arbitrary management writes.

Useful development commands: `./demo status`, `./demo logs`, `./demo sql`,
`./demo build`, `./demo check`, `./demo wait --timeout 120`, and `./demo reset baseline`.
`status` lists the owned containers, including stopped ones, and the control
service's actual query-backed readiness response with the scenario/component
failure details. It returns nonzero when unconfigured, unreachable, gated or not
ready; running containers alone are not a successful status.

All CQ indexes and native computational state are volatile memory. The native
pipeline's default memory provider includes resource-ownership/I/O wrappers;
those wrappers do not imply disk indexes, a query WAL or RocksDB. PostgreSQL
persists the authoritative business configuration, saved plans and receipts.
Restart reconstructs computation state and requires new observed confirmation.
`./demo check --smoke` reads actual query results and SSE. `--errors` also checks
authorization, revisions, input rejection and route isolation without changing
state. `--concurrency` exercises competing revision-checked edits, concurrent
idempotent creation, conflicting retry rejection, complete cleanup and historical
receipt replay across reset without reexecuting the old mutation.
`--commands` exercises workload creation/scaling/profile changes, policy
denial, deletion, empty requirements, empty inventory and recovery, followed by
regional policy revocation/recovery/reauthorization.
`--runbook` exercises
load redistribution, fragmentation, resilience, missing reports, VM failure,
regional recovery and policy fencing with actual commands and query assertions.
It retains every failure/confirmation gate, not a simulated successful result.
`--commands`, `--concurrency` and `--runbook` deliberately reset
and mutate the current demo.

Full `--acceptance` first runs the isolated writer-recovery checks described below,
then creates a fresh uniquely named temporary Compose project
and database using the already-built control/runtime image IDs, without publishing
another UI port. It removes only that test project's services and volume on exit,
including failed checks; the presenter's running project and database are untouched.
Before the scenario sequence, it stops and recreates that entire test stack without
rebuilding images or deleting its database. The restart must preserve non-default
settings, identities, the complete saved plan and every durable receipt, then
freshly confirm execution under a new observation epoch. Retrying a deleted
workload's creation must return its historical receipt without recreating it.
`./demo check --restart` runs this isolated restart regression on its own.
Before removing containers, the restart check stops services using their existing
Compose grace periods and requires control, Drasi and PostgreSQL to have exited
with code zero, without an OOM, runtime error or abnormal state. It preserves each
Docker state under `.build/<acceptance-project>/restart-stop-<service>.json`.
Successful `compose down` alone is not graceful-shutdown evidence: Docker can
return success after a forced kill. Earlier restart passes establish persistence,
not this stronger exit-code assertion. The retained old presenter timed out during
graph shutdown and exited 137; that failure is preserved rather than relabeled a
pass. The updated release passes the strict exit-code checks and separate
SSE-connected service restarts. The validator's regression checks run with
`python3 -m unittest discover -s ops -p 'test_*.py'`.
Failed runs retain immutable image IDs, the runtime's source-file hash manifest,
service logs and actual query snapshots under
`.build/<acceptance-project>/` before removing the temporary resources.
For a reproducible projection failure, `GPU_LAB_DIAGNOSTICS=1 ./demo check --acceptance`
also enables a bounded envelope recorder and three read-only predicate/context
queries in the same instance graph. They are absent from the ordinary demo and
do not drive readiness, writes, or UI feeds. The recorder uses the public envelope
codec for actual native producer and query output, preserving identities,
sequences, and lineage. It fails explicitly at 256 MiB or 32 run files rather than
silently dropping evidence. Failed runs capture the final predicate queries and
stop the recorder before copying its files. These sibling subscriptions preserve
each stream's FIFO, not the precise cross-stream order selected at another query's
merge; they must not be presented as an exact recording of that query's inputs.
An authenticated, diagnostic-only endpoint captures public native query output
metrics (sequence counts and last/maximum transaction duration); outbox occupancy
is not an input queue depth. `./demo check --query-drain` uses a fresh isolated
project to create/delete a workload, retain the unchanged 30-second confirmation
window, then pause real reports for a bounded diagnostic drain and request their
restoration. It always preserves diagnostic snapshots/envelopes before cleanup.
Convergence after a pause is never counted as an acceptance pass.
For offline inspection without replay or another evaluator, run
`cargo run --locked --manifest-path crates/native/Cargo.toml --bin inspect-trace -- <recording.ndjson>`
with the example-owned native target. It decodes the recorded envelopes through
the public codecs and rejects non-advancing per-stream sequences.
Build images with `./demo up` or `./demo build control drasi` first. This uses
Docker Compose 2.24 or newer for the isolated port override.
Acceptance additionally retains the unapproved transaction-consistency gate.
`./demo check --writer-recovery` uses the same isolation with a test-only HTTP
forwarder on its private network. It drops a real successful commit response and
separately returns one transient 503 before forwarding. The actual native writer,
database receipts and query feedback must recover with one saved version and
fresh confirmation; a late replay must acknowledge the original commit without
another write. No query rows or database outcomes are manufactured by the test.
It also rejects all five attempts for one candidate, requires a visible writer
error without a saved mutation or retry loop, and verifies that the input gate
stays open for corrective edits. A genuinely new measured input must then produce a new decision
that commits and freshly confirms.
`./demo destroy --confirm gpu-demo` irreversibly
removes only this checkout's demo data volume.

Credentials live in ignored `.env`. Do not commit it, paste Compose's expanded
configuration into reports, or remove it while preserving a database whose
roles still use those passwords.

## Design and presentation

The complete behavioral specification is in
[docs/gpu-cluster-demo.md](docs/gpu-cluster-demo.md); the ordered scenarios are in
[docs/gpu-cluster-demo-runbook.md](docs/gpu-cluster-demo-runbook.md). These copies
belong to this example; the originals in `drasi-core` are unchanged.

The original eight replicas reserve 216 GiB and 260 synthetic demand units.
Every GPU has an 80-GiB workload budget and 75 managed units with baseline
background load. Profiles are illustrative, not benchmark claims. The regional
fixture models a fictional customer's processing-locality requirement, not a
universal GDPR prohibition or production geographical enforcement.
