#![cfg(not(target_family = "wasm"))]
// the imported Soroswap clients take many arguments
#![allow(clippy::too_many_arguments)]

extern crate std;

use factory::{Fee, Swap, SwapRoute, Transfer};
use soroban_sdk::{
    testutils::{Address as _, Ledger, MuxedAddress as _},
    token::{StellarAssetClient, TokenClient},
    vec, xdr, Address, BytesN, Env, MuxedAddress, Vec,
};
use std::rc::Rc;

pub mod factory {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/factory.wasm");
}

pub mod kite {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/kite.wasm");
}

/// Soroswap as deployed on testnet (soroswap/core, commit 6eade00).
pub mod soroswap {
    pub mod factory {
        soroban_sdk::contractimport!(file = "wasm/soroswap_factory.wasm");
    }
    pub mod router {
        soroban_sdk::contractimport!(file = "wasm/soroswap_router.wasm");
    }
    pub const PAIR: &[u8] = include_bytes!("../wasm/soroswap_pair.wasm");
}

pub const NOW: u64 = 1_700_000_000;
pub const EXPIRES_AT: u64 = NOW + 3_600;

pub struct Kite {
    pub env: Env,
    pub factory: factory::Client<'static>,
    pub admin: Address,
    pub operator: Address,
    pub implementation: BytesN<32>,
}

pub fn setup() -> Kite {
    let env = Env::default();
    env.ledger().set_timestamp(NOW);
    env.cost_estimate().budget().reset_unlimited();

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let implementation = env.deployer().upload_contract_wasm(kite::WASM);
    let id = env.register(
        factory::WASM,
        (
            admin.clone(),
            Some(operator.clone()),
            implementation.clone(),
        ),
    );

    Kite {
        factory: factory::Client::new(&env, &id),
        env,
        admin,
        operator,
        implementation,
    }
}

pub fn token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

pub fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    env.mock_all_auths();
    StellarAssetClient::new(env, token).mint(to, &amount);
    env.set_auths(&[]);
}

pub fn balance(env: &Env, token: &Address, of: &Address) -> i128 {
    TokenClient::new(env, token).balance(of)
}

/// A G account with no trustlines.
pub fn untrusted(env: &Env) -> Address {
    let account = MuxedAddress::generate(env).address();
    let xdr::ScAddress::Account(account_id) = xdr::ScAddress::from(&account) else {
        unreachable!()
    };
    let key = Rc::new(xdr::LedgerKey::Account(xdr::LedgerKeyAccount {
        account_id: account_id.clone(),
    }));
    let entry = Rc::new(xdr::LedgerEntry {
        data: xdr::LedgerEntryData::Account(xdr::AccountEntry {
            account_id,
            balance: 0,
            flags: 0,
            home_domain: Default::default(),
            inflation_dest: None,
            num_sub_entries: 0,
            seq_num: xdr::SequenceNumber(0),
            thresholds: xdr::Thresholds([1; 4]),
            signers: Default::default(),
            ext: xdr::AccountEntryExt::V0,
        }),
        last_modified_ledger_seq: 0,
        ext: xdr::LedgerEntryExt::V0,
    });
    env.host().add_ledger_entry(&key, &entry, None).unwrap();
    account
}

/// A G account with a trustline on each of `tokens`.
pub fn account(env: &Env, tokens: &[&Address]) -> Address {
    let account = untrusted(env);
    env.mock_all_auths();
    for token in tokens {
        StellarAssetClient::new(env, token).trust(&account);
    }
    env.set_auths(&[]);
    account
}

/// The customer pays with the token's own `transfer`, as a wallet does.
pub fn pay(env: &Env, token: &Address, to: &Address, amount: i128) {
    let customer = account(env, &[token]);
    mint(env, token, &customer, amount);
    env.mock_all_auths();
    TokenClient::new(env, token).transfer(&customer, to, &amount);
    env.set_auths(&[]);
}

pub fn transfer(k: &Kite, pay_in: &Address, settlement: &Address, amount_in: i128) -> Transfer {
    let env = &k.env;
    Transfer {
        id: BytesN::from_array(env, &[7; 32]),
        pay_in: pay_in.clone(),
        amount_in,
        settlement_token: settlement.clone(),
        settlement_recipient: account(env, &[settlement]).into(),
        min_amount_out: 1_000,
        fees: vec![
            env,
            Fee {
                recipient: account(env, &[settlement]).into(),
                amount: 10,
            },
        ],
        refund_address: account(env, &[pay_in]).into(),
        expires_at: EXPIRES_AT,
        swap: SwapRoute::None,
        implementation: k.implementation.clone(),
    }
}

pub fn with_swap(mut t: Transfer, router: &Address, path: Vec<Address>) -> Transfer {
    t.swap = SwapRoute::Some(Swap {
        router: router.clone(),
        path,
    });
    t
}

/// Soroswap's factory, pairs and router, with liquidity on each `(a, b, amount_a, amount_b)`.
pub fn soroswap(env: &Env, pools: &[(&Address, &Address, i128, i128)]) -> Address {
    env.mock_all_auths();
    let setter = Address::generate(env);
    let pair_hash = env.deployer().upload_contract_wasm(soroswap::PAIR);
    let factory = env.register(soroswap::factory::WASM, ());
    soroswap::factory::Client::new(env, &factory).initialize(&setter, &pair_hash);

    let router = env.register(soroswap::router::WASM, ());
    let client = soroswap::router::Client::new(env, &router);
    client.initialize(&factory);

    let provider = Address::generate(env);
    for (a, b, amount_a, amount_b) in pools {
        StellarAssetClient::new(env, a).mint(&provider, amount_a);
        StellarAssetClient::new(env, b).mint(&provider, amount_b);
        client.add_liquidity(a, b, amount_a, amount_b, &0, &0, &provider, &EXPIRES_AT);
    }
    env.set_auths(&[]);
    router
}
