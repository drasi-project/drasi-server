#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
"""Reuse the repository's immutable plugin policy for actual loaded metadata."""

import json
from pathlib import Path
import platform
import sys

source_root = Path(sys.argv[1]).resolve(strict=True)
sys.path.insert(0, str(source_root / "scripts"))
from install_plugins import group_pins, validate_loaded_plugins  # noqa: E402

plugins = json.loads(Path(sys.argv[2]).read_text())
validate_loaded_plugins(
    plugins["plugins"], group_pins("trading", platform.system(), platform.machine())
)
print("Loaded plugins match the reviewed signed hashes, versions and ABI.")
