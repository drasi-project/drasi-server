# GPU Cluster Lab: Drasi Server hosting

**Not implemented yet.** This directory is reserved for running the application
as a `gpu-demo` instance owned by the stock `drasi-server` executable, using
configuration and plugins rather than a custom embedded runtime.

There is intentionally no launcher, Compose deployment or placeholder Server
configuration here. Use the working [embedded version](../embedded/README.md).

The Server-hosted version should reuse [shared/](../shared/README.md), including
the native plugin, domain libraries, policies, queries, migrations, fixtures and
React UI, rather than copy those implementations. Deployment configuration and
Server-specific lifecycle integration belong here.

Declarative pipeline reconstruction, bootstrap/readiness coordination and
scenario-reset integration still need to be addressed before this version can
run. The existing embedded host remains the behavior reference.
