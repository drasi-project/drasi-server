#!/bin/bash
# Post-create script for Drasi Server Trading Demo
#
# Responsibilities:
#   1. Install OS-level build dependencies (apt packages) that start-demo.sh
#      assumes are already present.
#   2. Make demo scripts executable.
#   3. Delegate everything else (Rust build, UI build, plugins, npm/pip deps,
#      service startup) to start-demo.sh — single source of truth.

set -e

echo "🔧 Initializing Drasi Server Trading Demo environment..."

# Ensure the shared Docker network exists (also handled by initializeCommand,
# but kept here for safety on rebuilds).
echo "🌐 Ensuring shared Docker network exists..."
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# Install system build dependencies required to compile drasi-server.
echo "📦 Installing system dependencies (PostgreSQL client, OpenSSL, Protobuf, Clang, jq, oniguruma)..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    libjq-dev \
    libonig-dev

# Set JQ_LIB_DIR for the jq-sys crate (architecture-aware) so cargo build works.
JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
export JQ_LIB_DIR
echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.bashrc
echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.zshrc 2>/dev/null || true

# Make demo scripts executable.
chmod +x examples/trading/start-demo.sh examples/trading/stop-demo.sh

# ---------------------------------------------------------------------------
# Pre-install plugins required by the trading config from the OCI registry.
#
# The trading config (examples/trading/server/trading-sources-only.yaml) sets
# `autoInstallPlugins: true` but has no explicit `plugins:` section, so the
# server's auto-install path is a no-op for sources/reactions referenced by
# `kind:`. We install them up front so start-demo.sh can launch drasi-server
# successfully. Only runs if drasi-server has already been built; otherwise
# start-demo.sh will build it first and the user can install plugins after.
# ---------------------------------------------------------------------------
DRASI_BIN="target/release/drasi-server"
PLUGINS_DIR="target/release/plugins"
if [ -x "$DRASI_BIN" ]; then
    mkdir -p "$PLUGINS_DIR"
    NEEDED_PLUGINS=(
        "source/http"
        "source/postgres"
        "bootstrap/scriptfile"
        "bootstrap/postgres"
    )
    for ref in "${NEEDED_PLUGINS[@]}"; do
        category="${ref%%/*}"
        name="${ref##*/}"
        if ls "$PLUGINS_DIR"/libdrasi_${category}_${name}.so >/dev/null 2>&1; then
            echo "Plugin already installed: $ref"
        else
            echo "⬇️  Installing plugin from registry: $ref"
            "$DRASI_BIN" --skip-verification --plugins-dir "$PLUGINS_DIR" \
                plugin install "$ref"
        fi
    done
fi

# Launch the demo in the background. start-demo.sh handles:
#   - building drasi-server + UI (make build-release) if not already built
#   - npm install in app/
#   - pip install in mock-generator/
#   - starting postgres, drasi-server, React app, trading API, price generator
echo "🚀 Starting Trading Demo (delegating to start-demo.sh)..."
nohup bash examples/trading/start-demo.sh > /tmp/trading-demo-startup.log 2>&1 &

echo ""
echo "✅ Drasi Server Trading Demo is starting in the background!"
echo "   Check startup progress: tail -f /tmp/trading-demo-startup.log"
echo "   Trading App: http://localhost:5273"
echo "   Drasi API:   http://localhost:8280"
