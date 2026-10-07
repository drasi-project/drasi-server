#!/bin/bash

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Builds a matched Drasi Server and the five plugins required by the trading
# demo. Shared origin checks require an explicitly selected, coherent local
# Cargo graph; the presence of a sibling checkout never selects it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRASI_SERVER_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_PATH="$SCRIPT_DIR/server/trading-sources-only.yaml"
PLUGINS_ROOT="$SCRIPT_DIR/plugins"
LOCAL_PLUGINS_DIR="$PLUGINS_ROOT/local"
SERVER_TARGET_DIR="$DRASI_SERVER_ROOT/target"
SERVER_BINARY="$SERVER_TARGET_DIR/release/drasi-server"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

fail() {
    echo -e "${RED}Error: $*${NC}" >&2
    exit 1
}

for command_name in cargo make python3 git; do
    command -v "$command_name" >/dev/null 2>&1 ||
        fail "Required command not found: $command_name"
done
python3 -c 'import sys; sys.exit("Python 3.11 or later is required" if sys.version_info < (3, 11) else 0)'

DRASI_CORE_ROOT="$(python3 "$DRASI_SERVER_ROOT/scripts/plugin_origin.py" local-workspace)"

mkdir -p "$PLUGINS_ROOT"

METADATA_FILE="$(mktemp "${TMPDIR:-/tmp}/drasi-trading-metadata.XXXXXX")"
VALIDATION_LOG="$(mktemp "${TMPDIR:-/tmp}/drasi-trading-validation.XXXXXX")"
STAGING_DIR="$PLUGINS_ROOT/.local-staging-$$"
BACKUP_DIR="$PLUGINS_ROOT/.local-backup-$$"

cleanup() {
    rm -f "$METADATA_FILE" "$VALIDATION_LOG"
    if [ -d "$STAGING_DIR" ]; then
        rm -rf "$STAGING_DIR"
    fi
    if [ -d "$BACKUP_DIR" ]; then
        if [ ! -e "$LOCAL_PLUGINS_DIR" ]; then
            mv "$BACKUP_DIR" "$LOCAL_PLUGINS_DIR" || true
        else
            rm -rf "$BACKUP_DIR"
        fi
    fi
}
trap cleanup EXIT

echo "Resolving Drasi Server dependencies..."
if ! (cd "$DRASI_SERVER_ROOT" && cargo metadata --locked --format-version 1 > "$METADATA_FILE"); then
    fail "Cargo could not resolve the Drasi Server dependency graph."
fi

echo -e "Using local drasi-core: ${GREEN}$DRASI_CORE_ROOT${NC}"
echo "Building Drasi Server against the resolved local dependencies..."
(cd "$DRASI_SERVER_ROOT" && CARGO_TARGET_DIR="$SERVER_TARGET_DIR" make build-release)

[ -x "$SERVER_BINARY" ] || fail "Drasi Server binary was not produced: $SERVER_BINARY"

CORE_TARGET_DIR="$(
    cargo metadata \
        --manifest-path "$DRASI_CORE_ROOT/Cargo.toml" \
        --locked \
        --format-version 1 \
        --no-deps |
        python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"

case "$(uname -s)" in
    Darwin)
        PLUGIN_EXT="dylib"
        PLUGIN_PREFIX="lib"
        ;;
    Linux)
        PLUGIN_EXT="so"
        PLUGIN_PREFIX="lib"
        ;;
    MINGW* | MSYS* | CYGWIN*)
        PLUGIN_EXT="dll"
        PLUGIN_PREFIX=""
        ;;
    *)
        fail "Unsupported platform: $(uname -s)"
        ;;
esac

REQUIRED_PLUGIN_CRATES=(
    drasi-source-http
    drasi-source-postgres
    drasi-bootstrap-scriptfile
    drasi-bootstrap-postgres
    drasi-reaction-sse
)
REQUIRED_PLUGIN_FILES=(
    "${PLUGIN_PREFIX}drasi_source_http.${PLUGIN_EXT}"
    "${PLUGIN_PREFIX}drasi_source_postgres.${PLUGIN_EXT}"
    "${PLUGIN_PREFIX}drasi_bootstrap_scriptfile.${PLUGIN_EXT}"
    "${PLUGIN_PREFIX}drasi_bootstrap_postgres.${PLUGIN_EXT}"
    "${PLUGIN_PREFIX}drasi_reaction_sse.${PLUGIN_EXT}"
)

echo "Building the five trading demo plugins..."
for crate_name in "${REQUIRED_PLUGIN_CRATES[@]}"; do
    cargo build \
        --manifest-path "$DRASI_CORE_ROOT/Cargo.toml" \
        --locked \
        --release \
        --lib \
        -p "$crate_name" \
        --features "$crate_name/dynamic-plugin"
done

mkdir -p "$STAGING_DIR"
for plugin_file in "${REQUIRED_PLUGIN_FILES[@]}"; do
    source_path="$CORE_TARGET_DIR/release/$plugin_file"
    [ -f "$source_path" ] || fail "Built plugin not found: $source_path"
    cp "$source_path" "$STAGING_DIR/$plugin_file"
done

echo "Validating local plugin ABI, target, and trading configuration..."
if ! "$SERVER_BINARY" validate \
    --config "$CONFIG_PATH" \
    --plugins-dir "$STAGING_DIR" \
    >"$VALIDATION_LOG" 2>&1; then
    cat "$VALIDATION_LOG" >&2
    fail "Drasi Server rejected the local plugin set."
fi

if grep -Eq 'plugin ABI mismatch|SDK version mismatch|target( triple)? mismatch|Failed to load plugin|\[WARN\].*not installed' "$VALIDATION_LOG" ||
    ! grep -Fq 'Plugins (5 loaded' "$VALIDATION_LOG"; then
    cat "$VALIDATION_LOG" >&2
    echo >&2
    echo -e "${RED}Local trading plugins are incompatible with this Drasi Server.${NC}" >&2
    echo "The server and plugins must resolve drasi-plugin-sdk and drasi-host-sdk" >&2
    echo "from the same patched drasi-core checkout." >&2
    exit 1
fi

python3 - \
    "$METADATA_FILE" \
    "$DRASI_SERVER_ROOT" \
    "$DRASI_CORE_ROOT" \
    "$SERVER_BINARY" \
    "$STAGING_DIR" \
    "${REQUIRED_PLUGIN_FILES[@]}" <<'PY'
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import sys

(
    metadata_path,
    server_root,
    core_root,
    server_binary,
    staging_dir,
    *plugin_files,
) = sys.argv[1:]

server_root = pathlib.Path(server_root).resolve()
core_root = pathlib.Path(core_root).resolve()
server_binary = pathlib.Path(server_binary).resolve()
staging_dir = pathlib.Path(staging_dir).resolve()

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
    commit = git_bytes(root, "rev-parse", "HEAD").decode().strip()
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
        "root": str(root),
        "commit": commit,
        "dirty": bool(status),
        "worktree_sha256": fingerprint.hexdigest(),
    }

metadata = json.loads(pathlib.Path(metadata_path).read_text())
sys.path.insert(0, str(server_root / "scripts"))
from plugin_origin import selected_packages
resolved_versions = {
    name: package["version"] for name, package in selected_packages(metadata).items()
}

version_output = subprocess.check_output([str(server_binary), "--version"], text=True)
abi_match = re.search(r"^plugin ABI:\s*(.+)$", version_output, re.MULTILINE)
if not abi_match:
    raise SystemExit("Unable to read the plugin ABI from the matched server")

rustc_output = subprocess.check_output(["rustc", "-vV"], text=True)
target_match = re.search(r"^host:\s*(.+)$", rustc_output, re.MULTILINE)
if not target_match:
    raise SystemExit("Unable to determine the native Rust target")

manifest = {
    "format_version": 1,
    "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "ffi_abi": abi_match.group(1),
    "target_triple": target_match.group(1),
    "resolved_versions": resolved_versions,
    "server_binary": str(server_binary),
    "server_sha256": sha256(server_binary),
    "plugins": {
        filename: sha256(staging_dir / filename)
        for filename in plugin_files
    },
    "drasi_server": git_state(server_root),
    "drasi_core": git_state(core_root),
}

(staging_dir / "local-build.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n"
)
PY

if [ -d "$LOCAL_PLUGINS_DIR" ]; then
    mv "$LOCAL_PLUGINS_DIR" "$BACKUP_DIR"
fi

if mv "$STAGING_DIR" "$LOCAL_PLUGINS_DIR"; then
    if [ -d "$BACKUP_DIR" ]; then
        rm -rf "$BACKUP_DIR"
    fi
else
    if [ -d "$BACKUP_DIR" ]; then
        mv "$BACKUP_DIR" "$LOCAL_PLUGINS_DIR"
    fi
    fail "Failed to install the validated local plugin set."
fi

echo
echo -e "${GREEN}Local trading runtime is ready.${NC}"
echo "  Server:  $SERVER_BINARY"
echo "  Plugins: $LOCAL_PLUGINS_DIR"
echo
echo "Start it with:"
echo "  ./examples/trading/start-demo.sh --plugin-source local"
