#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "Usage: $0" >&2; exit 1; }
mode="$(python3 "$root/scripts/plugin_origin.py" mode)"
case "$mode" in
    registry|local) ;;
    *) echo "Unsupported plugin dependency origin: $mode" >&2; exit 1 ;;
esac
printf '%s\n' "$mode"
