#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

if [[ ! -e "$root/../drasi-core" && ! -L "$root/../drasi-core" ]]; then
    bash "$root/scripts/prepare-core.sh" >&2
fi
mode="$(python3 "$root/scripts/plugin_origin.py" mode)"
case "$mode" in
    registry) bash "$root/scripts/prepare-core.sh" --check >&2 ;;
    local) ;; # Deliberate matching local SDK development is a separate source mode.
    *) echo "Unsupported plugin dependency origin: $mode" >&2; exit 1 ;;
esac

# Always build this checkout; an existing published executable is not pin evidence.
make -C "$root" build-release >&2
if [[ "$mode" == registry ]]; then
    bash "$root/scripts/prepare-core.sh" --check >&2
    python3 "$root/scripts/install_plugins.py" \
        --server-bin "$root/target/release/drasi-server" \
        --plugins-dir "$root/target/release/plugins" >&2
else
    make -C "$root" build-local-plugins >&2
fi
printf '%s\n' "$mode"
