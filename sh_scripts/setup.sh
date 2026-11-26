#!/usr/bin/env bash
# Installs what building, testing and deploying need: the wasm target and the stellar CLI.
#
# Usage: ./sh_scripts/setup.sh [--no-cli]

source "$(dirname "$0")/_lib.sh"

STELLAR_CLI_VERSION=30.0.1

need rustup cargo

info "Rust toolchain (rust-toolchain.toml)"
rustup show active-toolchain >/dev/null
rustup target add wasm32v1-none

if [[ "${1:-}" != --no-cli ]]; then
  if stellar --version 2>/dev/null | grep -q "$STELLAR_CLI_VERSION"; then
    info "stellar CLI $STELLAR_CLI_VERSION already installed"
  else
    info "stellar CLI $STELLAR_CLI_VERSION (on Linux it needs libdbus-1-dev and libudev-dev)"
    cargo install --locked stellar-cli --version "$STELLAR_CLI_VERSION"
  fi
fi

command -v jq >/dev/null || warn "jq is missing; the deploy scripts need it"
info "Ready. Try ./sh_scripts/test.sh"
