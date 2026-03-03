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
        submodule-update vscode-test

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
	@echo "  make build              - Build debug binary"
	@echo "  make build-release      - Build release binary"
	@echo "  make build-cross TARGET=<triple>         - Cross-compile (debug)"
	@echo "  make build-cross-release TARGET=<triple> - Cross-compile (release)"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run all tests"
	@echo "  make test-smoke         - Plugin smoke test"
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

# === Build ===

build:
	cargo build

build-release:
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
