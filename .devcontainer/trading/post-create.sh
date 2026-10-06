#!/bin/bash
# Post-create script for Drasi Server Trading Demo
#
# Responsibilities:
#   1. Install OS-level build and runtime dependencies.
#   2. Build this checkout and UI with its locked released dependencies.
#   3. Make demo scripts executable.
#   4. Pre-install the preserved compatible plugins, including SSE.
#   5. Delegate remaining startup to start-demo.sh (services).

set -e
set -o pipefail

# Anchor to the repository root regardless of where this script is invoked.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "🔧 Initializing Drasi Server Trading Demo environment..."
echo "   Repo root: $REPO_ROOT"

# Ensure the shared Docker network exists (also handled by initializeCommand,
# but kept here for safety on rebuilds).
echo "🌐 Ensuring shared Docker network exists..."
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# Build the checked-out server rather than downloading an arbitrary binary.
echo "📦 Installing build and runtime dependencies..."
sudo apt-get update && sudo apt-get install -y \
    build-essential \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    cmake \
    postgresql-client \
    curl \
    jq \
    ca-certificates \
    libssl3 \
    libssl-dev \
    libjq-dev \
    libonig-dev

export JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"

# Shared with start-demo.sh: no latest release, empty UI placeholder, or
# unversioned plugin install can stand in for the reviewed checkout and pins.
bash scripts/prepare-trading.sh

# Make demo scripts executable.
chmod +x examples/trading/start-demo.sh examples/trading/stop-demo.sh

# The shared helper prepares the package/app with their existing locked scripts.
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
