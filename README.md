# Kite Stellar Contracts

The Stellar (Soroban) smart contracts behind [Kite](https://github.com/Kite-01/server), an open-source crypto payment processor. Each payment gets its own contract, at an address known before anyone pays. The contract receives the customer's tokens, swaps them through Soroswap if needed, pays fees and settles to the merchant, or refunds the customer.

Written in Rust with soroban-sdk 29.0.1.

## How it works

A Kite server creates a transfer: who gets paid, in which token, the minimum to receive, fees, a refund address, an expiry, and the swap route if the customer pays in another token.

1. **Predict**: `factory.predict_address(transfer)` gives the `C…` address the customer pays into. Nothing is deployed yet. The address comes from the factory's address and a 32-byte salt, `sha256` of the transfer's `ScVal` XDR, so the same transfer always gives the same address and any change gives another.
2. **Pay**: the customer sends USDC or XLM to that address with the token's own `transfer`, through its Stellar Asset Contract (SAC). A SAC credits a contract address that has no contract yet; the integration tests prove it on the real SAC.
3. **Deploy and settle**: the operator calls `factory.settle(operator, transfer)`. In one transaction it deploys `kite` at the predicted address and settles:
   - checks the balance is at least `amount_in`;
   - returns anything above `amount_in` to the refund address;
   - swaps exactly `amount_in` through Soroswap, only if the customer paid a different token from the settlement token;
   - pays each fee from the settlement token;
   - sends the rest to the merchant.
4. **Refund**: `factory.refund(transfer, operator)` returns what the contract holds to the refund address. Before `expires_at` only an operator can call it. After it, anyone can, passing no operator, so a customer is never stuck waiting on a server.
5. **Recover**: `factory.recover(operator, transfer, token, destination)` sends tokens left behind, such as a late payment or a refund that couldn't be delivered, to `destination` or the refund address.

Soroban calls are synchronous and atomic: a settlement happens in full or not at all, so unlike NEAR there are no resumable steps. A transfer that's never paid is never deployed, and costs nothing.

### Details worth knowing

- **Most transfers don't swap.** When the customer pays in the settlement token, `swap` is `None` and `settle` never calls a router. That path needs no Soroswap deployment anywhere.
- **The swap is measured.** The excess is refunded in `pay_in` first, then exactly `amount_in` is swapped. The output is the settlement token balance after the swap minus before, not the router's return value, and `min_amount_out` is enforced on it. Below it, the whole `settle` fails and the transfer stays `Open`, to be settled again or refunded.
- **A refund that can't be delivered never blocks anything.** The refund address may have no trustline, for example. The excess or refund is sent with `try_transfer`; if that fails, the amount stays in the contract for `recover`, and `refunded` reports what was actually sent (0). The transfer still closes.
- **A fee or merchant payout that can't be delivered fails the settlement.** If a fee recipient or the merchant has no trustline, `settle` fails and the transfer stays `Open`, to be refunded.
- **Deploying is idempotent.** `settle`, `refund` and `recover` reuse the contract if it's already there.
- **Only the factory controls a deposit.** `kite` calls the factory's `require_auth` on every change. The factory is the direct caller, so that needs no signature, and nobody else can move funds.

## Stellar specifics

- **Contract addresses (`C…`)** come from the deployer, a salt and the network: the same transfer has a different address on testnet and on mainnet.
- **SACs.** Native XLM and issued assets like USDC each have a Stellar Asset Contract, a SEP-41 token. The contracts only use SEP-41 (`balance`, `transfer`). Other SEP-41 tokens should work, but only SACs are tested.
- **Paying a `C…` address.** Classic payment operations, and most exchanges, can't send to a contract address. Customers pay with the token's `transfer`, from a wallet that can, or from Kite's checkout.
- **Trustlines.** A `G…` account needs a trustline to hold an issued asset like USDC; native XLM needs none, and neither does a contract.
- **Muxed addresses (`M…`, CAP-67).** Every recipient is a `MuxedAddress`, so a payout or refund can go to a `G…`, `C…` or `M…` address. A Soroban transaction can't carry a memo, so an `M…` address is how a refund reaches an exchange's shared account. The SAC's `transfer` event carries the `to_muxed_id`.
- **TTL.** Contract data and code expire unless their TTL is extended. Both contracts keep their state in instance storage and extend it to 30 days whenever they're touched, once fewer than 7 days are left. If a finished deposit is archived, nothing is lost: its balances are entries of the token, and the deposit can be restored with a `RestoreFootprint` operation before `recover`.

## Contract interface

### `factory`

| Method                                                                         | Who                           | What it does                                                                                                                           |
| ------------------------------------------------------------------------------ | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `__constructor(admin, operator: Option, implementation)`                       | once                          | Sets the admin, an optional first operator and the `kite` Wasm hash.                                                                   |
| `predict_address(transfer)`                                                    | view                          | Validates the transfer and returns its deposit address. Fails for an implementation the factory never accepted.                        |
| `admin()`, `is_operator(operator)`, `implementation()`, `is_implementation(h)` | view                          | Roles and accepted versions.                                                                                                           |
| `settle(operator, transfer)`                                                   | operator                      | Deploys if needed, then settles. Fails with `InsufficientPayment`, without deploying, until `amount_in` has arrived.                   |
| `refund(transfer, operator: Option)`                                           | operator; anyone after expiry | Deploys if needed, then refunds. `operator` is `None` after expiry. Fails with `NothingToRefund`, without deploying, if unpaid.        |
| `recover(operator, transfer, token, destination: Option<MuxedAddress>)`        | operator                      | Deploys if needed, then sends all of `token` to `destination` or the refund address. Refused for the transfer's own tokens while open. |
| `set_implementation(hash)`                                                     | admin                         | Accepts an uploaded `kite` and uses it for new transfers. Every version accepted before stays accepted.                                |
| `grant_operator(operator)`, `revoke_operator(operator)`                        | admin                         | Manages operators.                                                                                                                     |
| `set_admin(admin)`                                                             | admin                         | Hands over the factory.                                                                                                                |
| `upgrade(wasm_hash)`                                                           | admin                         | Replaces the factory's code, keeping its address and storage.                                                                          |

Operators pass their own address, which the factory checks with `require_auth` and its operator list. The customer never signs anything for `settle`, `refund` or `recover`.

### `kite`

The contract each deposit runs. The factory deploys it with `__constructor(factory, transfer)`, and only the factory calls `settle()`, `refund()` and `recover(token, destination)`. Its views are `get_factory`, `get_transfer` and `get_status`. A transfer's status is `Open`, then `Settled` or `Refunded`.

### `Transfer`

```rust
pub struct Transfer {
    pub id: BytesN<32>,              // the server's transfer uuid, zero-padded
    pub pay_in: Address,             // SAC / SEP-41 token
    pub amount_in: i128,
    pub settlement_token: Address,
    pub settlement_recipient: MuxedAddress,
    pub min_amount_out: i128,
    pub fees: Vec<Fee>,              // at most MAX_FEES (10)
    pub refund_address: MuxedAddress,
    pub expires_at: u64,             // unix seconds
    pub swap: SwapRoute,             // None when pay_in == settlement_token
    pub implementation: BytesN<32>,  // kite Wasm hash this transfer runs
}

pub struct Fee { pub recipient: MuxedAddress, pub amount: i128 }
pub struct Swap { pub router: Address, pub path: Vec<Address> } // pay_in, …, settlement_token
pub enum SwapRoute { None, Some(Swap) }
```

`SwapRoute` is `Option<Swap>` as an enum: soroban-sdk can't derive its test conversions for an `Option` of a struct inside a struct. In XDR it's `["None"]` or `["Some", {path, router}]`.

`validate()` checks: a non-zero id; positive `amount_in` and `min_amount_out`; at most 10 fees, each positive, their sum not overflowing nor above `min_amount_out`; and the route: `None` exactly when `pay_in == settlement_token`, otherwise a path of 2 to 4 (`MAX_PATH`) tokens from `pay_in` to `settlement_token`.

### Swaps (Soroswap)

`settle` calls the router's `swap_exact_tokens_for_tokens(amount_in, min_amount_out, path, to, deadline)`, with `to` the deposit contract and `deadline` `expires_at`. This is the router of [soroswap/core](https://github.com/soroswap/core) at commit `6eade00` (`contracts/router/src/lib.rs`):

- it calls `to.require_auth()`, which the deposit satisfies as the router's direct caller;
- it then pulls `amount_in` with `transfer(to, pair, amount_in)` on the first token, into the first pair. The deposit authorizes exactly that call with `authorize_as_current_contract`, naming the token, the pair (`router_pair_for(path[0], path[1])`) and the amount.

The integration tests run Soroswap's factory, pair and router Wasm as deployed on testnet (hashes `86285a92…`, `8447525e…` and `4b95bbf9…`, those `public/testnet.contracts.json` lists), with only the operator's signature mocked.

### Events

Every event's topics are `["kite", <name>]`, and its data is a map:

| Event                | Contract | Data                                                                        |
| -------------------- | -------- | --------------------------------------------------------------------------- |
| `deployed`           | factory  | `transfer_id`, `address`, `implementation`                                  |
| `settled`            | kite     | `transfer_id`, `amount_out`, `recipient`, `amount` (to the merchant)        |
| `fee_paid`           | kite     | `transfer_id`, `index` (in `transfer.fees`), `recipient`, `amount`          |
| `refunded`           | kite     | `transfer_id`, `token`, `refund_address`, `amount_refunded` (what was sent) |
| `excess_refunded`    | kite     | `transfer_id`, `token`, `to`, `amount` (only when it was sent)              |
| `recovered`          | kite     | `transfer_id`, `token`, `to`, `amount`                                      |
| `implementation_set` | factory  | `implementation`                                                            |
| `operator_granted`   | factory  | `operator`                                                                  |
| `operator_revoked`   | factory  | `operator`                                                                  |

A deposit's events are published by the deposit contract, so their contract id is the pay-in address. The Kite server reads them from each transaction it sends.

### Errors

`NotOperator` 1, `InvalidId` 2, `InvalidAmount` 3, `TooManyFees` 4, `InvalidFee` 5, `FeesExceedMinAmountOut` 6, `InvalidSwap` 7, `UnknownImplementation` 8, `NotOpen` 9, `InsufficientPayment` 10, `NothingToRefund` 11, `NotExpired` 12, `RecoveryNotAllowed` 13, `SlippageExceeded` 14, `NothingToRecover` 15.

## Upgrades

- **`kite` code**: upload the new version (`sh_scripts/upload-kite.sh`), then have the admin run `set_implementation` with its hash. Each transfer names the version it runs, and the factory accepts every version it has used. A transfer created before an upgrade still settles, refunds and recovers with its own code, and no upgrade can change the code of a deposit that already holds funds.
- **`factory` code**: upload the new Wasm, then the admin calls `upgrade` with its hash. Its address, roles and accepted versions stay the same.

A new `kite` version changes the address of every new transfer, since the address commits to the transfer and the transfer to the hash. `types` pins two fixed vectors (`vector::fixed_vector`, a same-token transfer, and `vector::fixed_vector_with_swap`), and the server's `kiteStellarContract` test pins the same ones.

## Development

You need Rust through `rustup` (`rust-toolchain.toml` pins the version), the stellar CLI and jq. `sh_scripts/setup.sh` installs the CLI; on Linux, building it needs `libdbus-1-dev` and `libudev-dev`.

```sh
./sh_scripts/setup.sh
./sh_scripts/test.sh              # unit tests
./sh_scripts/test.sh integration  # builds both contracts, then the integration tests
./sh_scripts/lint.sh              # rustfmt, clippy, shellcheck
./sh_scripts/build.sh             # stellar contract build, and each Wasm's hash
```

soroban-sdk 29 only builds a contract Wasm through `stellar contract build`. The integration tests in `tests/integration` import the built `kite` and `factory`, use real SACs from the Soroban test environment, and Soroswap's testnet Wasm from `tests/integration/wasm/`.

## Scripts

Everything in `sh_scripts/` runs from any directory and shares `_lib.sh`.

| Variable             | Meaning                                                                                    |
| -------------------- | ------------------------------------------------------------------------------------------ |
| `STELLAR_NETWORK`    | `testnet`, the default and the only network the scripts send to.                           |
| `STELLAR_ACCOUNT`    | The signer: a key name in the stellar CLI's keychain (`stellar keys`).                     |
| `STELLAR_SECRET_KEY` | A secret key to sign with instead, read from the environment, never put on a command line. |
| `KITE_FACTORY`       | The factory, for `admin.sh` and `status.sh`. Defaults to the one in `deployments/`.        |
| `YES=1`              | Skip the confirmation prompt.                                                              |

| Script                                               | What it does                                                                                               |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `setup.sh [--no-cli]`                                | Adds the `wasm32v1-none` target and installs the stellar CLI.                                              |
| `build.sh`                                           | Builds `kite` and `factory`, and prints each Wasm's size and hash.                                         |
| `test.sh [unit\|integration\|all]`                   | Runs the tests. Anything after the suite goes to `cargo test`.                                             |
| `lint.sh [--fix]`                                    | `cargo fmt`, clippy with warnings as errors, and shellcheck if it's installed.                             |
| `upload-kite.sh`                                     | Uploads `kite` and prints its hash.                                                                        |
| `ADMIN= OPERATOR= IMPLEMENTATION= deploy-factory.sh` | Deploys the factory.                                                                                       |
| `admin.sh <command> <arg>`                           | `set-implementation`, `grant-operator`, `revoke-operator`, `set-admin`, `upgrade`.                         |
| `status.sh [operator…]`                              | Reads the factory: admin, implementation, whether your build matches, whether each address is an operator. |

The scripts that send record what they did in `deployments/stellar-testnet.json`. `status.sh` sends nothing.

### Deploying on testnet

1. **A deployer**: `stellar keys generate kite-deployer --network testnet --fund` creates a key and funds it with Friendbot. Never commit keys.
2. **The operator**: the Kite server's Stellar operator is the key of `KITE_OPERATOR_MNEMONIC` on the SEP-5 path `m/44'/148'/0'`. `stellar keys add kite-operator --seed-phrase` imports it (account 0), and `stellar keys address kite-operator` prints its `G…` address.
3. **Upload `kite`**: `STELLAR_ACCOUNT=kite-deployer ./sh_scripts/upload-kite.sh`. Note the hash.
4. **Deploy the factory**: `STELLAR_ACCOUNT=kite-deployer ADMIN=<admin G…> OPERATOR=<operator G…> IMPLEMENTATION=<hash> ./sh_scripts/deploy-factory.sh`. Use an admin the server never holds.
5. **Configure the server**: put the factory's `C…` address in its `stellar:testnet` chain config. The server refuses a factory where its operator isn't one.
6. **Fund the operator** with Friendbot: it pays the fees of every transfer.

Testnet is reset every few months, which removes every contract on it. Deploy again after a reset.

## CI

The **CI** workflow runs on every pull request and every push to `main`: **Lint** (`lint.sh`), **Unit tests** (`test.sh unit`) and **Integration tests** (`test.sh integration`, which also uploads the built Wasm and puts its hashes in the run summary). Nothing is deployed from CI.

## Project layout

```text
contracts/
  types/       Transfer, Fee, Swap, validation, the salt, the fixed vectors, and the kite client
  kite/        the deposit contract: settle, refund, recover
  factory/     roles, versions, predict_address, and the calls into deposits
tests/
  integration/ factory + kite + real SACs + Soroswap's testnet Wasm
sh_scripts/    setup, build, test, lint, upload, deploy, admin and status
deployments/   what the scripts deployed
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and tests. To report a vulnerability, follow [SECURITY.md](SECURITY.md) rather than opening an issue.

## License

[MIT](LICENSE)
