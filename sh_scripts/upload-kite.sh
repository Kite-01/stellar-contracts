#!/usr/bin/env bash
# Uploads the kite wasm and prints its hash, the `implementation` a factory and every transfer
# names. Uploading the same wasm again is harmless.
# Usage: STELLAR_ACCOUNT=<key> ./sh_scripts/upload-kite.sh

source "$(dirname "$0")/_lib.sh"
need stellar jq
need_testnet
need_signer

"$SCRIPTS/build.sh"
wasm="$(wasm_of kite)"

confirm "Upload $wasm ($(wasm_hash "$wasm")) to $NETWORK?"
hash="$(stellar contract upload --wasm "$wasm" --network "$NETWORK")"

record --arg hash "$hash" '.implementations = ((.implementations // []) + [$hash] | unique)'
info "kite uploaded: $hash"
echo "$hash"
