#!/usr/bin/env bash
# Checks formatting and runs clippy, with warnings as errors.
# Usage: ./sh_scripts/lint.sh [--fix]
#
# tests/integration imports the built contracts, so clippy builds them first.

source "$(dirname "$0")/_lib.sh"
need cargo

if [[ "${1:-}" == --fix ]]; then
  info "cargo fmt"
  cargo fmt --all
else
  info "cargo fmt --check"
  cargo fmt --all -- --check
fi

[[ -f "$(wasm_of kite)" && -f "$(wasm_of factory)" ]] || "$SCRIPTS/build.sh"

info "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

if command -v shellcheck >/dev/null; then
  info "shellcheck"
  shellcheck -x "$SCRIPTS"/*.sh
fi
