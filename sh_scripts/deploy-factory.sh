#!/usr/bin/env bash
# Deploys the factory with its admin, first operator and kite implementation.
# Usage: STELLAR_ACCOUNT=<key> ADMIN=<G…> OPERATOR=<G…> IMPLEMENTATION=<hash> ./sh_scripts/deploy-factory.sh
#
# OPERATOR is the Kite server's Stellar operator: KITE_OPERATOR_MNEMONIC on m/44'/148'/0'.
# IMPLEMENTATION is the hash ./sh_scripts/upload-kite.sh prints.

source "$(dirname "$0")/_lib.sh"
need stellar jq
need_testnet
need_signer

admin="${ADMIN:?ADMIN is required}"
operator="${OPERATOR:-}"
implementation="${IMPLEMENTATION:?IMPLEMENTATION is required (./sh_scripts/upload-kite.sh prints it)}"

"$SCRIPTS/build.sh"

args=(--admin "$admin" --implementation "$implementation")
# an Option is passed as JSON
[[ -n "$operator" ]] && args+=(--operator "\"$operator\"")

confirm "Deploy the factory on $NETWORK with admin $admin, operator ${operator:-none} and implementation $implementation?"
factory_id="$(stellar contract deploy --wasm "$(wasm_of factory)" --network "$NETWORK" -- "${args[@]}")"

record --arg factory "$factory_id" --arg admin "$admin" --arg operator "$operator" \
  --arg implementation "$implementation" --arg wasm "$(wasm_hash "$(wasm_of factory)")" \
  '.factory = $factory | .admin = $admin | .operators = ((.operators // []) + [$operator] | map(select(. != "")) | unique)
   | .implementation = $implementation | .implementations = ((.implementations // []) + [$implementation] | unique)
   | .factory_wasm = $wasm'

info "Factory deployed: $factory_id (recorded in ${DEPLOYMENTS#"$ROOT/"})"
echo "$factory_id"
