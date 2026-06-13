# MatForge IDE — task runner. Run `just` (or `just help`) to list recipes.
#
# Override the compiler location for any recipe, e.g.:
#   just MATLABC=/path/to/matlabc run

# Path to the matlab_llvm compiler (exported to recipes as $MATLABC_PATH).
MATLABC := env_var_or_default("MATLABC_PATH", "/home/leonardo/work/matlab_llvm/build/matlabc")
# Demo project used by `just demo` / `just shot`.
DEMO := "/tmp/mf_demo"

export MATLABC_PATH := MATLABC

# List all recipes (default).
default:
    @just --list

alias help := default

# ---- Build -----------------------------------------------------------------

# Build the whole workspace (debug).
build:
    cargo build

# Build an optimized release binary.
release:
    cargo build --release

# Build only the GTK-free core crate.
build-core:
    cargo build -p matforge-core

# ---- Run -------------------------------------------------------------------

# Launch the IDE.
run:
    cargo run -p matforge

# Launch the IDE with a folder/file opened (defaults to the demo project).
open folder=DEMO file="":
    MATFORGE_OPEN="{{folder}}" MATFORGE_FILE="{{file}}" cargo run -p matforge

# ---- Test ------------------------------------------------------------------

# Run all unit tests (no display required).
test:
    cargo test

# Run unit tests for the core crate only.
test-core:
    cargo test -p matforge-core --lib

# Run integration tests against the real matlabc (skips if absent).
test-integration:
    cargo test -p matforge-core --test integration

# Unit + integration tests.
test-all: test test-integration

# End-to-end tests: drive the real GTK binary via X11 input, assert on app
# state. Needs a display + python-xlib (`just e2e-setup`); use xvfb-run in CI.
e2e: build
    python3 e2e/run_e2e.py

# Install the e2e harness dependency (no sudo).
e2e-setup:
    pip install --user -r e2e/requirements.txt

# ---- Coverage --------------------------------------------------------------

# Print a coverage summary for the core crate (needs cargo-llvm-cov).
coverage:
    cargo llvm-cov --package matforge-core --summary-only

# Generate an HTML coverage report and print its path.
coverage-html:
    cargo llvm-cov --package matforge-core --html
    @echo "open target/llvm-cov/html/index.html"

# Fail if core coverage drops below 90% (region).
coverage-check:
    cargo llvm-cov --package matforge-core --fail-under-lines 90

# Install the coverage tooling.
coverage-setup:
    rustup component add llvm-tools-preview
    cargo install cargo-llvm-cov

# ---- Quality ---------------------------------------------------------------

# Format the whole workspace.
fmt:
    cargo fmt

# Check formatting without writing.
fmt-check:
    cargo fmt --check

# Lint with clippy, warnings as errors (needs `rustup component add clippy`).
lint:
    cargo clippy --all-targets -- -D warnings

# fmt-check + lint + tests — the pre-commit gate.
check: fmt-check lint test

# The full CI gate locally: pre-commit checks + integration tests.
ci: check test-integration

# ---- Demo / screenshots ----------------------------------------------------

# Create a sample .m project under {{DEMO}}.
seed-demo:
    mkdir -p {{DEMO}}
    printf 'function y = hello(x)\n%% compute a simple ramp\ny = zeros(1, x);\nfor i = 1:x\n    y(i) = i * 2;\nend\ndisp(y)\nend\n' > {{DEMO}}/hello.m
    @echo "wrote {{DEMO}}/hello.m"

# Launch the IDE with the demo project + file opened and auto-compiled.
demo: seed-demo build
    MATFORGE_OPEN="{{DEMO}}" MATFORGE_FILE="{{DEMO}}/hello.m" MATFORGE_COMPILE=1 \
        cargo run -p matforge

# Capture a screenshot of the demo window to {{out}} (needs gnome-screenshot).
shot out="/tmp/matforge.png": seed-demo build
    #!/usr/bin/env bash
    set -euo pipefail
    MATFORGE_OPEN="{{DEMO}}" MATFORGE_FILE="{{DEMO}}/hello.m" MATFORGE_COMPILE=1 \
        ./target/debug/matforge >/tmp/matforge_shot.log 2>&1 &
    pid=$!
    sleep 4
    gnome-screenshot -w -f "{{out}}" || import -window root "{{out}}"
    kill $pid 2>/dev/null || true
    echo "saved {{out}}"

# ---- Packaging -------------------------------------------------------------

PREFIX := env_var_or_default("PREFIX", env_var("HOME") + "/.local")

# Build release + install the binary, desktop entry, and icon under $PREFIX
# (default ~/.local — no root needed). Adds a per-user desktop launcher.
install:
    cargo build --release -p matforge
    install -Dm755 target/release/matforge "{{PREFIX}}/bin/matforge"
    install -Dm644 crates/app/resources/matforge.desktop "{{PREFIX}}/share/applications/matforge.desktop"
    install -Dm644 crates/app/resources/matforge.svg "{{PREFIX}}/share/icons/hicolor/scalable/apps/matforge.svg"
    @echo "installed to {{PREFIX}} — ensure {{PREFIX}}/bin is on PATH"

# Remove a `just install`.
uninstall:
    rm -f "{{PREFIX}}/bin/matforge" \
          "{{PREFIX}}/share/applications/matforge.desktop" \
          "{{PREFIX}}/share/icons/hicolor/scalable/apps/matforge.svg"
    @echo "removed from {{PREFIX}}"

# Build a release tarball (binary + desktop + icon + README) under dist/.
dist:
    cargo build --release -p matforge
    rm -rf dist/matforge && mkdir -p dist/matforge
    cp target/release/matforge dist/matforge/
    cp crates/app/resources/matforge.desktop crates/app/resources/matforge.svg dist/matforge/
    cp README.md dist/matforge/ 2>/dev/null || true
    tar -C dist -czf dist/matforge-linux-x86_64.tar.gz matforge
    @echo "wrote dist/matforge-linux-x86_64.tar.gz"

# Build a .deb (needs `cargo install cargo-deb`).
deb:
    cargo deb -p matforge

# ---- Housekeeping ----------------------------------------------------------

# Remove build artifacts.
clean:
    cargo clean

# Print the resolved compiler path and whether it exists.
doctor:
    @echo "MATLABC_PATH = {{MATLABC}}"
    @test -x "{{MATLABC}}" && echo "  -> found" || echo "  -> MISSING (set MATLABC_PATH)"
    @echo "gtk4: $(pkg-config --modversion gtk4 2>/dev/null || echo 'not found')"
    @echo "rustc: $(rustc --version)"
