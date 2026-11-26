#!/usr/bin/env bash
# Builds kite and factory to target/wasm32v1-none/release/<name>.wasm and prints their hashes.
#
# Usage: ./sh_scripts/build.sh

source "$(dirname "$0")/_lib.sh"
need stellar

for name in "${CONTRACTS[@]}"; do
  info "Building $name"
  stellar contract build --package "$name"
done

printf '\n%-10s %8s  %s\n' contract size "wasm hash"

for name in "${CONTRACTS[@]}"; do
  wasm="$(wasm_of "$name")"
  printf '%-10s %7sK  %s\n' "$name" "$(($(wc -c <"$wasm") / 1024))" "$(wasm_hash "$wasm")"
done
