# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Makefile for Drasi Server

.PHONY: all build build-release build-static build-dynamic build-dynamic-release \
        build-dynamic-server build-dynamic-server-release build-dynamic-plugins build-dynamic-plugins-release \
        run run-release setup demo demo-cleanup \
        doctor validate clean clippy test test-static test-dynamic test-smoke test-smoke-static test-smoke-dynamic \
        fmt fmt-check help docker-build \
        submodule-update vscode-test

# Path to drasi-core workspace (relative to this Makefile)
DRASI_CORE_DIR := ../drasi-core

# All plugin crate names that support dynamic loading.
# These must be listed as dependencies in Cargo.toml (even if optional) so they
# can be built from this workspace with `cargo build -p <name>`.
# Platform plugins are excluded — they live only in drasi-core.
DYNAMIC_PLUGINS := \
	drasi-source-mock \
	drasi-source-http \
	drasi-source-grpc \
	drasi-source-postgres \
	drasi-source-mssql \
	drasi-reaction-log \
	drasi-reaction-http \
	drasi-reaction-http-adaptive \
	drasi-reaction-grpc \
	drasi-reaction-grpc-adaptive \
	drasi-reaction-sse \
	drasi-reaction-profiler \
	drasi-reaction-storedproc-postgres \
	drasi-reaction-storedproc-mysql \
	drasi-reaction-storedproc-mssql \
	drasi-bootstrap-postgres \
	drasi-bootstrap-mssql \
	drasi-bootstrap-scriptfile

# Build -p flags for all plugins
PLUGIN_PKG_FLAGS := $(foreach p,$(DYNAMIC_PLUGINS),-p $(p))

# Shared RUSTFLAGS for ALL dynamic builds (server + plugins).
# CRITICAL: server and plugins MUST be built in a SINGLE cargo invocation
# to ensure shared deps (serde, tokio, etc.) have identical symbol hashes.
#   -C prefer-dynamic    → share libstd/tokio with plugins (avoid dual-runtime)
#   --cfg ...            → enable drasi_plugin_init export in plugin crates
#   -C link-args=...     → RUNPATH=$ORIGIN so binary finds .so in its own dir
DYNAMIC_RUSTFLAGS := -C prefer-dynamic --cfg feature="dynamic-plugin" -C link-args=-Wl,-rpath,$$ORIGIN

# Default target
help:
	@echo "Drasi Server Development Commands"
	@echo ""
	@echo "Getting Started:"
	@echo "  make setup              - Check dependencies and create default config"
	@echo "  make run                - Build (debug) and run the server"
	@echo "  make run-release        - Build (release) and run the server"
	@echo "  make demo               - Run the getting-started example"
	@echo ""
	@echo "Static Build (all plugins linked into binary):"
	@echo "  make build              - Build debug binary"
	@echo "  make build-release      - Build release binary"
	@echo "  make build-static       - Alias for build-release"
	@echo ""
	@echo "Dynamic Build (plugins as shared libraries):"
	@echo "  make build-dynamic              - Build server + plugins (debug)"
	@echo "  make build-dynamic-release      - Build server + plugins (release)"
	@echo "  make build-dynamic-server       - Build only the server (debug)"
	@echo "  make build-dynamic-server-release - Build only the server (release)"
	@echo "  make build-dynamic-plugins      - Build only plugins (debug)"
	@echo "  make build-dynamic-plugins-release - Build only plugins (release)"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run all tests"
	@echo "  make test-static        - Run tests with builtin-plugins (default)"
	@echo "  make test-dynamic       - Run tests with dynamic-plugins feature"
	@echo "  make test-smoke         - Plugin smoke test (both static + dynamic)"
	@echo "  make test-smoke-static  - Plugin smoke test (static only)"
	@echo "  make test-smoke-dynamic - Plugin smoke test (dynamic only)"
	@echo "  make vscode-test        - Run VSCode extension tests"
	@echo ""
	@echo "Code Quality:"
	@echo "  make clippy             - Run linter"
	@echo "  make fmt                - Format code"
	@echo "  make fmt-check          - Check formatting"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build       - Build Docker image (IMAGE_PREFIX, DOCKER_TAG_VERSION)"
	@echo ""
	@echo "Utilities:"
	@echo "  make doctor             - Check system dependencies"
	@echo "  make validate           - Validate config file (CONFIG=path)"
	@echo "  make clean              - Clean build artifacts"
	@echo "  make demo-cleanup       - Stop demo containers"
	@echo "  make submodule-update   - Initialize/update git submodules"
	@echo ""

# === Getting Started ===

# Check dependencies and create config
setup: doctor
	@echo ""
	@echo "Building Drasi Server..."
	@cargo build
	@echo ""
	@if [ ! -f "config/server.yaml" ]; then \
		echo "Creating default configuration..."; \
		mkdir -p config; \
		./target/debug/drasi-server --config config/server.yaml 2>&1 | head -5 || true; \
	else \
		echo "Configuration already exists: config/server.yaml"; \
	fi
	@echo ""
	@echo "Setup complete! Run 'make run' to start the server."

# Build and run (debug mode)
run:
	cargo run

# Build and run with custom config
run-config:
	@if [ -z "$(CONFIG)" ]; then \
		echo "Usage: make run-config CONFIG=path/to/config.yaml"; \
		exit 1; \
	fi
	cargo run -- --config $(CONFIG)

# Build and run (release mode)
run-release:
	cargo run --release

# === Static Build (default) ===

build:
	cargo build

build-release:
	cargo build --release

build-static:
	cargo build --release

# === Dynamic Build ===

# Build server + all plugins as shared libraries (debug)
build-dynamic: build-dynamic-server
	@echo ""
	@echo "=== Dynamic build complete (debug) ==="
	@echo "Server:  target/debug/drasi-server"
	@echo "Plugins: target/debug/libdrasi_*.so"
	@echo "Runtime: target/debug/libdrasi_plugin_runtime.so"

# Build server + all plugins as shared libraries (release)
build-dynamic-release: build-dynamic-server-release
	@echo ""
	@echo "=== Dynamic build complete (release) ==="
	@echo "Server:  target/release/drasi-server"
	@echo "Plugins: target/release/libdrasi_*.so"
	@echo "Runtime: target/release/libdrasi_plugin_runtime.so"

# Build server + all plugins in a SINGLE cargo invocation (debug).
# Using a single command ensures shared deps get unified feature resolution
# and identical symbol hashes across server and all plugin .so files.
build-dynamic-server:
	RUSTFLAGS='$(DYNAMIC_RUSTFLAGS)' \
		cargo build --no-default-features --features 'dynamic-plugins,all-plugin-deps'
	@echo "Copying Rust libstd to target/debug/..."
	@SYSROOT=$$(rustc --print sysroot) && HOST=$$(rustc -vV | grep host | cut -d' ' -f2) && \
		for f in $${SYSROOT}/lib/rustlib/$${HOST}/lib/libstd-*.so; do \
			[ -f "$$f" ] && cp "$$f" target/debug/ && echo "  $$(basename $$f)"; \
		done

build-dynamic-server-release:
	RUSTFLAGS='$(DYNAMIC_RUSTFLAGS)' \
		cargo build --no-default-features --features 'dynamic-plugins,all-plugin-deps' --release
	@echo "Copying Rust libstd to target/release/..."
	@SYSROOT=$$(rustc --print sysroot) && HOST=$$(rustc -vV | grep host | cut -d' ' -f2) && \
		for f in $${SYSROOT}/lib/rustlib/$${HOST}/lib/libstd-*.so; do \
			[ -f "$$f" ] && cp "$$f" target/release/ && echo "  $$(basename $$f)"; \
		done

# Build ONLY plugin shared libraries (without server binary).
# Still uses the same features so deps are compatible if server is built later.
build-dynamic-plugins:
	@echo "=== Building dynamic plugins (debug) ==="
	RUSTFLAGS='$(DYNAMIC_RUSTFLAGS)' \
		cargo build --no-default-features --features 'dynamic-plugins,all-plugin-deps' --lib

build-dynamic-plugins-release:
	@echo "=== Building dynamic plugins (release) ==="
	RUSTFLAGS='$(DYNAMIC_RUSTFLAGS)' \
		cargo build --no-default-features --features 'dynamic-plugins,all-plugin-deps' --lib --release

clippy:
	cargo clippy --all-targets --all-features

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

test:
	cargo test --all-features

# Test the static build: build with default features and run unit tests
test-static:
	@echo "=== Testing static build ==="
	cargo build
	cargo test --all-features
	@echo "=== Static build: OK ==="

# Test the dynamic build: build with dynamic-plugins feature and run unit tests
test-dynamic:
	@echo "=== Testing dynamic build ==="
	cargo test --no-default-features --features dynamic-plugins --lib
	@echo "=== Dynamic build: OK ==="

# Plugin smoke tests: start server and create every plugin kind, verify no crash
test-smoke:
	@echo "=== Plugin smoke test (static + dynamic) ==="
	./tests/plugin_smoke_test.sh

test-smoke-static:
	@echo "=== Plugin smoke test (static only) ==="
	./tests/plugin_smoke_test.sh --static

test-smoke-dynamic:
	@echo "=== Plugin smoke test (dynamic only) ==="
	./tests/plugin_smoke_test.sh --dynamic

vscode-test:
	cd dev-tools/vscode/drasi-server && npm test

# === Docker ===

# Docker build variables
IMAGE_PREFIX ?= ghcr.io/drasi-project
DOCKER_TAG_VERSION ?=
DOCKERX_OPTS ?=

# Build Docker image
docker-build:
	@if [ -z "$(DOCKER_TAG_VERSION)" ]; then \
		echo "Error: DOCKER_TAG_VERSION is required"; \
		echo "Usage: make docker-build DOCKER_TAG_VERSION=v1.0.0"; \
		exit 1; \
	fi
	docker buildx build . -t $(IMAGE_PREFIX)/drasi-server:$(DOCKER_TAG_VERSION) $(DOCKERX_OPTS)

# === Utilities ===

# Check system dependencies
doctor:
	@echo "Checking Drasi Server dependencies..."
	@echo ""
	@echo "Required:"
	@command -v cargo >/dev/null 2>&1 && echo "  [OK] Rust/Cargo $$(rustc --version | cut -d' ' -f2)" || echo "  [MISSING] Rust/Cargo - https://rustup.rs"
	@command -v git >/dev/null 2>&1 && echo "  [OK] Git" || echo "  [MISSING] Git"
	@if [ -d "$(DRASI_CORE_DIR)/lib" ]; then echo "  [OK] drasi-core found"; else echo "  [MISSING] drasi-core - expected at $(DRASI_CORE_DIR)"; fi
	@echo ""
	@echo "Optional (for examples):"
	@command -v docker >/dev/null 2>&1 && echo "  [OK] Docker" || echo "  [SKIP] Docker - https://docs.docker.com/get-docker/"
	@(command -v docker-compose >/dev/null 2>&1 || docker compose version >/dev/null 2>&1) && echo "  [OK] Docker Compose" || echo "  [SKIP] Docker Compose"
	@command -v curl >/dev/null 2>&1 && echo "  [OK] curl" || echo "  [SKIP] curl"
	@echo ""

# Validate configuration
validate:
	@if [ -z "$(CONFIG)" ]; then \
		echo "Validating config/server.yaml..."; \
		cargo run --release -- validate --config config/server.yaml 2>/dev/null || echo "Note: validate subcommand not yet implemented"; \
	else \
		echo "Validating $(CONFIG)..."; \
		cargo run --release -- validate --config $(CONFIG) 2>/dev/null || echo "Note: validate subcommand not yet implemented"; \
	fi

# Run the getting-started demo
demo:
	@echo "Starting Drasi Server Getting Started Demo..."
	@echo ""
	@if [ ! -d "examples/getting-started" ]; then \
		echo "Error: examples/getting-started directory not found"; \
		exit 1; \
	fi
	@cd examples/getting-started && ./scripts/setup-database.sh
	@echo ""
	@echo "Database ready. Starting server..."
	@sleep 2
	@cd examples/getting-started && ./scripts/start-server.sh

# Clean up demo resources
demo-cleanup:
	@if [ -d "examples/getting-started" ]; then \
		cd examples/getting-started && ./scripts/cleanup.sh --volumes 2>/dev/null || ./scripts/cleanup.sh; \
	fi

# Clean build artifacts
clean:
	cargo clean

# Initialize and update git submodules
submodule-update:
	@echo "Initializing and updating git submodules..."
	git submodule update --init --recursive
	@echo "Submodules updated successfully"
