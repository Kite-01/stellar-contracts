#!/usr/bin/env bash
# Shows a factory's live state on chain. Read-only: every call is simulated.
# Usage: KITE_FACTORY=<C…> ./sh_scripts/status.sh [operator to check…]

source "$(dirname "$0")/_lib.sh"
need stellar

contract="$(factory)"
implementation="$(view "$contract" implementation | tr -d '"')"

echo "factory $contract on $NETWORK"
printf '  %-16s %s\n' admin "$(view "$contract" admin | tr -d '"')"
printf '  %-16s %s\n' implementation "$implementation"

local_wasm="$(wasm_of kite)"
if [[ -f "$local_wasm" ]]; then
  local_hash="$(wasm_hash "$local_wasm")"
  if [[ "$local_hash" == "$implementation" ]]; then
    echo "  local build      matches the implementation"
  else
    echo "  local build      $local_hash (accepted: $(view "$contract" is_implementation --implementation "$local_hash"))"
  fi
fi

for operator in "$@"; do
  printf '  %-16s %s\n' "operator?" "$operator: $(view "$contract" is_operator --operator "$operator")"
done
