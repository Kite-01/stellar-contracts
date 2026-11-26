#!/usr/bin/env bash
# Runs the tests.
# Usage: ./sh_scripts/test.sh [unit|integration|all] [extra cargo test args]
#   unit         types, kite and factory, natively (default)
#   integration  builds both contracts, then runs tests/integration against them, real SACs and
#                Soroswap's testnet wasm
#   all          both

source "$(dirname "$0")/_lib.sh"
need cargo

suite="${1:-unit}"
[[ $# -gt 0 ]] && shift

unit() {
  info "Unit tests"
  cargo test -p types -p kite -p factory "$@"
}

integration() {
  "$SCRIPTS/build.sh"
  info "Integration tests"
  cargo test -p integration "$@"
}

case "$suite" in
unit) unit "$@" ;;
integration) integration "$@" ;;
all)
  unit "$@"
  integration "$@"
  ;;
*) die "unknown suite '$suite': use unit, integration or all" ;;
esac
