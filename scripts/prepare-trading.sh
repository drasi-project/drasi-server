#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
mode="$(bash "$root/scripts/prepare-build.sh" "$@")"

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

# npm prepares the file dependency during app installation; its build tools must exist first.
npm --prefix "$root/dev-tools/react" ci >&2
npm --prefix "$root/dev-tools/react" run build >&2
npm --prefix "$root/examples/trading/app" ci >&2
printf '%s\n' "$mode"
