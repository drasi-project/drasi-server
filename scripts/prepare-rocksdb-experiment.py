#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

"""Apply the temporary, commit-pinned RocksDB override to a nightly candidate."""

import os
from pathlib import Path
import subprocess
import tomllib

from plugin_origin import EXPERIMENT_REPOSITORY, EXPERIMENT_REVISION


def prepare(manifest):
    original = manifest.read_text()
    if "patch" in tomllib.loads(original):
        raise ValueError("RocksDB experiment requires a manifest without existing source patches")
    manifest.write_text(original + f"""

[patch.crates-io]
drasi-index-rocksdb = {{ git = "{EXPERIMENT_REPOSITORY}", rev = "{EXPERIMENT_REVISION}" }}

[patch."{EXPERIMENT_REPOSITORY}"]
drasi-core = {{ version = "=0.5.10" }}
drasi-query-ast = {{ version = "=0.3.5" }}
""")
    # Keep workspace path dependencies from introducing duplicate core/AST types.
    subprocess.run(["cargo", "update", "--package", "drasi-index-rocksdb"], check=True)


if __name__ == "__main__":
    if os.environ.get("DRASI_ROCKSDB_EXPERIMENT") != "1":
        raise SystemExit("RocksDB experiment must be explicitly enabled")
    prepare(Path("Cargo.toml"))
    print(f"Experimental RocksDB index: {EXPERIMENT_REPOSITORY}@{EXPERIMENT_REVISION}")
