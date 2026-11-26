# AGENTS.md

Notes for AI agents working in this repository, or reviewing it. Start with the [README](README.md): it explains how the contracts work, the Stellar specifics, and every script.

## Commands

```sh
./sh_scripts/setup.sh
./sh_scripts/lint.sh
./sh_scripts/build.sh

# unit tests (types, kite, factory)
./sh_scripts/test.sh

# builds both contracts, then runs tests/integration
./sh_scripts/test.sh integration

# a deployment as the chain sees it (read-only)
KITE_FACTORY=<C…> ./sh_scripts/status.sh
```

- Run one test with `./sh_scripts/test.sh unit <name>` or `./sh_scripts/test.sh integration <name>`.
- soroban-sdk 29 refuses to build a contract Wasm outside `stellar contract build`. If the stellar CLI can't be installed, `SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo build --target wasm32v1-none --release -p kite -p factory` builds Wasm good enough for the tests, but never deploy it.
- `tests/integration` and clippy on it need `target/wasm32v1-none/release/{kite,factory}.wasm`; the scripts build them first.

## Layout

- `contracts/types/` (crate `types`, used as `kite_types`): `Transfer`, `Fee`, `Swap`, `SwapRoute`, `Error`, `validate`, the deploy salt, and the `kite` client the factory calls. `vector::fixed_vector` and `vector::fixed_vector_with_swap` pin transfers to their XDR, salt and address.
- `contracts/kite/`: the per-transfer contract: settle (with the Soroswap swap), refund, recover.
- `contracts/factory/`: admin, operators, accepted implementations, `predict_address`, and the operator calls (`settle`, `refund`, `recover`), each deploying the deposit first if needed.
- `tests/integration/`: the built contracts against real SACs and Soroswap's testnet Wasm (`tests/integration/wasm/`, fetched from testnet by hash).
- `sh_scripts/`: setup, build, test, lint, and uploading, deploying and administering a deployment.

## Rules

- **Addresses**: changing `Transfer`, `Fee`, `Swap`, `SwapRoute`, a field's name or type, or how the salt is computed changes every deposit address, and breaks every open transfer. Update the fixed vectors, and tell the user that the server's `src/shared/__tests__/kiteStellarContract.test.ts` pins the same vectors and must change in the same release.
- **Events, methods and errors** are the server's interface (`src/shared/kiteStellarContract.ts` in Kite-01/server). Keep their names, fields and codes, or say what breaks.
- **Secrets**: never commit keys or seed phrases. The scripts sign through the stellar CLI's keychain (`STELLAR_ACCOUNT`), or `STELLAR_SECRET_KEY` from the environment.
- **Don't send transactions** (`upload-kite.sh`, `deploy-factory.sh`, `admin.sh`) unless the user asks. `status.sh` only reads. Testnet only.
- **Style**: `cargo fmt`, clippy clean, and comments as sparse as in the code around them.
