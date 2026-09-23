# @drasi/react

Build a table from rows you already own, connect a table to an existing Drasi
query, or use headless hooks for your own UI. Styles are opt-in; provisioning,
business logic and application dialogs remain yours.

**Private, unpublished package in this repository.** Use a built local
artifact, not `npm install @drasi/react` from a registry. Trading keeps its
existing local dependency. React/React DOM **18.3.1** are the tested peers;
React 19 and untested framework integrations are not claimed.

## Installation and entrypoints

**Use the built tarball:** tested on Node **22.20.0 / npm 10.9.3** and
Node **24.19.0 / npm 11.17.0**. From the repository root:

```sh
npm --prefix dev-tools/react ci --ignore-scripts
npm --prefix dev-tools/react run build
(cd dev-tools/react && npm pack --ignore-scripts --pack-destination ../..)
```

Then, from your React application's directory:

```sh
# @drasi-install: tarball
DRASI_PACKAGE_TARBALL="/absolute/path/to/drasi-server/drasi-react-0.1.0.tgz"
npm install --save-exact --ignore-scripts "$DRASI_PACKAGE_TARBALL" react@18.3.1 react-dom@18.3.1
```

Keep one React identity. A default `file:` symlink can resolve a second
development React even at the same version. The optional
[copied-local recipe](docs/getting-started.md#installation-and-entrypoints)
is tested only on Node 24.19.0/npm 11.17.0 with `--install-links`.
The measured npm **10.9.3** directory path runs `prepare` despite
`--ignore-scripts`; use the tarball instead. The guide covers reinstall/cache
behavior, client-only installation and the complete entrypoint map.

## Your first table

Drop this component into your existing React app. It needs no server, Drasi
provider, query or network request. The stable `rowKey` preserves row identity
while the headers sort supplied rows.

```tsx
// @drasi-docs: data-table.tsx
import { DataTable, type ColumnDef } from '@drasi/react/components';
import '@drasi/react/styles.css';

interface Delivery { id: string; destination: string; parcels: number }
const rows: readonly Delivery[] = [
  { id: 'north', destination: 'Warehouse A', parcels: 12 },
  { id: 'south', destination: 'Warehouse B', parcels: 3 },
];
const columns: readonly ColumnDef<Delivery>[] = [
  { key: 'destination', label: 'Destination', align: 'left' },
  { key: 'parcels', label: 'Parcels', align: 'right' },
  {
    key: 'summary', label: 'Summary', sortable: false,
    format: (_value, row) => `${row.parcels} parcels to ${row.destination}`,
  },
];

export default function Deliveries() {
  return <DataTable<Delivery>
    title="Deliveries" rows={rows} columns={columns} rowKey={row => row.id}
  />;
}
```

## Choose the next step

| You want to... | Use |
| --- | --- |
| Show one existing live query | [The `DrasiProvider` + `QueryTable` quickstart](docs/getting-started.md#quickstart), with explicit instance/query/reaction references and validated raw identity. |
| Build cards or custom markup | [Hooks-only UI](docs/reference.md#hooks-only-ui-and-scoped-retry) from `@drasi/react/react`; no presentation controls or CSS. |
| Share one query across several presentations | [Hoist one hook](docs/reference.md#app-owned-composition), then pass its rows to tables/cards. A provider shares a connection, **not a query cache**. |
| Add actions, controlled sorting or a dialog | [Typed actions and slots](docs/reference.md#columns-and-actions), [sorting](docs/reference.md#sorting), and the provider-free [Modal](docs/reference.md#modal). |
| Connect securely and handle failures | [Authentication/lifecycle/hosting](docs/connection.md), [errors and scoped retry](docs/reference.md#errors-and-recovery). |
| Check a prop, callback or default | [Complete API and recipe reference](docs/reference.md). |

## Run the examples beside the components

The single canonical workspace is **`dev-tools/react/examples`**.
Its [repository README](https://github.com/drasi-project/drasi-server/blob/main/dev-tools/react/examples/README.md)
has the exact commands for four small cold-storage entries:
`QueryTable`, a hook-plus-`DataTable` composition, hooks-only cards, and a
clearly labelled [server-free simulated showcase](https://github.com/drasi-project/drasi-server/blob/main/dev-tools/react/examples/README.md#run-only-the-simulated-showcase).

**Repository-only link:** runnable source, setup scripts and tests are not
shipped in the tarball. Use the source checkout to run them; the documented
clean consumer copies that workspace and installs the actual tarball without
package source aliases or install-time rebuilding. The real pages use actual
pre-created resources; the simulated page is not protocol/recovery proof.

## Data flow and ownership

Operator-owned resources feed one read-only client/SSE connection, then each
subscriber's raw rows, validated projection and UI. `QueryTable` composes the
hook and table; `DataTable` and `Modal` need no provider. The application owns
resource creation, actions, layout and additional announcements.

Use raw `getKey` for accumulation and sparse deletes; rendered `rowKey` is
separate. Snapshot/SSE handoff is best effort with bounded visible recovery,
not atomic or exactly once. Read the [identity/consistency contract](docs/reference.md#hooks-raw-identity-and-derived-views)
and [subscriber cost guidance](docs/reference.md#subscriber-state-and-computation-cost)
before scaling a live view.

## Verify, adapt and contribute

[Testing and compatibility](docs/testing.md) maps public surfaces to unit,
integration, installed-package, browser and actual-backend checks and commands.
[Migration](docs/migration.md) covers the former bootstrap API and Trading's
app-owned wrappers. Both guides distinguish historical evidence from current
guarantees. Human screen-reader acceptance remains **open**; automated checks
are not a WCAG claim or a waiver of Trading's existing contrast findings.

## License

Apache License 2.0. Retain the shipped [LICENSE](LICENSE) and [NOTICE](NOTICE)
and applicable dependency notices. Private staging grants no publication or
trademark permission.

<details>
<summary>Links from earlier versions of this README</summary>

<a id="choose-your-starting-point"></a><a id="quickstart"></a><a id="example-workspace"></a>
Installation and live examples moved to [Getting started](docs/getting-started.md).

<a id="api-reference"></a><a id="public-symbol-map"></a><a id="connection-options"></a><a id="normalized-results-and-explicit-wire-adapters"></a><a id="hooks-raw-identity-and-derived-views"></a><a id="subscriber-state-and-computation-cost"></a><a id="hooks-only-ui-and-scoped-retry"></a><a id="snapshotstream-consistency-and-limits"></a><a id="components"></a><a id="provider-free-datatable"></a><a id="columns-and-actions"></a><a id="sorting"></a><a id="live-querytable-and-scoped-state-slots"></a><a id="app-owned-composition"></a><a id="table-sizing"></a><a id="modal"></a><a id="scoped-themes-and-portals"></a><a id="reduced-motion"></a><a id="low-level-clients"></a><a id="drasiclient"></a><a id="drasisseclient"></a><a id="errors-and-recovery"></a><a id="troubleshooting"></a>
Prop, option, callback, recipe and error sections moved to the [API reference](docs/reference.md).

<a id="server-and-read-only-dto-contract"></a><a id="authentication-and-injected-transports"></a><a id="ownership-and-reconfiguration"></a><a id="consumer-and-hosting-responsibilities"></a><a id="ssr-and-import-safety"></a>
Connection, hosting and SSR sections moved to [Connections](docs/connection.md).

<a id="119-bootstrap-to-connect-only-migration"></a><a id="p6--164-part-b-migration"></a><a id="accessibility-evidence-and-remaining-acceptance"></a><a id="p5--164-part-a-migration"></a><a id="p4--163-part-b-migration"></a>
Migration and acceptance history moved to [Migration](docs/migration.md).

<a id="verified-compatibility"></a><a id="development-and-trading-verification"></a>
Compatibility and contributor checks moved to [Testing](docs/testing.md).
</details>
