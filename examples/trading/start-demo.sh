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
PLUGIN_SOURCE="local"

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
  local     Use the matching native libraries prepared by ./build-local-plugins.sh (default).
  registry  Unavailable until the matching native Trading libraries are published.
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
    local) ;;
    registry)
        echo "Native Trading requires matching unpublished local PG/HTTP libraries. Run build-local-plugins.sh; registry fallback is not supported for this configuration." >&2
        exit 1
        ;;
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

# Shared with the devcontainer: validate the locked graph and prepare the real UI.
bash "$DRASI_SERVER_ROOT/scripts/prepare-trading.sh" --plugin-source "$PLUGIN_SOURCE"

# Keep registry and local artifacts isolated so they can never be mixed.
CONFIG_PATH="$BASE_CONFIG"
SERVER_PLUGIN_ARGS=()
if [ "$PLUGIN_SOURCE" = "registry" ]; then
    PLUGINS_DIR="$SCRIPT_DIR/plugins/registry"
    mkdir -p "$PLUGINS_DIR"
else
    PLUGINS_DIR="$SCRIPT_DIR/plugins/local"
    LOCAL_MANIFEST="$PLUGINS_DIR/local-build.json"

    if [ ! -f "$LOCAL_MANIFEST" ]; then
        echo -e "${RED}Local trading plugins have not been prepared.${NC}"
        echo
        echo "Enable the local [patch.crates-io] entries in Cargo.toml, then run:"
        echo "  ./examples/trading/build-local-plugins.sh"
        exit 1
    fi

    LOCAL_CORE="$(python3 "$DRASI_SERVER_ROOT/scripts/plugin_origin.py" local-workspace)"
    if ! python3 - "$LOCAL_MANIFEST" "$DRASI_SERVER_ROOT/target/release/drasi-server" "$PLUGINS_DIR" "$DRASI_SERVER_ROOT" "$LOCAL_CORE" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

manifest_path = pathlib.Path(sys.argv[1])
server_binary = pathlib.Path(sys.argv[2]).resolve()
plugins_dir = pathlib.Path(sys.argv[3]).resolve()
source_roots = {
    "drasi_server": pathlib.Path(sys.argv[4]).resolve(),
    "drasi_core": pathlib.Path(sys.argv[5]).resolve(),
}

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

if sys.platform == "darwin":
    prefix, extension = "lib", "dylib"
elif sys.platform == "win32":
    prefix, extension = "", "dll"
else:
    prefix, extension = "lib", "so"
required_plugins = {
    f"{prefix}drasi_{kind}.{extension}"
    for kind in ("computation_source_http", "computation_source_postgres",
                 "computation_bootstrap_http", "computation_bootstrap_postgres", "reaction_sse")
}
if set(manifest.get("plugins", {})) != required_plugins:
    errors.append("manifest must contain exactly the five required trading plugins")
for filename in sorted(required_plugins):
    expected_hash = manifest.get("plugins", {}).get(filename)
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
    if pathlib.Path(root).resolve() != source_roots[key]:
        errors.append(f"{key} no longer resolves from the recorded checkout")
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

print(f"Matched native plugin directory: {plugins_dir}")
PY
    then
        exit 1
    fi

    SERVER_PLUGIN_ARGS=(--skip-verification)

    VALIDATION_LOG="$LOG_DIR/local-plugin-validation.log"
    if ! "$DRASI_SERVER_ROOT/target/release/drasi-server" validate \
        --config "$CONFIG_PATH" \
        --plugins-dir "$PLUGINS_DIR" \
        >"$VALIDATION_LOG" 2>&1 ||
        grep -Eq 'plugin ABI mismatch|SDK version mismatch|target( triple)? mismatch|Failed to load plugin|\[WARN\].*not installed' "$VALIDATION_LOG"; then
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

# Configured native slots are operator-owned. Never recreate or reset one here.
echo "Verifying PostgreSQL replication setup..."
for slot in drasi_trading_slot drasi_broker_slot; do
    exists="$(docker-compose exec -T postgres psql -XAt -v ON_ERROR_STOP=1 -U drasi_user -d trading_demo \
        -c "SELECT count(*) FROM pg_replication_slots WHERE slot_name='$slot' AND plugin='pgoutput' AND database=current_database();")"
    if [ "$exists" != 1 ]; then
        echo "Configured slot $slot is missing or incompatible. Provision the documented database explicitly; startup will not replace it." >&2
        exit 1
    fi
done
for publication in drasi_trading_pub drasi_broker_pub; do
    exists="$(docker-compose exec -T postgres psql -XAt -v ON_ERROR_STOP=1 -U drasi_user -d trading_demo \
        -c "SELECT count(*) FROM pg_publication WHERE pubname='$publication';")"
    if [ "$exists" != 1 ]; then
        echo "Configured publication $publication is missing. Startup does not alter existing database objects." >&2
        exit 1
    fi
done

echo "Starting the Trading API before native HTTP initialization..."
cleanup_processes() {
    for pid in "${DRASI_PID:-}" "${REACT_PID:-}" "${API_PID:-}" "${GENERATOR_PID:-}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" || echo "Unable to signal owned process $pid" >&2
        fi
    done
    for pid in "${DRASI_PID:-}" "${REACT_PID:-}" "${API_PID:-}" "${GENERATOR_PID:-}"; do
        if [ -n "$pid" ]; then
            if wait "$pid" 2>/dev/null; then
                :
            else
                status=$?
                case "$status" in
                    127 | 130 | 143) ;;
                    *) echo "Owned process $pid exited with status $status" >&2 ;;
                esac
            fi
        fi
    done
}
trap cleanup_processes EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cd "$SCRIPT_DIR/mock-generator"
if ! python3 -c "import requests, flask, flask_cors, psycopg2" 2>/dev/null; then
    echo "Install the Trading API requirements in your Python environment: pip install -r mock-generator/requirements.txt" >&2
    exit 1
fi
python3 trading_api.py > "$LOG_DIR/trading-api.log" 2>&1 &
API_PID=$!
wait_for_service "http://localhost:${TRADING_API_PORT:-9200}/health" "Trading API" "$API_PID" "$LOG_DIR/trading-api.log"

GENERATOR="$DRASI_SERVER_ROOT/target/release/examples/native-postgres"
RECIPE="$SCRIPT_DIR/server/trading-native-recipe.json"
[ -x "$GENERATOR" ] || { echo "Rebuild the native Trading configuration generator." >&2; exit 1; }
DRASI_PG_POSTGRES_STOCKS_SETTINGS="$("$GENERATOR" settings "$RECIPE" postgres-stocks)"
DRASI_PG_POSTGRES_BROKER_SETTINGS="$("$GENERATOR" settings "$RECIPE" postgres-broker)"
export TRADING_SNAPSHOT_URL="${TRADING_SNAPSHOT_URL:-http://localhost:${TRADING_API_PORT:-9200}/api/initial-prices}"
DRASI_HTTP_PRICE_FEED_SETTINGS="$("$GENERATOR" settings "$RECIPE" price-feed)"
export DRASI_PG_POSTGRES_STOCKS_SETTINGS DRASI_PG_POSTGRES_BROKER_SETTINGS DRASI_HTTP_PRICE_FEED_SETTINGS
export DRASI_HTTP_PRICE_FEED_HOST="${DRASI_HTTP_PRICE_FEED_HOST:-0.0.0.0}"
export DRASI_HTTP_PRICE_FEED_PORT="${DRASI_HTTP_PRICE_FEED_PORT:-9100}"
export TRADING_PRICE_SOURCE_URL="${TRADING_PRICE_SOURCE_URL:-http://localhost:$DRASI_HTTP_PRICE_FEED_PORT}"

sleep 2

# Step 2: Start Drasi Server
echo ""
echo "Step 2: Starting Drasi Server (the app installs its complete native group)..."

cd "$DRASI_SERVER_ROOT"
RUST_LOG=info,drasi_server::sources::postgres=debug \
    ./target/release/drasi-server \
        --config "$CONFIG_PATH" \
        --plugins-dir "$PLUGINS_DIR" \
        "${SERVER_PLUGIN_ARGS[@]}" \
        > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo "Drasi Server started with PID: $DRASI_PID"
echo "PostgreSQL and HTTP initialization will begin when the app installs all query owners."

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

echo "The empty instance is ready for the app-owned native computation bundle."

# Step 3: Install React app dependencies (if needed)
echo ""
echo "Step 3: Setting up React application..."
cd "$SCRIPT_DIR/app"
echo "Locked package and app dependencies are prepared"

# Step 4: Start React app
echo "Starting React application..."
npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo "React app started with PID: $REACT_PID"

# Wait for React app (Vite dev server runs on 5273)
wait_for_service "http://localhost:5273" "React application"

# Live HTTP prices are gated on all actual query owners being Running.
cd "$SCRIPT_DIR/mock-generator"

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
echo "Press Ctrl+C to stop all services"
wait