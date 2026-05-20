#!/bin/bash
# Post-create script for the Drasi Server Trading Demo dev container.
#
# Keep this small: install OS deps, build the server + UI, then hand off to
# start-demo.sh. Plugins are auto-installed by drasi-server at startup
# because the trading config sets `autoInstallPlugins: true`.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "🔧 Initializing Drasi Server Trading Demo environment..."

# Ensure the shared Docker network exists.
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# System build dependencies (sudo required).
echo "📦 Installing system dependencies..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    libjq-dev \
    libonig-dev \
    curl

# jq-sys crate needs JQ_LIB_DIR (architecture-aware). Persist for future shells.
JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
export JQ_LIB_DIR
grep -q 'JQ_LIB_DIR' ~/.bashrc 2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.bashrc
grep -q 'JQ_LIB_DIR' ~/.zshrc  2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.zshrc 2>/dev/null || true

cd "$DRASI_SERVER_ROOT"
chmod +x examples/trading/start-demo.sh examples/trading/stop-demo.sh 2>/dev/null || true

# target/ is bind-mounted from the host; wipe any host-built artifacts so the
# in-container build always produces native Linux ELF / .so files.
echo "🧹 Cleaning target/release/ for a fresh in-container build..."
rm -rf "$DRASI_SERVER_ROOT/target/release"

echo "🔨 Building Drasi Server + Web UI (this may take several minutes)..."
make build-release

# Hand off to the canonical start script. It will:
#   - start postgres via docker-compose
#   - start drasi-server (auto-installs missing plugins from the OCI registry)
#   - npm install + start the React app
#   - pip install + start the trading API and price generator
echo "🚀 Starting Trading Demo via start-demo.sh..."
exec bash examples/trading/start-demo.sh
#!/bin/bash
# Post-create script for Drasi Server Trading Demo.
#
# Self-contained: installs system deps, builds drasi-server + Web UI from a
# clean target/, downloads required plugins from the OCI registry, and starts
# PostgreSQL, drasi-server, the React app, the trading API, and the price
# generator.
#
# This script always wipes target/release/ before building so we never have to
# reconcile artifacts left behind by a host build (e.g. macOS .dylib).

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEMO_DIR="$DRASI_SERVER_ROOT/examples/trading"
LOG_DIR="$DEMO_DIR/logs"
mkdir -p "$LOG_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "🔧 Initializing Drasi Server Trading Demo environment..."

# ---------------------------------------------------------------------------
# 1. Shared Docker network (also created by initializeCommand on the host).
# ---------------------------------------------------------------------------
echo "🌐 Ensuring shared Docker network exists..."
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# ---------------------------------------------------------------------------
# 2. System build dependencies (sudo required; only available in post-create).
# ---------------------------------------------------------------------------
echo "📦 Installing system dependencies..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    libjq-dev \
    libonig-dev \
    curl

# jq-sys crate needs JQ_LIB_DIR (architecture-aware). Persist for future shells.
JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
export JQ_LIB_DIR
grep -q 'JQ_LIB_DIR' ~/.bashrc 2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.bashrc
grep -q 'JQ_LIB_DIR' ~/.zshrc  2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.zshrc 2>/dev/null || true

chmod +x "$DEMO_DIR/start-demo.sh" "$DEMO_DIR/stop-demo.sh" 2>/dev/null || true

# ---------------------------------------------------------------------------
# 3. Build drasi-server + Web UI from a clean target/release/.
# ---------------------------------------------------------------------------
cd "$DRASI_SERVER_ROOT"

echo -e "${YELLOW}🧹 Cleaning target/release/ for a fresh in-container build...${NC}"
rm -rf "$DRASI_SERVER_ROOT/target/release"

echo -e "${YELLOW}🔨 Building Drasi Server + Web UI (this may take several minutes)...${NC}"
make build-release

# ---------------------------------------------------------------------------
# 4. Install plugins required by the trading config from the OCI registry.
# ---------------------------------------------------------------------------
PLUGINS_DIR="$DRASI_SERVER_ROOT/target/release/plugins"
mkdir -p "$PLUGINS_DIR"

NEEDED_PLUGINS=(
    "source/http"
    "source/postgres"
    "bootstrap/scriptfile"
    "bootstrap/postgres"
)

DRASI_BIN="$DRASI_SERVER_ROOT/target/release/drasi-server"
for ref in "${NEEDED_PLUGINS[@]}"; do
    echo -e "${YELLOW}⬇️  Installing plugin from registry: $ref${NC}"
    "$DRASI_BIN" --skip-verification --plugins-dir "$PLUGINS_DIR" \
        plugin install "$ref"
done

# ---------------------------------------------------------------------------
# Helper: wait for an HTTP endpoint to respond 200/204.
# ---------------------------------------------------------------------------
wait_for_service() {
    local url=$1 name=$2 max=${3:-30} i=0
    echo -n "Waiting for $name..."
    while [ $i -lt $max ]; do
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|204"; then
            echo -e " ${GREEN}✓${NC}"; return 0
        fi
        sleep 2; i=$((i+1)); echo -n "."
    done
    echo -e " ${RED}✗${NC}"; return 1
}

# ---------------------------------------------------------------------------
# 5. Start PostgreSQL via docker-compose.
# ---------------------------------------------------------------------------
echo ""
echo "Step 1: Starting PostgreSQL database..."
cd "$DEMO_DIR/database"
docker-compose up -d

echo -n "Waiting for PostgreSQL..."
i=0
while [ $i -lt 30 ]; do
    if docker-compose exec -T postgres pg_isready -U drasi_user -d trading_demo >/dev/null 2>&1; then
        echo -e " ${GREEN}✓${NC}"; break
    fi
    sleep 2; i=$((i+1)); echo -n "."
done
if [ $i -eq 30 ]; then
    echo -e " ${RED}✗${NC}"
    echo "PostgreSQL failed to start. Logs: docker-compose logs postgres"
    exit 1
fi

# Ensure the replication publication exists.
PUB_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t \
    -c "SELECT pubname FROM pg_publication WHERE pubname = 'drasi_trading_pub';" | tr -d ' ')
if [ -z "$PUB_EXISTS" ]; then
    echo "Creating publication drasi_trading_pub..."
    docker-compose exec -T postgres psql -U postgres -d trading_demo \
        -c "CREATE PUBLICATION drasi_trading_pub FOR TABLE stocks, portfolio, stock_prices;"
fi
sleep 2

# ---------------------------------------------------------------------------
# 6. Start Drasi Server.
# ---------------------------------------------------------------------------
echo ""
echo "Step 2: Starting Drasi Server..."
cd "$DRASI_SERVER_ROOT"
RUST_LOG=info,drasi_server::sources::postgres=debug \
    nohup ./target/release/drasi-server \
        --config "examples/trading/server/trading-sources-only.yaml" \
        > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo "Drasi Server PID: $DRASI_PID"
sleep 2

if ! kill -0 $DRASI_PID 2>/dev/null; then
    echo -e "${RED}✗ Drasi Server failed to start${NC}"
    tail -20 "$LOG_DIR/drasi-server.log"
    exit 1
fi

if ! wait_for_service "http://localhost:8280/health" "Drasi Server"; then
    echo -e "${RED}✗ Drasi Server API not responding${NC}"
    tail -50 "$LOG_DIR/drasi-server.log"
    kill $DRASI_PID 2>/dev/null || true
    exit 1
fi
sleep 3

# ---------------------------------------------------------------------------
# 7. React app: install deps + start.
# ---------------------------------------------------------------------------
echo ""
echo "Step 3: Setting up React application..."
cd "$DEMO_DIR/app"
if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
fi

echo "Starting React application..."
nohup npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo "React app PID: $REACT_PID"
wait_for_service "http://localhost:5273" "React application" || true

# ---------------------------------------------------------------------------
# 8. Trading API + price generator.
# ---------------------------------------------------------------------------
echo ""
echo "Step 4: Setting up trading API and price generator..."
cd "$DEMO_DIR/mock-generator"
if ! python3 -c "import requests, flask, psycopg2" 2>/dev/null; then
    echo "Installing Python dependencies..."
    pip3 install -r requirements.txt
fi

echo "Starting Trading API..."
nohup python3 trading_api.py > "$LOG_DIR/trading-api.log" 2>&1 &
API_PID=$!
echo "Trading API PID: $API_PID"
wait_for_service "http://localhost:9200/health" "Trading API" || true

echo "Starting price generator..."
nohup python3 simple_price_generator.py > "$LOG_DIR/price-generator.log" 2>&1 &
GENERATOR_PID=$!
echo "Price generator PID: $GENERATOR_PID"

# Persist PIDs so stop-demo.sh can clean up.
echo "$DRASI_PID"     > /tmp/drasi-demo-server.pid
echo "$REACT_PID"     > /tmp/drasi-demo-react.pid
echo "$API_PID"       > /tmp/drasi-demo-api.pid
echo "$GENERATOR_PID" > /tmp/drasi-demo-generator.pid

echo ""
echo "======================================"
echo -e "${GREEN}   Trading Demo Started${NC}"
echo "======================================"
echo "  Trading UI:      http://localhost:5273"
echo "  Drasi API:       http://localhost:8280"
echo "  Drasi Server UI: http://localhost:8280/ui?instance=trading-server"
echo "  Trading API:     http://localhost:9200"
echo "  HTTP Source:     http://localhost:9100"
echo ""
echo "  Logs: $LOG_DIR/{drasi-server,react-app,trading-api,price-generator}.log"
echo "  To stop: cd examples/trading && ./stop-demo.sh"
#!/bin/bash
# Post-create script for Drasi Server Trading Demo
#
# Self-contained: installs system deps, builds drasi-server + UI + plugins,
# then starts PostgreSQL, drasi-server, the React app, the trading API,
# and the price generator. Does NOT call examples/trading/start-demo.sh.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEMO_DIR="$DRASI_SERVER_ROOT/examples/trading"
LOG_DIR="$DEMO_DIR/logs"
mkdir -p "$LOG_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "🔧 Initializing Drasi Server Trading Demo environment..."

# ---------------------------------------------------------------------------
# 1. Shared Docker network (initializeCommand on host also creates this).
# ---------------------------------------------------------------------------
echo "🌐 Ensuring shared Docker network exists..."
docker network inspect drasi-network >/dev/null 2>&1 || docker network create drasi-network

# ---------------------------------------------------------------------------
# 2. System build dependencies (sudo required; only available in post-create).
# ---------------------------------------------------------------------------
echo "📦 Installing system dependencies..."
sudo apt-get update && sudo apt-get install -y \
    postgresql-client \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    clang \
    libclang-dev \
    libjq-dev \
    libonig-dev \
    curl \
    file

# jq-sys crate needs JQ_LIB_DIR (architecture-aware). Persist for future shells.
JQ_LIB_DIR="/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
export JQ_LIB_DIR
grep -q 'JQ_LIB_DIR' ~/.bashrc 2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.bashrc
grep -q 'JQ_LIB_DIR' ~/.zshrc  2>/dev/null || echo "export JQ_LIB_DIR=\"$JQ_LIB_DIR\"" >> ~/.zshrc 2>/dev/null || true

chmod +x "$DEMO_DIR/start-demo.sh" "$DEMO_DIR/stop-demo.sh" 2>/dev/null || true

# ---------------------------------------------------------------------------
# 3. Build drasi-server (+ UI) and install plugins from the OCI registry.
#
# IMPORTANT: target/ is bind-mounted from the host. If the user previously
# built on macOS (Mach-O / .dylib), those artifacts are unusable inside this
# Linux container. Detect non-ELF binaries and wipe target/release/ so the
# build below produces native Linux ELF / .so files.
# ---------------------------------------------------------------------------
cd "$DRASI_SERVER_ROOT"

if [ -f "$DRASI_SERVER_ROOT/target/release/drasi-server" ]; then
    if ! file "$DRASI_SERVER_ROOT/target/release/drasi-server" | grep -q 'ELF'; then
        echo -e "${YELLOW}⚠️  Detected non-Linux artifacts in target/release/ (host-built).${NC}"
        echo -e "${YELLOW}    Wiping target/release/ for a fresh in-container build...${NC}"
        rm -rf "$DRASI_SERVER_ROOT/target/release"
    fi
fi

if [ ! -f "$DRASI_SERVER_ROOT/target/release/drasi-server" ]; then
    echo -e "${YELLOW}🔨 Building Drasi Server + Web UI (this may take several minutes)...${NC}"
    make build-release
elif [ ! -d "$DRASI_SERVER_ROOT/ui/dist" ]; then
    echo -e "${YELLOW}🔨 Web UI missing; building UI...${NC}"
    make build-ui
else
    echo "drasi-server binary and UI already present."
fi

# Install plugins required by the trading config from the OCI registry.
# (Avoids needing a sibling ../drasi-core checkout.)
PLUGINS_DIR="$DRASI_SERVER_ROOT/target/release/plugins"
mkdir -p "$PLUGINS_DIR"

# Wipe any non-ELF plugin artifacts left over from the host (e.g. macOS .dylib).
shopt -s nullglob
for f in "$PLUGINS_DIR"/*; do
    if ! file "$f" | grep -q 'ELF'; then
        echo -e "${YELLOW}    Removing non-Linux plugin artifact: $(basename "$f")${NC}"
        rm -f "$f"
    fi
done
shopt -u nullglob

NEEDED_PLUGINS=(
    "source/http"
    "source/postgres"
    "bootstrap/scriptfile"
    "bootstrap/postgres"
)

DRASI_BIN="$DRASI_SERVER_ROOT/target/release/drasi-server"
for ref in "${NEEDED_PLUGINS[@]}"; do
    # Plugin filenames follow `libdrasi_<category>_<name>.so`, where category
    # is source / reaction / bootstrap. Scope the existence check to the
    # category so e.g. `bootstrap/postgres` doesn't get a false hit on
    # `libdrasi_source_postgres.so`.
    category="${ref%%/*}"
    name="${ref##*/}"
    if ls "$PLUGINS_DIR"/libdrasi_${category}_${name}.so >/dev/null 2>&1; then
        echo "Plugin already installed: $ref"
    else
        echo -e "${YELLOW}⬇️  Installing plugin from registry: $ref${NC}"
        "$DRASI_BIN" --skip-verification --plugins-dir "$PLUGINS_DIR" \
            plugin install "$ref"
    fi
done

# ---------------------------------------------------------------------------
# Helper: wait for an HTTP endpoint to respond 200/204.
# ---------------------------------------------------------------------------
wait_for_service() {
    local url=$1 name=$2 max=${3:-30} i=0
    echo -n "Waiting for $name..."
    while [ $i -lt $max ]; do
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|204"; then
            echo -e " ${GREEN}✓${NC}"; return 0
        fi
        sleep 2; i=$((i+1)); echo -n "."
    done
    echo -e " ${RED}✗${NC}"; return 1
}

# ---------------------------------------------------------------------------
# 4. Start PostgreSQL via docker-compose.
# ---------------------------------------------------------------------------
echo ""
echo "Step 1: Starting PostgreSQL database..."
cd "$DEMO_DIR/database"
docker-compose up -d

echo -n "Waiting for PostgreSQL..."
i=0
while [ $i -lt 30 ]; do
    if docker-compose exec -T postgres pg_isready -U drasi_user -d trading_demo >/dev/null 2>&1; then
        echo -e " ${GREEN}✓${NC}"; break
    fi
    sleep 2; i=$((i+1)); echo -n "."
done
if [ $i -eq 30 ]; then
    echo -e " ${RED}✗${NC}"
    echo "PostgreSQL failed to start. Logs: docker-compose logs postgres"
    exit 1
fi

# Ensure replication publication exists.
PUB_EXISTS=$(docker-compose exec -T postgres psql -U drasi_user -d trading_demo -t \
    -c "SELECT pubname FROM pg_publication WHERE pubname = 'drasi_trading_pub';" | tr -d ' ')
if [ -z "$PUB_EXISTS" ]; then
    echo "Creating publication drasi_trading_pub..."
    docker-compose exec -T postgres psql -U postgres -d trading_demo \
        -c "CREATE PUBLICATION drasi_trading_pub FOR TABLE stocks, portfolio, stock_prices;"
fi
sleep 2

# ---------------------------------------------------------------------------
# 5. Start Drasi Server.
# ---------------------------------------------------------------------------
echo ""
echo "Step 2: Starting Drasi Server..."
cd "$DRASI_SERVER_ROOT"
RUST_LOG=info,drasi_server::sources::postgres=debug \
    nohup ./target/release/drasi-server \
        --config "examples/trading/server/trading-sources-only.yaml" \
        > "$LOG_DIR/drasi-server.log" 2>&1 &
DRASI_PID=$!
echo "Drasi Server PID: $DRASI_PID"
sleep 2

if ! kill -0 $DRASI_PID 2>/dev/null; then
    echo -e "${RED}✗ Drasi Server failed to start${NC}"
    tail -20 "$LOG_DIR/drasi-server.log"
    exit 1
fi

if ! wait_for_service "http://localhost:8280/health" "Drasi Server"; then
    echo -e "${RED}✗ Drasi Server API not responding${NC}"
    tail -50 "$LOG_DIR/drasi-server.log"
    kill $DRASI_PID 2>/dev/null || true
    exit 1
fi
sleep 3

# ---------------------------------------------------------------------------
# 6. React app: install deps + start.
# ---------------------------------------------------------------------------
echo ""
echo "Step 3: Setting up React application..."
cd "$DEMO_DIR/app"
if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
fi

echo "Starting React application..."
nohup npm run dev > "$LOG_DIR/react-app.log" 2>&1 &
REACT_PID=$!
echo "React app PID: $REACT_PID"
wait_for_service "http://localhost:5273" "React application" || true

# ---------------------------------------------------------------------------
# 7. Trading API + price generator.
# ---------------------------------------------------------------------------
echo ""
echo "Step 4: Setting up trading API and price generator..."
cd "$DEMO_DIR/mock-generator"
if ! python3 -c "import requests, flask, psycopg2" 2>/dev/null; then
    echo "Installing Python dependencies..."
    pip3 install -r requirements.txt
fi

echo "Starting Trading API..."
nohup python3 trading_api.py > "$LOG_DIR/trading-api.log" 2>&1 &
API_PID=$!
echo "Trading API PID: $API_PID"
wait_for_service "http://localhost:9200/health" "Trading API" || true

echo "Starting price generator..."
nohup python3 simple_price_generator.py > "$LOG_DIR/price-generator.log" 2>&1 &
GENERATOR_PID=$!
echo "Price generator PID: $GENERATOR_PID"

# Persist PIDs so stop-demo.sh can clean up.
echo "$DRASI_PID"     > /tmp/drasi-demo-server.pid
echo "$REACT_PID"     > /tmp/drasi-demo-react.pid
echo "$API_PID"       > /tmp/drasi-demo-api.pid
echo "$GENERATOR_PID" > /tmp/drasi-demo-generator.pid

echo ""
echo "======================================"
echo -e "${GREEN}   Trading Demo Started${NC}"
echo "======================================"
echo "  Trading UI:      http://localhost:5273"
echo "  Drasi API:       http://localhost:8280"
echo "  Drasi Server UI: http://localhost:8280/ui?instance=trading-server"
echo "  Trading API:     http://localhost:9200"
echo "  HTTP Source:     http://localhost:9100"
echo ""
echo "  Logs: $LOG_DIR/{drasi-server,react-app,trading-api,price-generator}.log"
echo "  To stop: cd examples/trading && ./stop-demo.sh"
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

# Launch the demo in the background. start-demo.sh handles:
#   - building drasi-server + UI (make build-release)
#   - building local plugins (make build-local-plugins)
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
