# Drasi Server Examples

This directory contains practical examples demonstrating different features and use cases of Drasi Server.

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

## Quick Start

Each example includes:
- `server-config.yaml` - Drasi Server configuration
- `scripts/` - Helper scripts for setup and testing
- `README.md` - Detailed documentation and instructions

To run an example:

```bash
# Navigate to the example directory
cd examples/getting-started

# Follow the instructions in the example's README.md
cat README.md
```

## Example Progression

1. **Start with:** `getting-started/` - Learn the basics with PostgreSQL CDC
2. **Experiment:** `playground/` - Interactive exploration via web UI
3. **Master:** `trading/` - Study production patterns

## Plugin Signature Verification

You can enable cosign signature verification for downloaded plugins by adding `--verify-plugins`:

```bash
cargo run -- --verify-plugins --config examples/getting-started/server-config.yaml
```

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
