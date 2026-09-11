# Drasi Server Examples

This directory contains practical examples demonstrating different features and use cases of Drasi Server.

First complete the [local Server setup](../docs/setup.md), including native
dependencies and matching runtime plugins. These examples exercise the generic
host; they do not install WorkGraph or create its Sandbox repository. For that,
use [WorkGraph setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/README.md)
and [Sandbox setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/sandbox.md).

## Available Examples

### 🚀 [getting-started/](getting-started/)
**Perfect for beginners** - A complete tutorial demonstrating core Drasi concepts with PostgreSQL CDC.

**Features:**
- PostgreSQL source with Change Data Capture (WAL replication)
- Bootstrap provider for initial data loading
- Multiple Cypher queries (filtering, aggregation, time-based)
- Log reaction for console output
- SSE reaction for real-time browser streaming
- Helper scripts for testing

**Start here if you're new to Drasi Server!**

---

### 🎮 [playground/](playground/)
**Interactive Web UI** - A hands-on environment to explore Drasi's continuous query capabilities.

**Features:**
- Dynamic source management via web UI
- Interactive query builder with Monaco Editor
- Real-time data tables with instant updates
- Live results streaming via SSE
- No external dependencies required

**Use this for:** Experimenting with Drasi without writing configuration files

---

### 📊 [trading/](trading/)
Comprehensive example demonstrating advanced features and production patterns.

**Features:**
- PostgreSQL replication source with bootstrap
- HTTP source for live data feeds
- Multi-source queries
- Production-ready configuration

**Use this for:** Understanding complex real-world scenarios and best practices

---

### 🐙 [github-webhooks/](github-webhooks/)
**GitHub Integration** - Receive and process GitHub webhook events in real-time using the HTTP source in webhook mode.

**Features:**
- HTTP source with webhook mode and HMAC-SHA256 signature verification
- Conditional mappings for push, pull request, and issue events
- Continuous queries over GitHub activity
- SSE streaming and console logging
- Simulation scripts for local testing (no GitHub setup required)

**Use this for:** Learning webhook mode, building real-world HTTP integrations

---

## Quick Start

Each example has its own README and layout. The
[configuration collection](configs/README.md) contains standalone YAML examples;
the tutorials also include apps, databases, or scripts as needed.

Use private copies of example configs, not tracked files or live host configs.
Review bind addresses, ports, external services, `autoInstallPlugins`, and
`persistConfig` before running them. Several examples intentionally enable OCI
installation and bind to all interfaces. For the first pipeline, follow the
[local mock/log recipe](../docs/setup.md#6-add-generic-plugins-for-examples).

Helper scripts can start services, download artifacts, mutate sample data, or
remove demo volumes; read the individual tutorial before executing them.

## Example Progression

1. **Start with:** `getting-started/` - Learn the basics with PostgreSQL CDC
2. **Experiment:** `playground/` - Interactive exploration via web UI
3. **Master:** `trading/` - Study production patterns

## Plugin Signature Verification

Startup signature verification is **enabled by default** (`verifyPlugins: true`).
There is no `--verify-plugins` flag. Trusted, self-built unsigned plugins require
the development-only `--skip-verification` exception and an explicit plugin
directory; see [plugin setup](../docs/setup.md#6-add-generic-plugins-for-examples).
Do not disable verification to work around an unavailable or incompatible
registry artifact.

## Common Patterns

All examples demonstrate:
- ✅ YAML-based configuration
- ✅ Auto-start components
- ✅ Source → Query → Reaction data flow
- ✅ REST API usage
- ✅ Helper scripts for testing

## Need Help?

- 📚 See main repository [README.md](../README.md)
- 📖 Read [CLAUDE.md](../CLAUDE.md) for development guidance
- 🐛 Report issues at [GitHub Issues](https://github.com/drasi-project/drasi-server/issues)
