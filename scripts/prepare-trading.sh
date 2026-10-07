#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source=registry
if [[ $# -eq 2 && "$1" == --plugin-source ]]; then
    source="$2"
elif [[ $# -ne 0 ]]; then
    echo "Usage: $0 [--plugin-source registry|local]" >&2
    exit 1
fi
case "$source" in
    registry|local) ;;
    *) echo "Invalid plugin source: $source" >&2; exit 1 ;;
esac
python3 -c 'import sys; sys.exit("Python 3.11 or later is required" if sys.version_info < (3, 11) else 0)'
mode="$(bash "$root/scripts/prepare-build.sh")"
if [[ "$source" != "$mode" ]]; then
    echo "Requested $source plugins, but Cargo resolves $mode dependencies." >&2
    echo "Select a coherent dependency graph explicitly; local mode never falls back to GHCR." >&2
    exit 1
fi

if [[ "$mode" == registry ]]; then
    # Local mode must preserve the matched binary for its freshness check.
    make -C "$root" build-release >&2
    python3 "$root/scripts/install_plugins.py" \
        --server-bin "$root/target/release/drasi-server" \
        --plugins-dir "$root/examples/trading/plugins/registry" >&2
fi

# npm prepares the file dependency during app installation; its build tools must exist first.
npm --prefix "$root/dev-tools/react" ci >&2
npm --prefix "$root/dev-tools/react" run build >&2
npm --prefix "$root/examples/trading/app" ci >&2
printf '%s\n' "$mode"
