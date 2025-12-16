# Makefile for Drasi Server

# RUSTFLAGS for Clippy linting (matching ci-lint.yml workflow)
RUSTFLAGS := -Dwarnings \
	-W clippy::print_stdout \
	-W clippy::unwrap_used \
	-A unused \
	-A clippy::module_inception \
	-A clippy::ptr_arg \
	-A clippy::type_complexity

.PHONY: clippy test fmt help

# Default target
help:
	@echo "Available targets:"
	@echo "  clippy        - Run cargo clippy with same configuration as CI"
	@echo "  test          - Run cargo test with all features"
	@echo "  fmt           - Check code formatting"
	@echo "  help          - Show this help message"

clippy:
	RUSTFLAGS="$(RUSTFLAGS)" cargo clippy --all-targets --all-features

test:
	cargo test --all-features

fmt:
	cargo fmt -- --check