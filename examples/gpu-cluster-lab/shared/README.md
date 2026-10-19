# GPU Cluster Lab: shared application

Reusable application assets for the [embedded demo](../embedded/README.md) and
the future [Drasi Server-hosted demo](../server/README.md).

| Path | Contents |
|---|---|
| `crates/contracts/` | Domain types, validation, profiles, fingerprints and starting fixtures. |
| `crates/policy/`, `policies/` | Policy evaluation and the shared Rego bundle. |
| `crates/placement/` | Placement and resilience solver. |
| `crates/simulator/` | Simulated execution and telemetry. |
| `crates/native/` | Native computation plugin, query definitions/projections and diagnostic tools. |
| `crates/control/` | Shared database operations and plan-writing client library. |
| `migrations/` | Database schema, roles, publication and reset-state migrations. |
| `queries/` | Continuous queries and reference queries. |
| `ui/` | React dashboard, row contracts and UI tests. |
| `docs/` | Common design and scenario runbook, with current implementation status linked to the embedded guide. |

`Cargo.toml` and `Cargo.lock` define the domain/control workspace.
`crates/native/` keeps its separate workspace and lockfile because it depends on
the sibling Core checkout and parent Server. No dependencies or package versions
were changed for this layout.

The existing binary names are retained without duplicating library packages:
`crates/control/Cargo.toml` points its `gpu-control` binary at
`../embedded/control/main.rs` (relative to this directory), and
`crates/native/Cargo.toml` points its optional `gpu-runtime` binary at
`../embedded/src/main.rs`. Their source code and deployment scripts belong to the
embedded version; the native plugin remains independently buildable with
`--features dynamic-plugin --lib`.

The embedded PostgreSQL input statically registers Core's
`drasi-computation-source-postgres` and `drasi-computation-bootstrap-postgres`
definitions through `NativePlugin::from_definition`. These use the same native
factories and operation boundaries as separately loaded libraries, inside the
existing DrasiLib instance's single graph. The bootstrap and CDC source bind the
actual database query's progress owner; its committed position carries the
dedicated slot identity, database/schema binding and LSN. There is no separate
PostgreSQL service, graph or private checkpoint store.

This is not a state-transparent rollout onto a pre-migration installation.
Completed PostgreSQL positions without the managed-slot ownership field fail
explicitly rather than guessing a slot or resnapshotting. Preserve the existing
slot, query state and receipts until an explicit transition is selected. Do not
reset/reseed the demo or delete retained state to bypass this failure.

The [embedded development guide](../embedded/README.md#operations-diagnostics-and-development)
documents commands for building and exercising these assets.
