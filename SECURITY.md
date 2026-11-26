# Security policy

These contracts hold and move funds, so we take reports seriously and will work with you to fix them.

## Reporting a vulnerability

Email [security@usekite.site](mailto:security@usekite.site) with:

- what the issue is and what an attacker could do with it
- the contract and method involved, and the commit, Wasm hash or contract address you looked at
- steps or a test that reproduces it, if you have one

Please don't open a public issue, pull request or discussion about it, and don't exploit it on a live network beyond what's needed to show it exists.

## What happens next

- We reply within 3 working days to confirm we've received it.
- We keep you updated while we investigate and fix it.
- Once a fix is deployed, we publish the details and credit you, unless you'd rather stay anonymous.

## In scope

- `factory`, `kite` and `types` in `contracts/`
- The deployments recorded under `deployments/`, and the `C…` deposit contracts they create
- `predict_address`: anything that lets two transfers share a deposit address, or a deposit run code the factory didn't allow

The Soroswap Wasm in `tests/integration/wasm/` is a test fixture. Issues in soroban-sdk, the SACs or Soroswap should go to those projects, though we'd like to hear if one affects Kite.

## Known limits

These are by design; reports that only restate them are out of scope.

- `recover` lets the factory's operator send tokens that aren't the transfer's own anywhere, and the transfer's own tokens once it's closed.
- A refund that can't be delivered closes the transfer and leaves the funds for an operator to `recover`.
- Swaps use the router's spot price; `min_amount_out` is the only protection, and the server sets it.
