#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
if [[ $# -gt 0 ]]; then
    [[ $# -eq 1 && "$1" == "--allow-sudo" ]] \
        || { echo "Usage: $0 [--allow-sudo]" >&2; exit 1; }
fi

if [[ ! -e "$root/../drasi-core" && ! -L "$root/../drasi-core" ]]; then
    bash "$root/scripts/prepare-core.sh" "$@" >&2
fi
mode="$(python3 "$root/scripts/plugin_origin.py" mode)"
case "$mode" in
    registry) bash "$root/scripts/prepare-core.sh" --check >&2 ;;
    local) ;; # Matching local SDK development intentionally has a different source revision.
    *) echo "Unsupported plugin dependency origin: $mode" >&2; exit 1 ;;
esac
printf '%s\n' "$mode"
