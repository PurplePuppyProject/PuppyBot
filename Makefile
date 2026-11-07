#############################################
# Makefile for PuppyBot
#
# Targets:
#   all           : Run tests and build the release binary (default target).
#   build         : Build the Cargo project in debug mode.
#   release       : Build the Cargo project in release mode and list the resulting binaries.
#   lint          : Check formatting (cargo fmt), run the linter (cargo clippy), check the docs (cargo doc) and that puppybot-core stays free of I/O.
#   lint_fix      : Automatically apply suggestions of the compiler and Clippy, and format the code.
#   check         : Everything a pull request must pass: lint, coverage, deny and, if installed, zizmor.
#   test          : Run the Cargo tests.
#   coverage      : Measure the test coverage and fail below COVERAGE_MIN percent of lines or functions (needs cargo-llvm-cov).
#   deny          : Check the dependencies' advisories, licenses and sources (needs cargo-deny).
#   zizmor        : Audit the GitHub Actions workflows for security problems (needs zizmor).
#   fuzz          : Run each fuzz target for FUZZ_SECONDS seconds (needs nightly Rust and cargo-fuzz).
#   clean         : Clean build artifacts using Cargo's built-in clean command.
#   version       : Prepare a release: set VERSION in Cargo.toml, both lock files and CHANGELOG.md.
#   run           : Run the bot. Pass args via the ARGS variable.
#
# Usage Examples:
#   make check
#       Runs all checks of a pull request.
#
#   make lint_fix
#       Automatically fixes lint issues.
#
#   make fuzz FUZZ_SECONDS=600
#       Runs each fuzz target for ten minutes.
#
#   make run ARGS="--version"
#       Runs the bot with additional arguments.
#############################################

.PHONY: all build release lint lint_fix check test coverage deny zizmor fuzz clean version run

# Seconds to run each fuzz target for, see the target fuzz.
FUZZ_SECONDS ?= 60

# The share of lines, and of functions, that the tests must cover, in percent, see the target
# coverage.
COVERAGE_MIN ?= 100

# Crates that puppybot-core must not depend on, not even indirectly: Discord, storage, async I/O
# and the network belong to the program, see crates/puppybot-core/src/lib.rs.
CORE_FORBIDDEN = twilight|redb|tokio|mio|hyper|reqwest|rustls|socket2

# Default target: test, then build for release.
all: test release

# ------------------------------
# Build targets.
# ------------------------------
# Build the Cargo project (debug mode).
# * (This target is called by CI as well.)
build:
	@echo "Building Cargo project (debug mode)..."
	@cargo build $(ARGS)

# Build the Cargo project in release mode.
release:
	@echo "Building Cargo project (release mode)..."
	@cargo build --release $(ARGS)
	@ls -lh target/release

# ------------------------------
# Lint targets.
# ------------------------------
# Check the formatting, run Clippy with all warnings as errors, and build the docs, whose
# warnings, e.g. broken links, are errors too.
# * (This target is called by CI as well.)
lint:
	@echo "Checking Rust formatting (cargo fmt)..."
	@cargo fmt --check
	@echo "Running Rust linter (cargo clippy)..."
	@cargo clippy --all-targets --all-features $(ARGS) -- -D warnings
	@echo "Checking the documentation (cargo doc)..."
	@RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features $(ARGS)
	@echo "Checking that puppybot-core stays free of I/O (cargo tree)..."
	@if cargo tree --package puppybot-core --edges normal --prefix none $(ARGS) | grep -E '^($(CORE_FORBIDDEN))[a-z0-9_-]* v'; then \
		echo "puppybot-core must not depend on these crates, see crates/puppybot-core/src/lib.rs."; \
		exit 1; \
	fi

# Automatically fix lint issues.
lint_fix:
	@echo "Running cargo clippy --fix to automatically apply suggestions..."
	@cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
	@echo "Running cargo fmt to format Rust files..."
	@cargo fmt

# Everything that CI checks for a pull request, except the fuzz targets and the release build.
# zizmor runs offline here, without asking GitHub; CI's run asks it.
check: lint coverage deny
	@if command -v zizmor >/dev/null 2>&1; then \
		$(MAKE) --no-print-directory zizmor ZIZMOR_ARGS=--offline; \
	else \
		echo "zizmor is not installed: skipping the audit of the workflows, which CI runs."; \
	fi

# ------------------------------
# Test targets.
# ------------------------------
# Run the Cargo tests.
# * (This target is called by CI as well.)
test:
	@echo "Running Cargo tests..."
	@cargo test $(ARGS)

# Measure the coverage of the Cargo tests, including the program they start, and fail if they
# cover less than COVERAGE_MIN percent of the lines or of the functions. Prints a summary per
# file and the lines that no test runs, and writes an HTML report with the covered lines to
# target/llvm-cov/html and an LCOV report to target/llvm-cov/lcov.info.
# Install the tools with: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov
# * (This target is called by CI as well.)
coverage:
	@echo "Measuring test coverage..."
	@# Data of earlier runs, e.g. of files that were moved since, would count too.
	@cargo llvm-cov clean --workspace
	@cargo llvm-cov --workspace --all-features --no-report $(ARGS)
	@cargo llvm-cov report --html
	@cargo llvm-cov report --lcov --output-path target/llvm-cov/lcov.info
	@cargo llvm-cov report --show-missing-lines \
		--fail-under-lines $(COVERAGE_MIN) --fail-under-functions $(COVERAGE_MIN)

# Check the dependencies for known vulnerabilities, unmaintained crates, licenses that don't go
# with the GPL and sources other than crates.io, as configured in deny.toml.
# Install the tool with: cargo install cargo-deny
# * (This target is called by CI as well.)
deny:
	@echo "Checking dependencies (cargo deny)..."
	@cargo deny --locked check

# Audit the GitHub Actions workflows, e.g. for template injection or more permissions than they
# need. With a token in GH_TOKEN, also with what GitHub knows, e.g. actions with known
# vulnerabilities or pins to commits that aren't in the action's repository. Options for zizmor
# go in ZIZMOR_ARGS, as ARGS holds those for cargo, e.g. in make check ARGS=--locked.
# Install the tool with: pipx install zizmor
# * (This target is called by CI as well.)
zizmor:
	@echo "Auditing the workflows (zizmor)..."
	@zizmor $(ZIZMOR_ARGS) .github/

# Run each fuzz target in fuzz/ for FUZZ_SECONDS seconds, e.g. make fuzz FUZZ_SECONDS=600.
# Stops at the first failure; fuzz/README.md describes how to reproduce it.
# Install the tools with: rustup toolchain install nightly && cargo install cargo-fuzz
# * (This target is called by CI as well.)
fuzz:
	@cd fuzz && for target in $$(cargo +nightly fuzz list); do \
		echo "Fuzzing $$target for $(FUZZ_SECONDS) seconds..."; \
		cargo +nightly fuzz run $$target -- -max_total_time=$(FUZZ_SECONDS) -dict=puppybot.dict || exit 1; \
	done

# Prepare a release, e.g. make version VERSION=0.1.0, see Releasing in the README.
version:
	@./utils/set-version.sh "$(VERSION)"

# ------------------------------
# Clean target.
# ------------------------------
# Clean build artifacts using Cargo's built-in clean command.
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean

# ------------------------------
# Run targets.
# ------------------------------
# Run the bot. Pass extra arguments via ARGS.
run:
	@cargo run --bin puppybot -- $(ARGS)
