#!/bin/bash
# Post-create script for Drasi Server Trading Demo
#
# Responsibilities:
#   1. Install OS-level runtime dependencies the demo assumes are present.
#   2. Download a prebuilt drasi-server binary from the latest GitHub release
#      (instead of compiling from source, which is slow).
#   3. Make demo scripts executable.
#   4. Pre-install plugins required by the trading config.
#   5. Delegate remaining startup to start-demo.sh (npm/pip deps, services).

set -e
set -o pipefail

# Anchor to the repository root regardless of where this script is invoked
# from. The devcontainer normally runs postCreateCommand with cwd set to the
# workspace folder, but manual invocations (e.g. `bash .devcontainer/trading/
# post-create.sh` from another directory, or running it as `/post-create.sh`)
# would otherwise break the relative paths used below.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "🔧 Initializing Drasi Server Trading Demo environment..."
echo "   Repo root: $REPO_ROOT"

# Ensure the shared Docker network exists (also handled by initializeCommand,
# but kept here for safety on rebuilds).
echo "🌐 Ensuring shared Docker network exists..."
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# Install runtime dependencies only. We no longer compile drasi-server in
# this container (we download a prebuilt binary), so build-only tooling like
# clang / protobuf-compiler / pkg-config / *-dev headers is omitted.
echo "📦 Installing runtime dependencies (PostgreSQL client, curl, jq, OpenSSL, oniguruma)..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    curl \
    jq \
    ca-certificates \
    libssl3 \
    libonig5

# ---------------------------------------------------------------------------
# Download a prebuilt drasi-server binary from the latest GitHub release.
# ---------------------------------------------------------------------------
DRASI_RELEASE_REPO="drasi-project/drasi-server"
DRASI_BIN="target/release/drasi-server"
PLUGINS_DIR="target/release/plugins"

# Detect platform: linux glibc vs musl, x86_64 vs aarch64.
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)  ARCH_TAG="x86_64" ;;
    aarch64|arm64) ARCH_TAG="aarch64" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Use musl variant if running on a musl-based distro (e.g. Alpine); otherwise
# default to glibc (Debian/Ubuntu — the standard devcontainer base).
if ldd --version 2>&1 | grep -qi musl || [ -f /etc/alpine-release ]; then
    LIBC_TAG="linux-musl"
else
    LIBC_TAG="linux-gnu"
fi

ASSET_NAME="drasi-server-${ARCH_TAG}-${LIBC_TAG}"
mkdir -p "$(dirname "$DRASI_BIN")"

if [ -x "$DRASI_BIN" ]; then
    echo "✅ drasi-server binary already present at $DRASI_BIN — skipping download."
else
    echo "⬇️  Resolving latest drasi-server release for asset: $ASSET_NAME"
    # Honor GITHUB_TOKEN if set (avoids API rate limits in Codespaces/CI).
    AUTH_HEADER=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        AUTH_HEADER=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
    fi

    RELEASE_JSON=$(curl -fsSL "${AUTH_HEADER[@]}" \
        -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${DRASI_RELEASE_REPO}/releases/latest")

    DOWNLOAD_URL=$(echo "$RELEASE_JSON" | jq -r --arg name "$ASSET_NAME" \
        '.assets[] | select(.name == $name) | .browser_download_url')

    if [ -z "$DOWNLOAD_URL" ] || [ "$DOWNLOAD_URL" = "null" ]; then
        echo "❌ Could not find asset '$ASSET_NAME' in the latest release."
        echo "   Available assets:"
        echo "$RELEASE_JSON" | jq -r '.assets[].name' | sed 's/^/     - /'
        exit 1
    fi

    RELEASE_TAG=$(echo "$RELEASE_JSON" | jq -r '.tag_name')
    echo "   Release: $RELEASE_TAG"
    echo "   URL:     $DOWNLOAD_URL"

    curl -fSL "${AUTH_HEADER[@]}" -o "$DRASI_BIN" "$DOWNLOAD_URL"
    chmod +x "$DRASI_BIN"
    echo "✅ Downloaded drasi-server to $DRASI_BIN"
fi

# Sanity-check the binary runs.
"$DRASI_BIN" --version || {
    echo "❌ Downloaded drasi-server binary failed to execute."
    exit 1
}

# The prebuilt binary has the Web UI assets embedded (see src/ui_assets.rs),
# so we don't need a real ui/dist on disk. Create the directory so
# start-demo.sh's "ui/dist missing → rebuild UI" check is satisfied and it
# doesn't invoke `make build-ui` (which would require Node tooling).
mkdir -p ui/dist

# Make demo scripts executable.
chmod +x examples/trading/start-demo.sh examples/trading/stop-demo.sh

# ---------------------------------------------------------------------------
# Pre-install plugins required by the trading config from the OCI registry.
#
# The trading config (examples/trading/server/trading-sources-only.yaml) sets
# `autoInstallPlugins: true` but has no explicit `plugins:` section, so the
# server's auto-install path is a no-op for sources/reactions referenced by
# `kind:`. Install them up front so start-demo.sh can launch drasi-server.
# ---------------------------------------------------------------------------
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

# Pre-install npm and pip dependencies so the first `start-demo.sh` run is
# fast and doesn't need network access. start-demo.sh will skip these steps
# when the directories already exist.
if [ -d examples/trading/app ] && [ ! -d examples/trading/app/node_modules ]; then
    echo "📦 Installing React app npm dependencies..."
    (cd examples/trading/app && npm install)
fi

if [ -f examples/trading/mock-generator/requirements.txt ]; then
    if ! python3 -c "import requests, flask, psycopg2" 2>/dev/null; then
        echo "📦 Installing Python dependencies for mock generator..."
        pip3 install -r examples/trading/mock-generator/requirements.txt
    fi
fi

echo ""
echo "✅ Drasi Server Trading Demo environment is ready!"
echo ""
echo "To start the demo, open a terminal and run:"
echo "    bash examples/trading/start-demo.sh"
echo ""
echo "Then open http://localhost:5273 in your browser."
echo "Press Ctrl+C in the start-demo.sh terminal to stop everything."
