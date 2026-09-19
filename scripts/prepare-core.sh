#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

set -euo pipefail

fail() {
    echo "Core preparation failed: $*" >&2
    exit 1
}

check_only=false
allow_sudo=false
if [[ $# -gt 0 ]]; then
    [[ $# -eq 1 ]] || fail "usage: $0 [--check | --allow-sudo]"
    case "$1" in
        --check) check_only=true ;;
        --allow-sudo) allow_sudo=true ;;
        *) fail "usage: $0 [--check | --allow-sudo]" ;;
    esac
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
sibling="${root%/*}/drasi-core"
repository="https://github.com/drasi-project/drasi-core.git"
pin_file="$root/.drasi-core-revision"
[[ -f "$pin_file" ]] || fail "missing reviewed revision file: $pin_file"
revision="$(cat "$pin_file")"
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || fail "expected exactly one full commit SHA in $pin_file"

if [[ ! -e "$sibling" && ! -L "$sibling" ]]; then
    [[ "$check_only" == false ]] || fail "missing sibling checkout: $sibling"
    # Reserve only an absent destination; never reset or replace an existing checkout.
    if [[ "$allow_sudo" == true && ! -w "$(dirname "$sibling")" ]]; then
        sudo -n mkdir -- "$sibling" || fail "cannot reserve absent sibling: $sibling"
        sudo -n chown -h "$(id -u):$(id -g)" "$sibling" \
            || fail "cannot assign the newly reserved sibling to the current user: $sibling"
        [[ -d "$sibling" && ! -L "$sibling" ]] || fail "newly reserved sibling changed: $sibling"
    else
        mkdir "$sibling" || fail "cannot reserve absent sibling: $sibling"
    fi
    git -C "$sibling" init --quiet
    git -C "$sibling" remote add origin "$repository"
    git -C "$sibling" fetch --quiet --depth=1 origin "$revision" \
        || fail "cannot fetch $revision; incomplete checkout left at $sibling"
    git -C "$sibling" -c advice.detachedHead=false checkout --quiet --detach "$revision"
fi

[[ -d "$sibling" ]] || fail "existing sibling is not a usable directory: $sibling"
actual_root="$(git -C "$sibling" rev-parse --show-toplevel)" \
    || fail "existing sibling is not a Git worktree: $sibling"
actual_root="$(cd "$actual_root" && pwd -P)"
physical_root="$(cd "$sibling" && pwd -P)"
[[ "$actual_root" == "$physical_root" ]] \
    || fail "sibling must be the repository root, not a nested directory: $sibling"
actual_revision="$(git -C "$sibling" rev-parse --verify 'HEAD^{commit}')" \
    || fail "existing sibling has no committed HEAD: $sibling"
[[ "$actual_revision" == "$revision" ]] \
    || fail "expected $revision, found $actual_revision at $sibling; checkout was not modified"
status="$(git -C "$sibling" status --porcelain=v1 --untracked-files=all)" \
    || fail "cannot inspect source status: $sibling"
[[ -z "$status" ]] \
    || fail "existing sibling has uncommitted files: $sibling; checkout was not modified"

echo "Verified drasi-core $revision at $physical_root"
