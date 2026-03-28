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
        doctor validate clean clippy test test-smoke \
        fmt fmt-check help docker-build \
        submodule-update vscode-test dev-build clean-dev-build \
        build-ui clean-ui build-local-test-plugins \
        build-local-plugins build-local-plugins-debug

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

# Auto-discover volume mounts for cross-compilation from local [patch.crates-io] paths.
# When developing with local path overrides in .cargo/config.toml, cross needs those
# directories mounted into its Docker container. If no local patches exist (crates
# come from crates.io), this produces an empty value and cross works normally.
CROSS_PATCH_VOLUMES := $(shell \
  grep -oP 'path\s*=\s*"\K[^"]+' .cargo/config.toml 2>/dev/null | \
  while read p; do \
    d="$$p"; \
    while [ "$$d" != "/" ]; do \
      if [ -f "$$d/Cargo.toml" ] && grep -q '^\[workspace\]' "$$d/Cargo.toml" 2>/dev/null; then \
        echo "$$d"; break; \
      fi; \
      d=$$(dirname "$$d"); \
    done; \
  done | sort -u | while read r; do printf -- '-v %s:%s ' "$$r" "$$r"; done)

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
	@echo "  make test               - Run all tests"
	@echo "  make test-smoke         - Plugin smoke test"
	@echo "  make vscode-test        - Run VSCode extension tests"
	@echo ""
	@echo "Plugins (local development with ../drasi-core):"
	@echo "  make build-local-plugins       - Build all plugins (release) from local drasi-core"
	@echo "  make build-local-plugins-debug - Build all plugins (debug) from local drasi-core"
	@echo "  make build-local-test-plugins   - Build test-only plugins (mock, log, scriptfile)"
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

# Build the web UI (requires Node.js/npm)
build-ui:
	@if command -v npm >/dev/null 2>&1; then \
		echo "Building web UI..."; \
		cd ui && npm install --prefer-offline && npm run build; \
		echo "UI built successfully at ui/dist/"; \
	else \
		echo "Warning: npm not found, skipping UI build. Install Node.js to build the UI."; \
	fi

build: build-ui
	cargo build

build-release: build-ui
	cargo build --release

build-cross:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-cross TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	CROSS_CONTAINER_OPTS="$(CROSS_PATCH_VOLUMES)" cross build --target-dir target/cross --target $(TARGET)

build-cross-release:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-cross-release TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	CROSS_CONTAINER_OPTS="$(CROSS_PATCH_VOLUMES)" cross build --target-dir target/cross --release --target $(TARGET)

clippy:
	cargo clippy --all-targets

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

test:
	cargo test

# Plugin smoke tests: start server and create every plugin kind, verify no crash
test-smoke:
	@echo "=== Plugin smoke test ==="
	./tests/plugin_smoke_test.sh

# Build cdylib test plugins (mock source, log reaction, scriptfile bootstrap)
# needed by solution deployment tests.
# Plugins are built from ../drasi-core and copied to target/debug/plugins/.
build-local-test-plugins:
	@echo "=== Building cdylib test plugins from drasi-core ==="
	cd ../drasi-core && cargo build --lib -p drasi-source-mock --features drasi-source-mock/dynamic-plugin
	cd ../drasi-core && cargo build --lib -p drasi-reaction-log --features drasi-reaction-log/dynamic-plugin
	cd ../drasi-core && cargo build --lib -p drasi-bootstrap-scriptfile --features drasi-bootstrap-scriptfile/dynamic-plugin
	@mkdir -p target/debug/plugins
	@echo "Copying test plugins to target/debug/plugins/..."
	@cp ../drasi-core/target/debug/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) target/debug/plugins/ 2>/dev/null && \
		echo "Test plugins copied successfully:" && \
		ls -1 target/debug/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) || \
		echo "Warning: No test plugin files found to copy"
	@echo "=== Test plugins ready in target/debug/plugins/ ==="

# Build ALL cdylib plugins from local ../drasi-core (release mode) and copy to target/release/plugins/.
# Use this when developing with [patch.crates-io] pointing to local drasi-core, so plugins match
# the server binary. Registry-downloaded plugins will NOT be ABI-compatible with local changes.
build-local-plugins:
	@echo "=== Building all cdylib plugins from local drasi-core (release) ==="
	cd ../drasi-core && make build-plugins-release
	@mkdir -p target/release/plugins
	@echo "Copying plugins to target/release/plugins/..."
	@cp ../drasi-core/target/release/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) target/release/plugins/ 2>/dev/null && \
		echo "Plugins copied successfully:" && \
		ls -1 target/release/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) || \
		echo "Warning: No plugin files found to copy"
	@echo "=== Local plugins ready in target/release/plugins/ ==="

# Build ALL cdylib plugins from local ../drasi-core (debug mode) and copy to target/debug/plugins/.
build-local-plugins-debug:
	@echo "=== Building all cdylib plugins from local drasi-core (debug) ==="
	cd ../drasi-core && make build-plugins
	@mkdir -p target/debug/plugins
	@echo "Copying plugins to target/debug/plugins/..."
	@cp ../drasi-core/target/debug/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) target/debug/plugins/ 2>/dev/null && \
		echo "Plugins copied successfully:" && \
		ls -1 target/debug/plugins/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT) || \
		echo "Warning: No plugin files found to copy"
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
