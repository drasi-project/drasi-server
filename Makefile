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

.PHONY: all build build-release build-cross build-cross-release \
        run run-release setup demo demo-cleanup \
        doctor validate clean clippy test test-all test-smoke \
        fmt fmt-check help docker-build \
        submodule-update vscode-test dev-build clean-dev-build \
        build-ui clean-ui build-local-test-plugins download-test-plugins \
        build-local-plugins build-local-plugins-debug test-tooling prepare-core

# Platform detection
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    PLUGIN_LIB_EXT := dylib
    PLUGIN_LIB_PREFIX := lib
else ifeq ($(OS),Windows_NT)
    PLUGIN_LIB_EXT := dll
    PLUGIN_LIB_PREFIX :=
else
    # Linux and other Unix
    PLUGIN_LIB_EXT := so
    PLUGIN_LIB_PREFIX := lib
endif

# Binary name
ifeq ($(OS),Windows_NT)
    SERVER_BIN := drasi-server.exe
else
    SERVER_BIN := drasi-server
endif

# Cross mounts path crates automatically, but their inherited workspace manifest
# also needs the complete sibling root. Preserve the logical path for symlinks.
CROSS_CORE_WORKSPACE = $(abspath ../drasi-core)

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
	@echo "Build:"
	@echo "  make prepare-core       - Obtain or verify the exact sibling engine revision"
	@echo "  make build              - Build debug binary and UI"
	@echo "  make build-release      - Build release binary and UI"
	@echo "  make build-ui           - Build only the web UI"
	@echo "  make build-cross TARGET=<triple>         - Cross-compile (debug)"
	@echo "  make build-cross-release TARGET=<triple> - Cross-compile (release)"
	@echo ""
	@echo "Development:"
	@echo "  make dev-build          - Format, lint, and test"
	@echo "  make clean-dev-build    - Clean, format, lint, and test"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run unit and integration tests (skips plugin-dependent tests)"
	@echo "  make test-all           - Build test plugins and run ALL tests (including ignored/E2E)"
	@echo "  make test-smoke         - Plugin smoke test"
	@echo "  make vscode-test        - Run VSCode extension tests"
	@echo ""
	@echo "Plugins (local builds require Cargo-resolved matching local SDKs):"
	@echo "  make build-local-plugins       - Build all plugins (release) from local drasi-core"
	@echo "  make build-local-plugins-debug - Build all plugins (debug) from local drasi-core"
	@echo "  make build-local-test-plugins   - Build test-only plugins (mock, log, scriptfile)"
	@echo "  make download-test-plugins      - Download test plugins from OCI registry (no drasi-core needed)"
	@echo "  make test-tooling               - Test plugin dependency-origin and setup policy"
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
	@echo "  make clean-ui           - Clean UI build artifacts"
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
run: build-ui
	cargo run

# Build and run with custom config
run-config: build-ui
	@if [ -z "$(CONFIG)" ]; then \
		echo "Usage: make run-config CONFIG=path/to/config.yaml"; \
		exit 1; \
	fi
	cargo run -- --config $(CONFIG)

# Build and run (release mode)
run-release: build-ui
	cargo run --release

# === Build ===

prepare-core:
	bash scripts/prepare-core.sh

# Build the web UI (requires Node.js/npm)
build-ui:
	@echo "Building web UI..."
	cd ui && npm ci && npm run build

build: build-ui
	cargo build --locked

build-release: build-ui
	cargo build --locked --release

build-cross:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-cross TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	@bash scripts/prepare-core.sh
	DRASI_CORE_WORKSPACE="$(CROSS_CORE_WORKSPACE)" cross build --locked --target-dir target/cross --target "$(TARGET)"

build-cross-release:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-cross-release TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	@bash scripts/prepare-core.sh
	DRASI_CORE_WORKSPACE="$(CROSS_CORE_WORKSPACE)" cross build --locked --target-dir target/cross --release --target "$(TARGET)"

clippy:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

test:
	cargo test

test-tooling:
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p 'test_*.py'

# Run ALL tests: build/download test plugins, run cargo tests (including #[ignore]),
# plugin smoke tests, and VSCode extension tests.
# Build local plugins only when Cargo resolves matching local SDK/host/FFI code.
# An engine-only path patch must retain the compatible registry SDK plugins.
test-all:
	@mode="$$(python3 scripts/plugin_origin.py mode)" && \
	case "$$mode" in \
		local) $(MAKE) build-local-plugins-debug ;; \
		registry) $(MAKE) download-test-plugins ;; \
		*) echo "Unsupported plugin dependency origin: $$mode" >&2; exit 1 ;; \
	esac
	@echo "=== Building server binary ==="
	cargo build --locked
	@echo "=== Running unit and integration tests (including ignored/E2E) ==="
	cargo test --locked --tests -- --include-ignored
	@echo "=== Running doctests ==="
	cargo test --locked --doc
	@echo "=== Running plugin smoke tests ==="
	./tests/plugin_smoke_test.sh --skip-build
	@echo "=== All tests passed ==="

# Plugin smoke tests: start server and create every plugin kind, verify no crash
test-smoke:
	@echo "=== Plugin smoke test ==="
	./tests/plugin_smoke_test.sh

# Build cdylib test plugins (mock source, log reaction, http reaction, scriptfile bootstrap)
# needed by solution deployment and E2E tests.
# Plugins are built from ../drasi-core and copied to target/debug/plugins/.
# Download pre-built test plugins from the OCI registry (no local drasi-core needed).
# Uses the server's built-in `plugin install` CLI to fetch mock source, log reaction,
# http reaction, and scriptfile bootstrap plugins.
download-test-plugins:
	@echo "=== Downloading test plugins from OCI registry ==="
	cargo build --locked
	python3 scripts/install_plugins.py --group test \
		--server-bin target/debug/$(SERVER_BIN) --plugins-dir target/debug/plugins
	@echo "=== Test plugins downloaded to target/debug/plugins/ ==="

build-local-test-plugins:
	@set -eu; \
	core_root="$$(python3 scripts/plugin_origin.py local-workspace)"; \
	plugins_dir="$(CURDIR)/target/debug/plugins"; \
	cd "$$core_root"; \
	cargo build --locked --lib \
		-p drasi-source-mock -p drasi-reaction-log -p drasi-reaction-http -p drasi-bootstrap-scriptfile \
		--features drasi-source-mock/dynamic-plugin,drasi-reaction-log/dynamic-plugin,drasi-reaction-http/dynamic-plugin,drasi-bootstrap-scriptfile/dynamic-plugin; \
	mkdir -p "$$plugins_dir"; \
	for plugin in drasi_source_mock drasi_reaction_log drasi_reaction_http drasi_bootstrap_scriptfile; do \
		cp "target/debug/$(PLUGIN_LIB_PREFIX)$$plugin.$(PLUGIN_LIB_EXT)" "$$plugins_dir/"; \
	done
	@echo "=== Test plugins ready in target/debug/plugins/ ==="

# Build ALL cdylib plugins from the workspace supplying the server's local SDKs.
# Engine/AST/parser path patches alone do not select that workspace's SDK or ABI.
build-local-plugins:
	@set -eu; \
	core_root="$$(python3 scripts/plugin_origin.py local-workspace)"; \
	$(MAKE) -C "$$core_root" build-plugins-release; \
	mkdir -p target/release/plugins; \
	cp "$$core_root"/target/release/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) target/release/plugins/
	@echo "=== Local plugins ready in target/release/plugins/ ==="

# Debug counterpart; the same Cargo-origin check rejects unused SDK siblings.
build-local-plugins-debug:
	@set -eu; \
	core_root="$$(python3 scripts/plugin_origin.py local-workspace)"; \
	$(MAKE) -C "$$core_root" build-plugins; \
	mkdir -p target/debug/plugins; \
	cp "$$core_root"/target/debug/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) target/debug/plugins/
	@echo "=== Local plugins ready in target/debug/plugins/ ==="

dev-run:
	cargo run -- --config config/server.yaml

dev-build: fmt clippy test
	@echo "Dev build complete!"

clean-dev-build: clean fmt clippy test
	@echo "Clean dev build complete!"

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
	@command -v node >/dev/null 2>&1 && echo "  [OK] Node.js $$(node --version)" || echo "  [MISSING] Node.js - https://nodejs.org (required to build the web UI)"
	@command -v npm >/dev/null 2>&1 && echo "  [OK] npm $$(npm --version)" || echo "  [MISSING] npm (required to build the web UI)"
	@if [ -d "drasi-core/lib" ]; then echo "  [OK] Submodules initialized"; else echo "  [MISSING] Submodules - run: git submodule update --init --recursive"; fi
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
clean: clean-ui
	cargo clean

# Clean UI build artifacts
clean-ui:
	rm -rf ui/dist ui/node_modules

# Initialize and update git submodules
submodule-update:
	@echo "Initializing and updating git submodules..."
	git submodule update --init --recursive
	@echo "Submodules updated successfully"
