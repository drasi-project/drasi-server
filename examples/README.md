# Drasi Server Examples

This directory contains practical examples demonstrating different features and use cases of Drasi Server.

## Available Examples

### 🚀 [getting-started/](getting-started/)
**Perfect for beginners** - A minimal example demonstrating core Drasi concepts.

**Features:**
- HTTP source for data ingestion
- Script file bootstrap provider (loads initial data from JSONL)
- Simple Cypher query filtering products over $50
- Log reaction for console output
- Helper scripts for testing

**Start here if you're new to Drasi Server!**

---

### 🔄 [drasi-platform/](drasi-platform/)
Platform integration example with Redis Streams and bootstrap support.

**Features:**
- Platform source consuming from Redis Streams
- Platform bootstrap provider for initial data loading
- Dual reactions: log (console) + platform (Redis CloudEvents)
- Consumer group management
- Complete event lifecycle demonstration

**Use this for:** Integrating with Drasi Platform infrastructure

---

### 📖 [drasi-platform-read/](drasi-platform-read/)
Simplified platform integration without bootstrap (read-only mode).

**Features:**
- Platform source without bootstrap provider
- Direct Redis Stream consumption
- Log reaction for output
- Simplified configuration

**Use this for:** Consuming pre-existing Redis Streams without initial data loading

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

1. **Start with:** `getting-started/` - Learn the basics
2. **Then try:** `drasi-platform-read/` - Understand platform integration
3. **Explore:** `drasi-platform/` - See bootstrap and dual reactions
4. **Master:** `trading/` - Study production patterns

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
