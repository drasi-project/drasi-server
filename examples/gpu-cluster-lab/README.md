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
