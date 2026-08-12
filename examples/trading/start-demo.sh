#!/bin/bash

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Drasi Trading Demo Startup Script
# This script starts all components of the trading demo in the correct order

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
BASE_CONFIG="$SCRIPT_DIR/server/trading-sources-only.yaml"
PLUGIN_SOURCE="registry"

# Create logs directory if it doesn't exist
mkdir -p "$LOG_DIR"

echo "======================================"
echo "   Drasi Trading Demo Startup"
echo "======================================"
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Usage: ./start-demo.sh [--plugin-source registry|local]

Plugin sources:
  registry  Use compatible signed plugins from GHCR (default).
  local     Use plugins prepared by ./build-local-plugins.sh.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --plugin-source)
            [ $# -ge 2 ] || {
                echo -e "${RED}Error: --plugin-source requires registry or local${NC}" >&2
                exit 1
            }
            PLUGIN_SOURCE="$2"
            shift 2
            ;;
        --plugin-source=*)
            PLUGIN_SOURCE="${1#*=}"
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown argument: $1${NC}" >&2
            usage >&2
            exit 1
            ;;
    esac
done

case "$PLUGIN_SOURCE" in
    registry | local) ;;
    *)
        echo -e "${RED}Error: Invalid plugin source '$PLUGIN_SOURCE' (expected registry or local)${NC}" >&2
        exit 1
        ;;
esac

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to wait for a service to be ready
wait_for_service() {
    local url=$1
    local service_name=$2
    local service_pid=${3:-}
    local log_file=${4:-}
    local max_attempts=30
    local attempt=0
    
    echo -n "Waiting for $service_name to be ready..."
    while [ $attempt -lt $max_attempts ]; do
        if [ -n "$service_pid" ] && ! kill -0 "$service_pid" 2>/dev/null; then
            local exit_code=0
            wait "$service_pid" 2>/dev/null || exit_code=$?
            echo -e " ${RED}✗${NC}"
            echo "$service_name exited before becoming ready (exit code $exit_code)"
            if [ -n "$log_file" ] && [ -f "$log_file" ]; then
                echo
                echo "Last 50 log lines:"
                tail -50 "$log_file"
            fi
            return 1
        fi
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|204"; then
            echo -e " ${GREEN}✓${NC}"
            return 0
        fi
        sleep 2
        attempt=$((attempt + 1))
        echo -n "."
    done
    echo -e " ${RED}✗${NC}"
    echo "Failed to connect to $service_name after $max_attempts attempts"
    if [ -n "$log_file" ] && [ -f "$log_file" ]; then
        echo
        echo "Last 50 log lines:"
        tail -50 "$log_file"
    fi
    return 1
}

# Check prerequisites
echo "Checking prerequisites..."

if ! command_exists docker; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

if ! command_exists npm; then
    echo -e "${RED}Error: Node.js/npm is not installed${NC}"
    exit 1
fi

if ! command_exists python3; then
    echo -e "${RED}Error: Python 3 is not installed${NC}"
    exit 1
fi

if [ ! -f "$DRASI_SERVER_ROOT/target/release/drasi-server" ]; then
    echo -e "${YELLOW}Drasi Server binary not found. Building (server + Web UI)...${NC}"
    cd "$DRASI_SERVER_ROOT"
    # Use the Makefile target so the Web UI (ui/dist) is built alongside the binary.
    # `cargo build --release` alone does NOT build the UI and the /ui route would 404.
    make build-release
elif [ ! -d "$DRASI_SERVER_ROOT/ui/dist" ]; then
    echo -e "${YELLOW}Web UI not built (ui/dist missing). Building UI...${NC}"
    cd "$DRASI_SERVER_ROOT"
    make build-ui
fi

# Keep registry and local artifacts isolated so they can never be mixed.
CONFIG_PATH="$BASE_CONFIG"
SERVER_PLUGIN_ARGS=()
if [ "$PLUGIN_SOURCE" = "registry" ]; then
    PLUGINS_DIR="$SCRIPT_DIR/plugins/registry"
    mkdir -p "$PLUGINS_DIR"
else
    PLUGINS_DIR="$SCRIPT_DIR/plugins/local"
    LOCAL_MANIFEST="$PLUGINS_DIR/local-build.json"
    LOCAL_CONFIG="$LOG_DIR/trading-sources-local.yaml"

    if [ ! -f "$LOCAL_MANIFEST" ]; then
        echo -e "${RED}Local trading plugins have not been prepared.${NC}"
        echo
        echo "Enable the local [patch.crates-io] entries in Cargo.toml, then run:"
        echo "  ./examples/trading/build-local-plugins.sh"
        exit 1
    fi

    if ! LOCAL_PLUGIN_REGISTRY="$(python3 - "$LOCAL_MANIFEST" "$DRASI_SERVER_ROOT/target/release/drasi-server" "$PLUGINS_DIR" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

manifest_path = pathlib.Path(sys.argv[1])
server_binary = pathlib.Path(sys.argv[2]).resolve()
plugins_dir = pathlib.Path(sys.argv[3]).resolve()

def sha256(path):
    digest = hashlib.sha256()
    with pathlib.Path(path).open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def git_bytes(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args],
        stderr=subprocess.DEVNULL,
    )

def git_state(root):
    root = pathlib.Path(root)
    status = git_bytes(
        root, "status", "--porcelain=v1", "-z", "--untracked-files=all"
    )
    fingerprint = hashlib.sha256()

    def add_bytes(value):
        fingerprint.update(len(value).to_bytes(8, "big"))
        fingerprint.update(value)

    add_bytes(status)
    add_bytes(git_bytes(root, "diff", "--binary", "HEAD"))
    untracked = git_bytes(
        root, "ls-files", "--others", "--exclude-standard", "-z"
    ).split(b"\0")
    for raw_path in filter(None, untracked):
        path = root / raw_path.decode(sys.getfilesystemencoding(), "surrogateescape")
        add_bytes(raw_path)
        add_bytes(str(path.lstat().st_mode).encode())
        if path.is_symlink():
            add_bytes(path.readlink().as_posix().encode())
        else:
            add_bytes(path.read_bytes())

    return {
        "commit": git_bytes(root, "rev-parse", "HEAD").decode().strip(),
        "worktree_sha256": fingerprint.hexdigest(),
    }

try:
    manifest = json.loads(manifest_path.read_text())
except (OSError, json.JSONDecodeError) as error:
    print(f"Unable to read local build manifest: {error}", file=sys.stderr)
    sys.exit(1)

errors = []
if manifest.get("format_version") != 1:
    errors.append("unsupported or missing local build manifest version")
if not server_binary.is_file():
    errors.append(f"Drasi Server binary is missing: {server_binary}")
elif sha256(server_binary) != manifest.get("server_sha256"):
    errors.append("Drasi Server was rebuilt after the local plugins were prepared")

for filename, expected_hash in manifest.get("plugins", {}).items():
    plugin_path = plugins_dir / filename
    if not plugin_path.is_file():
        errors.append(f"local plugin is missing: {filename}")
    elif sha256(plugin_path) != expected_hash:
        errors.append(f"local plugin changed after validation: {filename}")

for key in ("drasi_server", "drasi_core"):
    recorded = manifest.get(key, {})
    root = recorded.get("root")
    if not root:
        errors.append(f"manifest is missing {key} source information")
        continue
    try:
        current = git_state(root)
    except (OSError, subprocess.CalledProcessError):
        errors.append(f"unable to inspect recorded {key} checkout: {root}")
        continue
    for field in ("commit", "worktree_sha256"):
        if current[field] != recorded.get(field):
            errors.append(f"{key.replace('_', '-')} source changed after the local build")
            break

if errors:
    print("Local trading runtime is stale or incomplete:\n", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(
        "\nRebuild the matched server and plugin set:\n"
        "  ./examples/trading/build-local-plugins.sh",
        file=sys.stderr,
    )
    sys.exit(1)

print(plugins_dir)
PY
)"; then
        exit 1
    fi

    python3 - "$BASE_CONFIG" "$LOCAL_CONFIG" "$LOCAL_PLUGIN_REGISTRY" <<'PY'
import json
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text()
destination = pathlib.Path(sys.argv[2])
registry = json.dumps(str(pathlib.Path(sys.argv[3]).resolve()))

replacement = f"pluginRegistry: {registry}"
if re.search(r"(?m)^pluginRegistry:", source):
    source = re.sub(r"(?m)^pluginRegistry:.*$", replacement, source, count=1)
else:
    marker = "autoInstallPlugins: true"
    if source.count(marker) != 1:
        raise SystemExit(f"Unable to locate exactly one '{marker}' setting")
    source = source.replace(marker, f"{replacement}\n{marker}", 1)

destination.write_text(source)
PY

    CONFIG_PATH="$LOCAL_CONFIG"
    SERVER_PLUGIN_ARGS=(--skip-verification)

    VALIDATION_LOG="$LOG_DIR/local-plugin-validation.log"
    if ! "$DRASI_SERVER_ROOT/target/release/drasi-server" validate \
        --config "$LOCAL_CONFIG" \
        --plugins-dir "$PLUGINS_DIR" \
        >"$VALIDATION_LOG" 2>&1 ||
        grep -Eq 'plugin ABI mismatch|SDK version mismatch|target( triple)? mismatch|Failed to load plugin|\[WARN\].*not installed' "$VALIDATION_LOG" ||
        ! grep -Fq 'Plugins (5 loaded' "$VALIDATION_LOG"; then
        echo -e "${RED}Local trading plugins are incompatible with this Drasi Server.${NC}"
        echo
        grep -E 'plugin ABI mismatch|SDK version mismatch|target( triple)? mismatch|Failed to load plugin|\[WARN\].*not installed' "$VALIDATION_LOG" || tail -50 "$VALIDATION_LOG"
        echo
        echo "Rebuild the server and plugins from the same patched dependency graph:"
        echo "  ./examples/trading/build-local-plugins.sh"
        exit 1
    fi
fi

echo -e "${GREEN}All prerequisites met!${NC}"
echo "Plugin source: $PLUGIN_SOURCE"
echo ""

# Step 1: Start PostgreSQL
echo "Step 1: Starting PostgreSQL database..."
cd "$SCRIPT_DIR/database"
docker-compose up -d

# Wait for PostgreSQL to be ready
echo -n "Waiting for PostgreSQL to be ready..."
max_attempts=30
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if docker-compose exec -T postgres pg_isready -U drasi_user -d trading_demo >/dev/null 2>&1; then
        echo -e " ${GREEN}✓${NC}"
        break
    fi
    sleep 2
    attempt=$((attempt + 1))
    echo -n "."
done

if [ $attempt -eq $max_attempts ]; then
    echo -e " ${RED}✗${NC}"
    echo "PostgreSQL failed to start. Check logs with: docker-compose logs postgres"
    exit 1
fi

# Verify replication slot and publication
echo "Verifying PostgreSQL replication setup..."
SLOT_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t -c "SELECT slot_name FROM pg_replication_slots WHERE slot_name = 'drasi_trading_slot';" | tr -d ' ')
if [ -n "$SLOT_EXISTS" ]; then
    echo -e "Replication slot: ${GREEN}✓${NC}"
else
    echo -e "Replication slot: ${YELLOW}Will be created by Drasi Server${NC}"
fi

PUB_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t -c "SELECT pubname FROM pg_publication WHERE pubname = 'drasi_trading_pub';" | tr -d ' ')
if [ -n "$PUB_EXISTS" ]; then
    echo -e "Publication: ${GREEN}✓${NC}"
else
    echo -e "Publication: ${RED}Missing - creating...${NC}"
    docker-compose exec -T postgres psql -U postgres -d trading_demo -c "CREATE PUBLICATION drasi_trading_pub FOR TABLE stocks, portfolio, stock_prices;"
fi

sleep 2

# Step 2: Start Drasi Server
echo ""
echo "Step 2: Starting Drasi Server (sources only - app creates queries dynamically)..."

cd "$DRASI_SERVER_ROOT"
RUST_LOG=info,drasi_server::sources::postgres=debug \
    ./target/release/drasi-server \
        --config "$CONFIG_PATH" \
        --plugins-dir "$PLUGINS_DIR" \
        "${SERVER_PLUGIN_ARGS[@]}" \
        > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo "Drasi Server started with PID: $DRASI_PID"
echo "Replication source will bootstrap initial data from PostgreSQL..."

# Wait for Drasi Server to be ready
if ! wait_for_service \
    "http://localhost:8280/health" \
    "Drasi Server" \
    "$DRASI_PID" \
    "$LOG_DIR/drasi-server.log"; then
    echo -e "${RED}✗ Drasi Server API is not responding${NC}"
    if kill -0 "$DRASI_PID" 2>/dev/null; then
        kill "$DRASI_PID" 2>/dev/null
    fi
    exit 1
fi

# Verify sources are running
echo "Verifying Drasi sources..."
SOURCE_STATUS=$(curl -s http://localhost:8280/api/v1/sources)
if echo "$SOURCE_STATUS" | grep -qi '"status":"running"'; then
    echo -e "PostgreSQL replication source: ${GREEN}✓ Running${NC}"
    echo -e "HTTP source: ${GREEN}✓ Running${NC}"
else
    echo -e "Sources: ${YELLOW}Starting...${NC}"
fi

# Give bootstrap time to complete
echo "Allowing time for bootstrap to complete..."
sleep 3

# Step 3: Install React app dependencies (if needed)
echo ""
echo "Step 3: Setting up React application..."
cd "$SCRIPT_DIR/app"
if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
else
    echo "Dependencies already installed"
fi

# Step 4: Start React app
echo "Starting React application..."
npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo "React app started with PID: $REACT_PID"

# Wait for React app (Vite dev server runs on 5273)
wait_for_service "http://localhost:5273" "React application"

# Step 5: Install Python dependencies
echo ""
echo "Step 4: Setting up price generator and trading API..."
cd "$SCRIPT_DIR/mock-generator"
if ! python3 -c "import requests, flask, psycopg2" 2>/dev/null; then
    echo "Installing Python dependencies..."
    pip3 install -r requirements.txt
else
    echo "Python dependencies already installed"
fi

# Step 6: Start trading API
echo "Starting Trading API server..."
python3 trading_api.py > "$LOG_DIR/trading-api.log" 2>&1 &
API_PID=$!
echo "Trading API started with PID: $API_PID"

# Wait for Trading API to be ready
wait_for_service "http://localhost:9200/health" "Trading API"

# Step 7: Start price generator
echo "Starting simple price generator..."
python3 simple_price_generator.py > "$LOG_DIR/price-generator.log" 2>&1 &
GENERATOR_PID=$!
echo "Price generator started with PID: $GENERATOR_PID"

# Summary
echo ""
echo "======================================"
echo -e "${GREEN}   Demo Started Successfully!${NC}"
echo "======================================"
echo ""
echo "Access the demo at:"
echo "  • Trading UI: http://localhost:5273"
echo "  • Broker Panel: http://localhost:9200/broker"
echo "  • Drasi Server UI: http://localhost:8280/ui?instance=trading-server"
echo "  • Drasi API: http://localhost:8280"
echo "  • Trading API: http://localhost:9200"
echo "  • HTTP Source: http://localhost:9100"
echo "  • SSE Stream: http://localhost:8281/events"
echo ""
echo "Process PIDs:"
echo "  • Drasi Server: $DRASI_PID"
echo "  • React App: $REACT_PID"
echo "  • Trading API: $API_PID"
echo "  • Price Generator: $GENERATOR_PID"
echo ""
echo "Logs are available at:"
echo "  • Drasi Server: $LOG_DIR/drasi-server.log"
echo "  • React App: $LOG_DIR/react-app.log"
echo "  • Trading API: $LOG_DIR/trading-api.log"
echo "  • Price Generator: $LOG_DIR/price-generator.log"
echo ""
echo "To stop the demo, run: ./stop-demo.sh"
echo ""

# Save PIDs for stop script
echo "$DRASI_PID" > /tmp/drasi-demo-server.pid
echo "$REACT_PID" > /tmp/drasi-demo-react.pid
echo "$API_PID" > /tmp/drasi-demo-api.pid
echo "$GENERATOR_PID" > /tmp/drasi-demo-generator.pid

# Keep script running and forward signals
trap "echo 'Stopping demo...'; kill $DRASI_PID $REACT_PID $API_PID $GENERATOR_PID 2>/dev/null; cd $SCRIPT_DIR/database && docker-compose down; exit" INT TERM

echo "Press Ctrl+C to stop all services"
wait