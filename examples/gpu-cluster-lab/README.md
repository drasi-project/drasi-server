# GPU Cluster Lab

GPU Cluster Lab demonstrates continuous GPU workload placement using Drasi
queries, native computation components, a policy engine, and simulated telemetry.
The application assets are shared; the hosting arrangements are kept separate.

| Directory | Status and purpose |
|---|---|
| [embedded/](embedded/README.md) | The existing working demo. Its custom `gpu-runtime` executable embeds `DrasiLib` and Drasi Server API/UI routes. |
| [server/](server/README.md) | Reserved for the stock `drasi-server`-hosted version. Not implemented or runnable yet. |
| [shared/](shared/README.md) | Domain libraries, native plugin, database helpers and migrations, policies, queries, React UI, fixtures and common documentation. |

## Run the existing demo

From the directory containing the sibling `drasi-core` and `drasi-server`
checkouts:

```sh
cd drasi-server/examples/gpu-cluster-lab/embedded
./demo up
```

The GPU dashboard is at **http://localhost:5400** and the Drasi admin UI is at
**http://localhost:8080/ui/?instance=gpu-demo** by default.
See the [embedded guide](embedded/README.md) for requirements, scenarios,
configuration, diagnostics and shutdown commands.

Click the **i** beside the **GPU Cluster Lab** title for a scenario
overview of AI workload allocation, Drasi's sources/queries/transformers/reactions,
and the role and implementation of the four key transformers, followed by a
high-level diagram of the current embedded implementation.
Click a component or connection (or Tab, then Enter/Space) for its purpose,
contracts, examples and limits. Escape dismisses the selected detail first,
then closes the overview and returns focus to the dashboard.
On small screens, scroll the diagram sideways or use its component/connection
index. The overlay is static explanatory metadata, not live topology or another
read model; it does not change the scenario or add query subscriptions.

The **Data policy** button beside **Global analysis** opens a matching side panel,
closed by default so Workloads and the fleet remain the main view. Only one of
these side panels is open at a time; close it with its X or Escape to return focus
to its header button. On narrow screens, the opened policy panel uses the full
width above Workloads. The panel shows the shared rules: customer
match, permitted classifications, purposes and destination regions. All conditions
must pass. Data profiles identify the applicable rule scope; workloads supply
facts, not independently selected policies. Multiple configured scopes appear
within this shared panel, including scopes with no current workloads. The panel
is simply titled Data policy; internal rule and data-profile names are collapsed
under technical details rather than presented as another policy name.

Workload summaries show classification, purpose and **Policy results**.
The expanded table separates **Data Classification**, **Purpose** and **Policy
results**. Open results for actual evaluator reasons at each destination alongside
the workload's saved, running and confirmed replica counts. **View shared data
policy** opens the rules panel. Permission is separate from capacity:
an allowed destination is a candidate, not an allocation. Missing, mismatched or
outdated decisions remain unknown.

**Edit data policy** edits region permissions for a shared data scope and names
all observed workloads it affects. A scope selector appears only when multiple
rule scopes exist; a single rule needs no name or selection. Revision checks reject concurrent edits.
Changing workload data or purpose changes its facts under the existing rules;
changing shared rules can affect multiple workloads. Both paths display actual
subsequent query results, not browser-computed predictions. The editor leaves
customer, classification and purpose conditions unchanged and does not create
rule or data-profile records. The all-regions wildcard remains restricted to
the demo-only synthetic-data rule.

All scenario presets include both existing data choices: **Synthetic demo data**
(`synthetic`, for `demo`, all regions by default) and **Example customer EU
documents** (`restricted`, for `customer-support`, West/North Europe by default).
The workload editor shows the selected data's classification. Purpose is a
separate fact: selecting **Demo** does not reclassify EU documents, and choosing
synthetic data does not silently change the purpose. Actual permission still
comes from the current shared rules, which may have been edited.

For an existing database, build the updated control image and run
`embedded/demo data-choices` to add only missing catalog records transactionally,
without changing workloads, existing rules/revisions, receipts or scenario
settings. It refuses incompatible existing identities or a pending reset; it
does not migrate/reseed data or start/restart dependencies. Repeating it is a
no-op. The running control service must use the updated image for future scenario
resets to retain both preset choices; an older service still loads its original
single-choice presets. No runtime/solver change or medical-data category is needed.

The existing `ui-status` query projects the policy and data-profile maps already
emitted by the native policy component, guarded by observation epoch and matching
assessment/configuration signatures. No new subscription, source, route or schema
migration is needed; the nine UI subscriptions remain unchanged. An older embedded
runtime without this query projection explicitly shows that full rule criteria
and classification are unavailable rather than inferring them from allowed results.
Updating static assets alone cannot add these fields: an existing installation
needs a separately approved embedded-runtime rollout to load the revised compiled
query. Do not reset/reseed its database to update the UI.

## Layout and existing installations

The former top-level demo now lives in `embedded/`. Its launcher still derives
the Compose project name from this parent directory, so relocation does not
select a new database volume or different image names. Local `.env` credentials
and `.build` artifacts belong in `embedded/`; Rust artifacts belong in
`shared/target/`. None are versioned.

The existing host and application behavior are retained. This layout change
does not implement the Server-hosted version or change the live scenario.
The shared Cargo manifests retain the existing package and binary names;
embedded executable sources are referenced explicitly from those manifests.
The root `rust-toolchain.toml` and Docker build context are common to the layout.
