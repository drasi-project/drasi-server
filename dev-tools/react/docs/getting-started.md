# Installation and live quickstart

[Package overview](../README.md)

## Installation and entrypoints

Use the [portable tarball recipe](../README.md#installation-and-entrypoints) first.

Optional copied-local alternative, **Node 24.19.0 / npm 11.17.0 only**:

```sh
# @drasi-install: copied-local (Node 24.19.0, npm 11.17.0)
DRASI_PACKAGE_DIR="/absolute/path/to/drasi-server/dev-tools/react"
npm install --save-exact --ignore-scripts --install-links "$DRASI_PACKAGE_DIR" react@18.3.1 react-dom@18.3.1
```

The tested Node 22.20.0 / npm 10.9.3 directory-copy path invokes `prepare`
despite `--ignore-scripts`, even with command-scoped ignore propagation.
That mode is unsupported: use the tarball, not a lifecycle bypass. This is an
observation of **npm 10.9.3**, not every npm 10 release.

On the supported copied-local setup, `--install-links` installs distributable
files rather than a development symlink. Retain it for reinstall/CI with
`npm ci --ignore-scripts --install-links`, or set `install-links=true` in the
consumer project's `.npmrc`, never global config. Trading's existing local
dependency/startup and the package's `prepare` behavior are unchanged.

A client-only application can instead install the same tarball with
`npm install --save-exact --ignore-scripts --omit=peer /path/to/drasi-server/drasi-react-0.1.0.tgz`
and use only `/client`. Commit the consumer lockfile. These are **local-file
installs**, not instructions to fetch an available `@drasi/react` npm release.
No source aliases, Tailwind source scanning or install-time rebuilding are
needed by supported recipes. Fresh minimal consumers execute the installed
README/guide commands and render ESM/CommonJS controls with one React 18.3.1 identity.
Unsupported directory modes are executed negative controls, not successful
installs or skipped validation.

Copied installs are snapshots: rebuild, then reinstall. `npm install` may
treat an unchanged-version file dependency as satisfied. Use the copied-install
`npm ci` command above, or give a rebuilt tarball a new local path and update
the lock via its recipe. Restart the bundler after dependency changes; inspect
`npm ls react react-dom @drasi/react` when resolution is unexpected.
Do not patch third-party files, add source aliases, weaken type checks or
clear global caches to hide duplicate React or stale artifacts.

| Import | Contents and dependencies |
| --- | --- |
| `@drasi/react/client` | `DrasiClient`, `DrasiSSEClient`, `DrasiError`, result adapters, `accumulateResult` and connection/transport/read/result types. **No React/React DOM runtime or type imports.** |
| `@drasi/react/react` | Providers, query/status/definition hooks, `useTableSort`, `useRowAnimation`, `useReducedMotion` and named result/options types, including `SortConfig`. React, but no composed components, Radix, React DOM, tutorial code or CSS. |
| `@drasi/react/components` | Provider-free `DataTable` and `Modal`, live `QueryTable`, pure `queryTableState`, optional icons and typed column/action/state/slot/props contracts, including `ModalProps` and `TableHeight`. The client-only modal layer uses pinned Radix Dialog **1.1.15** and `scroll-into-view-if-needed` **3.1.0**, including their portal/geometry dependencies. No tutorial/fullscreen implementation or implicit CSS. `SortConfig` is also re-exported here. |
| `@drasi/react` | Deliberate convenience re-exports of all three groups. This is **not** a React-free import. |
| `@drasi/react/styles.css` | Explicit package-owned stylesheet, never imported by the JS modules. |

All four JS entrypoints ship real ESM/CommonJS exports and conditional `.d.ts`
and `.d.cts` declarations. Import these paths, not `src`, `dist` or hashed
shared chunks. The framework-agnostic client is a module of this same package.

Build output compacts whitespace only: syntax/identifiers are not minified,
debug/component names are preserved, and source maps retain source content.
Declarations, documentation, CSS and license notices remain in the package.

React and React DOM are **optional installation peers** so client-only
consumers need not install them. React is required to execute the
React/component/root entrypoints; browser/SSR applications supply their renderer.
Install the tested React and React DOM **18.3.1** versions for those applications.
The headless `/react` graph does not import React DOM or Radix. The
`/components` and root graphs include the dialog primitive; use the narrower
entrypoints for headless consumers. No React copy is bundled or hidden in
another dependency. The exact peer range
deliberately makes no React 19 or future-major claim.

The exported props/options and hook declarations include JSDoc for editor
hover/completion. Import named types such as `ModalProps`, `TableHeight`,
`DataTableProps` and `UseRowAnimationOptions` from their public entrypoints;
do not depend on a private helper or hashed declaration filename.

Import the component stylesheet once when using presentation:

```ts
import '@drasi/react/styles.css';
```

## Quickstart

Provision resources separately. This example requires instance `analytics`, a
running `readings` query and a running `sse` reaction `events` whose query list
includes `readings`. The SSE endpoint must be browser-reachable, including
proxy routing and CORS. Server bind addresses/ports are not client options.

```tsx
// @drasi-docs: quickstart.tsx
import { DrasiProvider, useDrasiConnectionStatus } from '@drasi/react/react';
import { QueryTable } from '@drasi/react/components';
import type { ReactionReference } from '@drasi/react/client';
import '@drasi/react/styles.css';

const QUERY_IDS = ['readings'];
const REACTION: ReactionReference = {
  id: 'events', endpoint: 'https://events.example/changes',
};
interface Reading { id: string; value: number }

function Readings() {
  return <QueryTable<Reading>
    queryId="readings"
    rowKey={row => row.id}
    queryOptions={{
      getKey: row => {
        if (typeof row.id !== 'string' || !row.id) throw new Error('Expected a stable reading ID');
        return row.id;
      },
      transform: row => {
        if (typeof row.id !== 'string' || typeof row.value !== 'number') {
          throw new Error('Expected a Reading');
        }
        return { id: row.id, value: row.value };
      },
    }}
    columns={[
      { key: 'id', label: 'Sensor' },
      { key: 'value', label: 'Value', align: 'right', format: (_raw, row) => row.value.toFixed(2) },
    ]}
    animateOnChange="value"
  />;
}

function ConnectionBadge() {
  const status = useDrasiConnectionStatus();
  return <span>{status.error?.message ?? (status.connected ? 'Stream open' : 'Connecting')}</span>;
}

export default function App() {
  return <DrasiProvider
    serverUrl="https://drasi.example" instanceId="analytics"
    queryIds={QUERY_IDS} reaction={REACTION}
  >
    <ConnectionBadge />
    <Readings />
  </DrasiProvider>;
}
```

Initialization reads the referenced queries concurrently, waits for the bounded
batch to settle, then checks the reaction and opens **one** SSE connection.
Initialization, subscriptions, reconnect and retry perform **GETs only**.
There is no resource-management mode, implicit instance, query-language choice
or deployment reconciler. Each subscription's `getQueryResults` also validates
its query resource before fetching the snapshot. These required validation GETs
are distinct from optional, app-triggered query-definition inspection.

### Example workspace

The [canonical repository examples](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/dev-tools/react/examples/README.md)
live at `dev-tools/react/examples`, beside this package. Their source and setup
scripts are repository-only, not included in the installed tarball:

- `/`: a live cold-storage table using existing Drasi resources.
- `/query-table.html`: the smaller live `QueryTable` composition, with query
  state/retry slots and a labelled client-only projection-error control.
- `/hooks.html`: a live custom semantic UI using only `@drasi/react/react`
  from this package, without package components or package CSS.
- `/showcase.html`: **explicitly simulated**, provider-free states, sorting,
  themes and Modal composition. It is not evidence of a live Drasi connection.

These entrypoints illustrate separate consumption styles, not new public APIs.
Their README owns startup commands and measured results. Trading remains the
app-owned provisioning, financial, tutorial-inspection and fullscreen example.
