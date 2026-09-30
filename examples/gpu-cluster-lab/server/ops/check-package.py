#!/usr/bin/env python3
"""Verify stock process ownership, plugin registration, provenance and clean shutdown."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid


ROOT = Path(__file__).resolve().parents[1]
FACTORIES = {
    "gpu.lab/telemetry-simulator",
    "gpu.lab/regorus-policy",
    "gpu.lab/placement-solver",
    "gpu.lab/resilience-assessor",
    "gpu.lab/plan-writer",
    "gpu.lab/runtime-status",
}


def run(*arguments):
    return subprocess.run(arguments, check=True, capture_output=True, text=True).stdout


def check(image):
    artifacts = ROOT / ".build"
    artifacts.mkdir(exist_ok=True)
    inspected = json.loads(run("docker", "image", "inspect", image))[0]
    assert inspected["Config"]["Entrypoint"] == ["/app/drasi-server"], "not the stock executable"
    image = inspected["Id"]
    name = f"gpu-stock-package-{uuid.uuid4().hex[:12]}"
    log_path = artifacts / "package-probe.log"
    with tempfile.TemporaryDirectory(prefix="package-probe-", dir=artifacts) as temporary, \
            log_path.open("w") as log:
        config = Path(temporary) / "server.json"
        config.write_text(json.dumps({
            "id": "gpu-package-probe",
            "host": "0.0.0.0",
            "port": 8080,
            "persistConfig": False,
            "persistIndex": False,
            "enableUi": False,
            "verifyPlugins": False,
            "autoInstallPlugins": False,
        }))
        process = subprocess.Popen([
            "docker", "run", "--name", name, "--network", "none", "--read-only",
            "--tmpfs", "/tmp", "-v", f"{config}:/probe.json:ro", image,
            "--config", "/probe.json", "--plugins-dir", "/app/plugins",
            "--skip-verification", "--disable-ui",
        ], stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 30
            while True:
                if process.poll() is not None:
                    raise RuntimeError(f"Stock Server exited during startup; see {log_path}")
                response = subprocess.run([
                    "docker", "exec", name, "curl", "--silent", "--show-error",
                    "--fail-with-body", "--max-time", "2",
                    "http://127.0.0.1:8080/api/v1/plugins/computation",
                ], capture_output=True, text=True)
                if response.returncode == 0:
                    metadata = json.loads(response.stdout)
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Stock API unavailable: {response.stderr}; see {log_path}")
                time.sleep(0.1)
            (artifacts / "native-plugin-metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
            plugins = metadata["plugins"]
            assert len(plugins) == 1, "GPU native plugin was not registered exactly once"
            plugin = plugins[0]
            assert plugin["plugin"]["id"] == "gpu-cluster-lab"
            assert {factory["implementation"]["name"] for factory in plugin["factories"]} == FACTORIES
            assert len(plugin["factories"]) == len(FACTORIES)
            standard = json.loads(run(
                "docker", "exec", name, "curl", "--silent", "--show-error",
                "--fail-with-body", "--max-time", "5", "http://127.0.0.1:8080/api/v1/plugins",
            ))
            loaded = {item["id"] for item in standard["plugins"] if item["status"] == "Loaded"}
            assert {"source/postgres", "bootstrap/postgres", "reaction/sse"} <= loaded
            run("docker", "exec", name, "sh", "-c",
                "cd /app && test -x /bin/kill && sha256sum -c artifacts.sha256")
            manifest_hash = run(
                "docker", "exec", name, "sha256sum", "/app/source-manifest.json"
            ).split()[0]
            expected = hashlib.sha256((artifacts / "runtime-src/source-manifest.json").read_bytes()).hexdigest()
            assert manifest_hash == expected, "image differs from the exported source manifest"
            result = {
                "image": image,
                "source_manifest_sha256": manifest_hash,
                "native_factories": sorted(FACTORIES),
                "loaded_plugins": sorted(loaded),
            }
        finally:
            if process.poll() is None:
                run("docker", "stop", "--signal", "SIGINT", "--timeout", "30", name)
            process.wait(timeout=40)
            state = run("docker", "inspect", "--format", "{{json .State}}", name)
            (artifacts / "package-probe-stop.json").write_text(state)
            run("docker", "rm", name)
            subprocess.run([
                sys.executable, str(ROOT.parent / "embedded/ops/check-stopped.py"), name,
            ], input=state, text=True, check=True)
    (artifacts / "package-check.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Stock Server loaded all GPU and standard plugins; fingerprints and exit 0 verified.")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: check-package.py <stock-runtime-image>")
    check(sys.argv[1])
