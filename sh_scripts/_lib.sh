# shellcheck shell=bash

set -euo pipefail

SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$SCRIPTS")"
cd "$ROOT"

# shellcheck disable=SC2034
CONTRACTS=(kite factory)

NETWORK="${STELLAR_NETWORK:-testnet}"
DEPLOYMENTS="$ROOT/deployments/stellar-$NETWORK.json"

# The stellar CLI signs as STELLAR_ACCOUNT: a key name from its keychain (`stellar keys`), or a
# secret key. STELLAR_SECRET_KEY takes precedence, so it never has to appear on a command line.
if [[ -n "${STELLAR_SECRET_KEY:-}" ]]; then
  export STELLAR_ACCOUNT="$STELLAR_SECRET_KEY"
fi

info() { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() {
  printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
  exit 1
}

need() {
  local cmd
  for cmd in "$@"; do
    command -v "$cmd" >/dev/null 2>&1 || die "$cmd is not installed (see ./sh_scripts/setup.sh)"
  done
}

need_testnet() {
  [[ "$NETWORK" == testnet ]] || die "only testnet is supported for now (STELLAR_NETWORK=$NETWORK)"
}

need_signer() {
  [[ -n "${STELLAR_ACCOUNT:-}" ]] || die "set STELLAR_ACCOUNT to a stellar CLI key name, or STELLAR_SECRET_KEY"
}

wasm_of() { echo "target/wasm32v1-none/release/$1.wasm"; }

wasm_hash() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

factory() {
  local id="${KITE_FACTORY:-}"
  if [[ -z "$id" && -f "$DEPLOYMENTS" ]]; then
    id="$(jq -r '.factory // empty' "$DEPLOYMENTS")"
  fi
  [[ -n "$id" ]] || die "KITE_FACTORY is required"
  echo "$id"
}

# invoke <contract> <function> [args…]
invoke() {
  need stellar
  need_signer
  stellar contract invoke --id "$1" --network "$NETWORK" --send yes -- "${@:2}"
}

# view <contract> <function> [args…]; simulated, never sent
view() {
  need stellar
  stellar contract invoke --id "$1" --network "$NETWORK" \
    --source-account "${STELLAR_ACCOUNT:-$1}" --send no -- "${@:2}" 2>/dev/null
}

record() {
  need jq
  mkdir -p "$(dirname "$DEPLOYMENTS")"
  [[ -f "$DEPLOYMENTS" ]] || echo '{}' >"$DEPLOYMENTS"
  local next
  next="$(jq "$@" "$DEPLOYMENTS")"
  echo "$next" >"$DEPLOYMENTS"
}

usage() {
  awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$SCRIPTS/$(basename "$0")"
}

confirm() {
  [[ "${YES:-}" == 1 ]] && return
  read -r -p "$* [y/N] " answer
  [[ "$answer" == y || "$answer" == Y ]] || die "cancelled"
}
