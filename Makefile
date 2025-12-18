# Makefile for Drasi Server

# RUSTFLAGS for Clippy linting (matching ci-lint.yml workflow)
RUSTFLAGS := -Dwarnings \
	-W clippy::print_stdout \
	-W clippy::unwrap_used \
	-A unused \
	-A clippy::module_inception \
	-A clippy::ptr_arg \
	-A clippy::type_complexity

.PHONY: clippy test fmt fmt-check help

# Default target
help:
	@echo "Available targets:"
	@echo "  clippy        - Run cargo clippy with same configuration as CI"
	@echo "  test          - Run cargo test with all features"
	@echo "  fmt           - Format code using cargo fmt"
	@echo "  fmt-check     - Check code formatting (CI, does not modify files)"
	@echo "  help          - Show this help message"

# Note: Warnings are configured via RUSTFLAGS above, not via inline clippy flags.
clippy:
	RUSTFLAGS="$(RUSTFLAGS)" cargo clippy --all-targets --all-features

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

test:
	cargo test --all-features