# OBS-03: Server telemetry runtime and ownership

Status: approved for implementation planning on 2026-11-12. Implementation
remains gated on review of the implementation plan.

Issue: [#236](https://github.com/drasi-project/drasi-server/issues/236).
Predecessor: [#235](https://github.com/drasi-project/drasi-server/issues/235).

## Intent and baseline

Make Server a reliable host for Core's existing observability: initialize
logging before construction, supply the correct collection handles on every
instance path, and retain ownership throughout rollback and shutdown.
This establishes the ownership foundation for subsequent exporters, not a
second instrumentation or observer system.

Reviewed local baseline:

- Server: `observability-item3`, commit `7233182`, sharing its committed tip
  with `observability-item-o2`. Existing uncommitted example/documentation and
  unrelated changes must remain intact.
- Core: sibling `../drasi-core`, branch `otel-drasi-server-work`, commit
  `5fc20eec`. Reuse its graph instrumentation and native observation bridge.
- OBS-02's implementation and decisions are recorded in
  [telemetry configuration](../../telemetry-configuration.md) and the user's
  external OTel progress document. Previous validation is historical evidence,
  not a new verification run for OBS-03.

## Goals

- Reliable ordinary logging, including an empty Server, early constructors,
  plugin discovery, and plugin CLI commands.
- One immutable Server-wide collection policy and one binding mechanism for
  configured, API-created, managed, native and programmatic instances.
- Correct instance/component identity and isolation, including components
  created after startup.
- Explicit ownership for standalone and embedded use, including two Servers
  sharing caller-owned collection.
- Bounded rollback/shutdown waits without losing unfinished owners or
  reporting incomplete cleanup as success.
- Focused evidence using logs and explicitly injected test collection.

## Non-goals and anti-goals

- Production Prometheus/OTLP delivery, new trace export, and native completed-
  span delivery integration remain OBS-04/OBS-05. Process polling and inspection
  endpoints remain OBS-06. Full backend/runtime parity remains OBS-07.
- No writable telemetry API, per-instance policy override, new authentication
  system, built-in trace UI, external-backend proxy or generic HTTP tracing.
- No changes to business results, UUID/configuration ownership, graph/log/SSE
  API contracts, persisted replay/receipt formats, or frozen plugin ABIs.
- Do not infer ownership from globals, create placeholder exporters, invent
  endpoint URLs for tests, duplicate Core observations, or retrofit cached
  telemetry handles into live/prebuilt components.
- Do not turn a timed-out cleanup into success, detach an untracked cleanup
  worker, or close a provider underneath unfinished producers.

## Approaches

1. **Selected: Server-owned coordinator with explicit borrowed bindings.**
   Standalone Server creates and owns its resources. Embedded callers supply
   known collection handles and retain provider ownership. This adds the least
   public lifecycle surface while supporting multi-Server hosts.
2. **Alternative: public application-owned runtime object.**
   CLI and applications construct a separate runtime owner and attach Servers.
   This centralizes multi-Server composition, but makes another lifecycle object
   mandatory for ordinary callers. Not selected.

The user also selected additive, checked ownership-aware APIs rather than
breaking existing prebuilt-Core constructor signatures.

## Ownership model

Use a Server-owned coordinator with two separate concerns:

- **Bindings:** cloneable, immutable access to the selected policy,
  `ComponentLogRuntime`, and optional shared `MetricRegistrationContext`,
  context adapter and native span collector. These are Core's existing types,
  not new recorder/provider implementations.
- **Owned cleanup state:** resources actually created by Server, admission and
  producer state, accepted cleanup work, joins and terminal outcomes. A borrowed
  binding is never also treated as an owned resource.

One metric registration context is shared per supplied recorder. Instance
identity is supplied when applying bindings to each Core builder; it is not
stored as the identity of process-wide plugin loading. Component identities
continue to come from Core's actual graph.

Standalone logging is owned by the standalone composition. Embedded logging,
recorders, collectors, processors and providers are borrowed. Embedded shutdown
does not call their shutdown methods, close their queues, or flush unrelated
Servers' work. It stops this Server's producers/scopes; the application drains
and closes shared collection after all users stop.

Do not install a global metric recorder, meter provider or tracer provider in
this issue. Explicit injected collection is sufficient for its tests.
Future backend implementations join this coordinator's lifecycle; do not build
a general-purpose plugin system for hypothetical cleanup participants.

## Initialization and logging

1. Load configuration and resolve final CLI settings, environment references
   and the OBS-02 policy. Preserve the existing safe endpoint/credential rules.
2. Establish standalone logging before plugin discovery, factories, stores or
   components can emit diagnostics. Create capture with Core's composable
   logging API and compose capture, console output and the `log` bridge once.
3. Keep logging filters separate from operation-span collection filters.
   `RUST_LOG` takes precedence over `logLevel`, preserving the existing console
   policy and `oci_client=error` fallback. Telemetry Off must not disable logs
   or globally filter away context needed by an injected tracing layer.
4. Validate collection availability and establish explicit bindings before
   any factory/component can cache telemetry handles.
5. Apply bindings before Core construction/restoration, then start instances
   and management listeners.

The CLI must pass the known initialized composition into Server construction,
not initialize once and have `DrasiServer::new` attempt a second installation.
Direct standalone library construction uses the same composition path.
Plugin commands need only an early logging composition, not telemetry export.
Missing-config creation diagnostics also use real initialized logging.

`get_or_init_global_registry()` is only a passive accessor. Replace uses that
assume otherwise. Plugin discovery callbacks must use the selected capture
registry; existing per-instance log APIs must read the capture bound to Core.

Standalone global subscriber/logger conflicts are explicit errors. Failed
installation is never permission to borrow an unknown global. Clean up
partially constructed resources on installation failure. Successfully installed
process globals cannot be uninstalled; do not promise in-process replacement
or restart after standalone logging has been closed.

Embedded callers explicitly promise that they composed the supplied logging
and optional tracing/context layers, and installed their log bridge, before
building components. A bundle of handles is not proof that an arbitrary global
subscriber contains those layers. Document this obligation; do not claim to
inspect or repair an unknown subscriber.

## Instance construction coverage

Use one binding application helper before each `DrasiLibBuilder::build()`:

| Path | Required integration |
|---|---|
| YAML/startup instances | Bind before managed host configuration, restoration and component construction |
| Empty Server, then API creation | Carry immutable bindings with Server's instance registry/API context; apply before building the new Core |
| Programmatic primary/additional builders | Apply the same Server policy to every builder before construction |
| Managed and native definitions | Use their owning Core's already-bound graph; no second policy/observer |
| Clone and solution deployment | Create components through the destination Core; never copy source collection identity |
| Shared plugin discovery | Capture early diagnostics without assigning a particular instance to the shared library |

Server-wide policy is authoritative for Server-built instances. Additional
Core builders do not introduce per-instance telemetry overrides. Existing
opaque prebuilt component methods retain their documented limitations;
deferred factories are the supported way to guarantee construction-time
collection. Do not claim that early constructor diagnostics can be recovered.

## Builders and embedding compatibility

Add a full telemetry configuration method and a separate explicit borrowed
collection handoff. Exact public names are a builder choice, but document these
semantics:

- A full configuration call replaces the previous configuration; conveniences
  change only their fields. Selecting a metrics backend replaces the previous
  selection, not the recorder of an unrelated caller.
- Final validation is shared with YAML startup and uses the final host/port/UI
  values. Preserve root `metrics` versus any `telemetry` rejection.
- For explicit embedded collection, the caller declares the enabled collection
  signals/policy and supplies their required handles. Those handles are the
  destination, not a fictional OTLP endpoint. Reuse Core's policy validation
  and the common Server validation rules rather than duplicating them.
- An injected metric context or collector must not silently satisfy or replace
  a requested production OTLP/Prometheus backend. Configured production
  destinations remain unavailable in OBS-03.
- Reject contradictory configuration and embedded handoffs, missing required
  handles, closed logging, or attempted unknown-global adoption before
  construction. There is one effective policy, not two competing policies.
- `build()` and all handle-building variants use the same validated binding
  and rollback paths. Ownership does not depend on whether the API runs.
- Preserve the public shape of the existing `DrasiServerWithHandles` result,
  including direct struct-literal callers. Add an ownership-bearing result and
  additive handle-building method that retain the cleanup owner and expose
  awaited shutdown. The legacy `build_with_handles()` result remains suitable
  for legacy or explicitly caller-owned collection; reject Server-owned
  collection there with guidance to use the ownership-bearing variant. Never
  return only bare Cores while discarding a new Server-owned runtime.
- Provide an additive construction-operation entry point exposing its lifecycle
  handle before the first asynchronous construction/startup step. Existing
  convenience builds/constructors delegate to this owned operation. Dropping
  their waiting future requests rollback; it does not cancel the accepted
  construction/cleanup driver or discard its resource and join ownership.
  Callers needing to observe cancelled construction retain the handle first.
- `build_core()` must not create Server-owned collection and then return only
  a Core with nobody responsible for its cleanup. Keep legacy behavior for
  callers not selecting new Server ownership; require explicit caller-owned
  bindings for new collection, otherwise return a clear configuration error.
- Preserve `from_core`/`from_cores` signatures as legacy wrappers around already
  configured Cores. They neither inject telemetry retroactively nor acquire
  ownership of existing providers.
- Add checked prebuilt-Core entry points for the explicit handoff. Require
  preconfiguration before construction, validate the policy, actual instance
  scope and exposed required capabilities using Core's public accessors, and
  reject known mismatches. Document subscriber composition and shared-handle
  equivalence as caller obligations where Core exposes no identity comparison;
  do not claim exhaustive verification of opaque providers.

Caller-owned process-wide collection may be shared by multiple Servers. The
explicit handoff itself is what authorizes borrowing, never the outcome of a
global installation attempt.

## Startup rollback and shutdown

Use the same owned cleanup coordinator for partial construction, later startup
failure, signal-driven shutdown and explicit embedded shutdown.

Proposed lifecycle:

```text
prepared/running -> quiescing -> stopping producers -> draining owned collection
                 -> shutting down owned providers -> complete
```

An incomplete phase remains owned and inspectable/retryable; it is not a
transition to `complete`. Concurrent/repeated shutdown joins accepted cleanup
rather than starting another teardown.

Order:

1. Stop management/ingress admission and hot-reload management work.
2. Join admitted API/management work and await instance/plugin producers while
   logging and telemetry remain usable.
3. Only after producer cleanup is confirmed, fence this Server's retired
   callbacks and stop/join any Server-owned collection producers.
4. Drain accepted records and flush/shut down owned providers, using only
   remaining time. Keep operational logging available to report failures.
5. Close owned logging last and publish an aggregate terminal outcome.

OBS-03 has no new process polling or scrape listener; later implementations
must observe these ownership/order boundaries.

### Deadlines and retained cleanup

- Preserve the current standalone overall shutdown wait of **30 seconds**;
  this is not replaced by a Core instance's independent lifecycle timeout.
- Use one absolute overall deadline, not nested fresh timeout allowances.
  Preserve the existing **5-second API drain cap**, capped by remaining time.
- After producer cleanup, collection drain plus provider flush share
  `min(telemetry.shutdownTimeoutSeconds, remaining overall time)`.
  The approved default is **5 seconds**, valid **1..=300 seconds**.
- Startup rollback uses the same proposed 30-second bounded wait from the
  failure, not another full allowance for every instance/resource. This is a
  design proposal, not a previously approved startup-timeout change.
- Deadline expiration bounds the caller's wait; it cannot safely kill arbitrary
  blocking cleanup or permit releasing live producer dependencies. Return an
  explicit incomplete-cleanup error, retaining the actual owner and worker
  joins. A later explicit cleanup wait may use a new caller budget; it must
  not recreate resources or reinterpret the first wait as success.
- Expose a cloneable lifecycle/cleanup handle before entering `run()`, retain
  it before asynchronous construction through the construction-operation API,
  keep it in handle-building results, and retain it in typed startup/shutdown
  failures when cleanup is unfinished. The operation's driver owns resources
  and its worker joins independently of an individual waiting future; public
  handles address that same state. Cancellation must leave a reachable owner,
  not a silently detached task.
- Unconfirmed producer cleanup prevents provider shutdown. Retain the instance,
  plugin libraries and collection dependencies together until safe retirement.
- Preserve primary errors and cleanup failures in a typed aggregate. Report
  timeouts, losses and incomplete phases explicitly. `Drop` is not async
  shutdown and must not report successful cleanup.

The user selected **retained standalone cleanup after a deadline**. If the
30-second wait expires with genuinely unfinished cleanup, CLI reports timeout
and incomplete cleanup, retains its lifecycle owner and runtime, and continues
waiting for the accepted cleanup to finish. It does not shut down the Tokio
runtime underneath pending work, grant each provider a fresh flush budget, or
claim a hard 30-second process-exit guarantee. After eventual cleanup it exits
nonzero because the shutdown deadline was missed. A completed cleanup that
reported failures can also exit nonzero without remaining alive. External
forced termination remains an operator/orchestrator action.
Startup rollback follows the same retention rule: the CLI handles typed
incomplete-startup failures rather than dropping the owner through `?` while
cleanup is still pending.

Continue joining existing work, not an automatic unbounded retry loop around
failed lifecycle operations. If a failure needs an explicit retry, retain and
report that cleanup-required state. Embedded waits instead return the typed
incomplete-cleanup result and retained handle at their deadline.

Reuse Core's ownership-before-spawn and cancellation-safe worker helpers.
Blocking SDK/thread joins run off the Tokio executor; neither runtime flavor
may use `block_in_place` or block the executor needed for progress.

The existing legacy progress exporter participates in the same overall
lifecycle ownership and deadline accounting. Preserve its HTTP/JSON transport,
gauge semantics, reference handling and millisecond configuration; do not add
a second provider. Its existing long worker shutdown/join allowances must not
extend the overall wait or discard the worker after a timeout.

## Capability staging and security

- Ordinary logging works with omitted telemetry and Off, including empty
  Server startup and plugin commands.
- Production exporter/process/inspection requests still fail explicitly until
  their implementing issue lands. Update capability wording accurately; do not
  advertise OBS-03's injected test collectors as production delivery.
- No new listeners or authentication exposure are introduced here.
- Preserve environment-backed credentials and reference-preserving persistence.
  Never log resolved destinations, credentials or opaque binding internals.
- Reuse Core's existing validated queue/record/series/link limits. New defaults
  or performance budgets are not introduced by this issue.

## Verification required

### Approved prerequisite amendment: Core shutdown outcome

During Task 2 review, Core's management driver was found to retain a completed
failure permanently while the public instance shutdown API exposes only
`Result<()>`. Server cannot distinguish this from genuinely unfinished cleanup.
The user approved a narrow additive change in the existing sibling Core branch.

Add an authoritative shutdown outcome carrying `Complete`/`Incomplete`
independently of the original typed result. Preserve the existing `shutdown()`
signature and behavior. Completion must cover graph/resource/adapter cleanup,
worker joins, and management/store closure, not just running status or error
classification. Unknown completion remains incomplete.

Server releases a confirmed-complete Core even when its outcome is an error,
retains that typed error in its terminal history, and retains/retries only
unfinished ownership. Qualify both complete-with-error and incomplete-with-error
cases, including eventual release after public handles are dropped. No telemetry
instrumentation, plugin ABI, or persisted-format changes are authorized by this
amendment.

Add focused tests that fail without the new wiring:

- Subprocess-isolated ordinary logging, invalid directives, global subscriber/
  logger conflicts, missing-config/empty-instance startup, plugin-command early
  diagnostics, and no default exporters/listeners.
- Empty startup followed by API creation; two-instance isolation; managed/native
  construction; clone/solution destination ownership using existing fixtures.
- Builder replacement/convenience precedence, invalid-policy parity with YAML,
  additional builders, handle-building cleanup, and prebuilt-Core contracts.
- Explicit shared subscriber/recorder/context handoff: two embedded Servers,
  actual collection from background managed work, no global reinstallation,
  and one Server stopping without closing the other's caller-owned collection.
- Injected stalled/failed cleanup, late startup failure, cancelled waits,
  repeated shutdown, retained owners and one shared deadline on current-thread
  and multi-thread Tokio. Verify sequencing, elapsed budget and retained joins,
  not just a successful health request.
- Legacy progress export/shutdown tests continue to pass without transport,
  payload, timing-unit or gauge changes.
- Preserve existing graph/log/SSE responses and business outputs in affected
  fixtures. Use existing compatibility proofs where suitable; do not rebuild
  all plugins or claim full qualification without running the relevant gates.

Run the smallest existing test groups covering the changes, then affected-target
formatting/type/lint checks. Distinguish baseline failures already recorded in
the compatibility documentation from failures introduced by OBS-03. No build,
test or plugin-rebuild results are claimed by this design.

## Builder discretion and approval

The implementer may choose exact type/method names, focused module placement,
and test fixture structure. Prefer existing helpers and small, explicit types
over configurable lifecycle frameworks. The approved Core prerequisite is
limited to shutdown outcome/completion; no Core instrumentation or FFI changes
are planned. Any further missing API must be identified rather than worked
around by guessing ownership guarantees.

Review these proposed calls especially:

- Preserve the 30-second overall wait and 5-second API cap; bound startup
  rollback with the same overall wait.
- After timeout with pending cleanup, standalone CLI retains ownership and
  remains alive until accepted cleanup finishes, then exits nonzero; the user
  selected this rather than forced process termination.
- Add explicit lifecycle cleanup access, including handle-building results and
  typed incomplete-cleanup failures, with an operation handle available before
  asynchronous construction starts.
- Keep prebuilt-Core compatibility through documented preconfiguration and
  additive checked handoffs, without claiming opaque provider introspection.

After design approval, write the implementation plan. Do not implement as part
of this design discussion.
