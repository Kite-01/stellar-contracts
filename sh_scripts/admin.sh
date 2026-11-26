#!/usr/bin/env bash
# Admin calls on a deployed factory. STELLAR_ACCOUNT must be its admin.
# Usage: ./sh_scripts/admin.sh <command> [arg]
#   set-implementation <hash>   accept an uploaded kite and use it for new transfers
#   grant-operator <G…>
#   revoke-operator <G…>
#   set-admin <G…|C…>
#   upgrade <hash>              replace the factory's own code (upload it first)

source "$(dirname "$0")/_lib.sh"
need stellar jq
need_testnet

command="${1:-}"
arg="${2:-}"
[[ -n "$command" && -n "$arg" ]] || {
  usage
  exit 1
}

contract="$(factory)"

case "$command" in
set-implementation)
  confirm "Accept $arg as the kite implementation on $contract?"
  invoke "$contract" set_implementation --implementation "$arg"
  record --arg hash "$arg" '.implementation = $hash | .implementations = ((.implementations // []) + [$hash] | unique)'
  ;;
grant-operator)
  confirm "Grant $arg the operator role on $contract?"
  invoke "$contract" grant_operator --operator "$arg"
  record --arg op "$arg" '.operators = ((.operators // []) + [$op] | unique)'
  ;;
revoke-operator)
  confirm "Revoke $arg's operator role on $contract?"
  invoke "$contract" revoke_operator --operator "$arg"
  record --arg op "$arg" '.operators = ((.operators // []) - [$op])'
  ;;
set-admin)
  confirm "Make $arg the admin of $contract?"
  invoke "$contract" set_admin --admin "$arg"
  record --arg admin "$arg" '.admin = $admin'
  ;;
upgrade)
  confirm "Upgrade $contract to wasm $arg?"
  invoke "$contract" upgrade --wasm_hash "$arg"
  record --arg wasm "$arg" '.factory_wasm = $wasm'
  ;;
*)
  usage
  exit 1
  ;;
esac
