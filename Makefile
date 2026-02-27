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

.PHONY: all build build-release build-static build-cross build-cross-release \
        build-dynamic build-dynamic-release \
        build-dynamic-server build-dynamic-server-release build-dynamic-plugins build-dynamic-plugins-release \
        build-dynamic-cross build-dynamic-cross-release \
        run run-release setup demo demo-cleanup \
        doctor validate clean clippy test test-static test-dynamic test-smoke test-smoke-static test-smoke-dynamic \
        fmt fmt-check help docker-build \
        submodule-update vscode-test

# Platform detection
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    PLUGIN_LIB_EXT := dylib
    PLUGIN_LIB_PREFIX := lib
    # macOS uses @loader_path instead of $ORIGIN
    RPATH_FLAG := -C link-args=-Wl,-rpath,@loader_path
else ifeq ($(OS),Windows_NT)
    PLUGIN_LIB_EXT := dll
    PLUGIN_LIB_PREFIX :=
    # Windows doesn't need rpath
    RPATH_FLAG :=
else
    # Linux and other Unix
    PLUGIN_LIB_EXT := so
    PLUGIN_LIB_PREFIX := lib
    RPATH_FLAG := -C link-args=-Wl,-rpath,$$ORIGIN
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
# NOTE: Only applied to the server's cross build (not plugin builds, since plugins
# live in their own workspace which cross mounts automatically).
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
	@echo "Static Build (all plugins linked into binary):"
	@echo "  make build              - Build debug binary"
	@echo "  make build-release      - Build release binary"
	@echo "  make build-static       - Alias for build (debug)"
	@echo "  make build-cross TARGET=<triple>         - Cross-compile (debug)"
	@echo "  make build-cross-release TARGET=<triple> - Cross-compile (release)"
	@echo ""
	@echo "Dynamic Build (cdylib plugins, loaded at runtime):"
	@echo "  make build-dynamic              - Build server + plugins (debug)"
	@echo "  make build-dynamic-release      - Build server + plugins (release)"
	@echo "  make build-dynamic-server       - Build only the server (debug)"
	@echo "  make build-dynamic-server-release - Build only the server (release)"
	@echo "  make build-dynamic-plugins      - Build only plugins (debug)"
	@echo "  make build-dynamic-plugins-release - Build only plugins (release)"
	@echo ""
	@echo "Cross-Compilation (dynamic build for other targets):"
	@echo "  make build-dynamic-cross TARGET=x86_64-pc-windows-gnu"
	@echo "  make build-dynamic-cross-release TARGET=x86_64-pc-windows-gnu"
	@echo "  Supported targets: see Cross.toml"
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

# Remove plugin shared library files that are side effects of crate-type = ["lib", "dylib"].
# These are unused in static builds (the rlib is linked into the binary).
define clean_plugin_libs
	@rm -f $(1)/$(PLUGIN_LIB_PREFIX)drasi_source_*.$(PLUGIN_LIB_EXT) \
	       $(1)/$(PLUGIN_LIB_PREFIX)drasi_reaction_*.$(PLUGIN_LIB_EXT) \
	       $(1)/$(PLUGIN_LIB_PREFIX)drasi_bootstrap_*.$(PLUGIN_LIB_EXT) \
	       $(1)/$(PLUGIN_LIB_PREFIX)drasi_plugin_runtime.$(PLUGIN_LIB_EXT)
endef

build:
	cargo build
	$(call clean_plugin_libs,target/debug)

build-release:
	cargo build --release
	$(call clean_plugin_libs,target/release)

build-static:
	cargo build
	$(call clean_plugin_libs,target/debug)

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

# === Dynamic Build (cdylib plugins) ===
#
# Each plugin is a self-contained cdylib (.so/.dylib/.dll) with its own tokio
# runtime, communicating with the host via #[repr(C)] vtable structs.
# No special RUSTFLAGS, RTLD_GLOBAL, or libstd copying needed.
#   - Don't need prefer-dynamic or RTLD_GLOBAL
#   - Don't need libstd copying
#   - Don't need single-invocation builds (no symbol hash coupling)
#   - Are fully self-contained .so/.dylib/.dll files
#
# Plugin crates are resolved via Cargo.toml dependencies — from crates.io in
# production, or from local paths when [patch.crates-io] is configured in
# .cargo/config.toml for development.

# Build server with cdylib-plugins feature + build all plugins as cdylib (debug)
build-dynamic: build-dynamic-server build-dynamic-plugins
	@echo ""
	@echo "=== cdylib build complete (debug) ==="
	@echo "Server:  target/debug/$(SERVER_BIN)"
	@echo "Plugins: target/debug/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT)"

# Build server with cdylib-plugins feature + build all plugins as cdylib (release)
build-dynamic-release: build-dynamic-server-release build-dynamic-plugins-release
	@echo ""
	@echo "=== cdylib build complete (release) ==="
	@echo "Server:  target/release/$(SERVER_BIN)"
	@echo "Plugins: target/release/$(PLUGIN_LIB_PREFIX)drasi_*.$(PLUGIN_LIB_EXT)"

# Build server without builtin plugins, with cdylib loading support (debug)
build-dynamic-server:
	@echo "=== Building cdylib server (debug) ==="
	cargo build --no-default-features --features dynamic-plugins

# Build server without builtin plugins, with cdylib loading support (release)
build-dynamic-server-release:
	@echo "=== Building cdylib server (release) ==="
	cargo build --no-default-features --features dynamic-plugins --release

# Build all plugins as cdylib shared libraries (debug).
# Uses cargo xtask to discover plugins via cargo metadata and build each one
# with the dynamic-plugin feature enabled.
build-dynamic-plugins:
	cargo xtask build-plugins

# Build all plugins as cdylib shared libraries (release)
build-dynamic-plugins-release:
	cargo xtask build-plugins --release

# === Cross-Compilation (dynamic build) ===
#
# Usage: make build-dynamic-cross TARGET=x86_64-pc-windows-gnu
# Uses `cross` for the server and `cargo xtask` with --target for plugins.

build-dynamic-cross:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-dynamic-cross TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	@echo "=== Cross-compiling dynamic build for $(TARGET) (debug) ==="
	CROSS_CONTAINER_OPTS="$(CROSS_PATCH_VOLUMES)" cross build --target-dir target/cross --no-default-features --features dynamic-plugins --target $(TARGET)
	cargo xtask build-plugins --target $(TARGET)

build-dynamic-cross-release:
	@if [ -z "$(TARGET)" ]; then \
		echo "Error: TARGET is required"; \
		echo "Usage: make build-dynamic-cross-release TARGET=x86_64-pc-windows-gnu"; \
		exit 1; \
	fi
	@echo "=== Cross-compiling dynamic build for $(TARGET) (release) ==="
	CROSS_CONTAINER_OPTS="$(CROSS_PATCH_VOLUMES)" cross build --target-dir target/cross --no-default-features --features dynamic-plugins --target $(TARGET) --release
	cargo xtask build-plugins --release --target $(TARGET)

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
