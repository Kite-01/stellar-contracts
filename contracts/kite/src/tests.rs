extern crate std;

use std::rc::Rc;

use super::*;
use kite_types::{Fee, SwapRoute};
use soroban_sdk::{
    testutils::{Address as _, Events, MuxedAddress as _},
    token::StellarAssetClient,
    xdr, Event,
};

struct Setup {
    env: Env,
    kite: KiteClient<'static>,
    usdc: Address,
    transfer: Transfer,
}

fn token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn untrusted(env: &Env) -> MuxedAddress {
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
    account.into()
}

// A G account with a trustline for `token`.
fn account(env: &Env, token: &Address) -> MuxedAddress {
    let account = untrusted(env);
    StellarAssetClient::new(env, token).trust(&account.address());
    account
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let usdc = token(&env);
    let transfer = Transfer {
        id: BytesN::from_array(&env, &[1; 32]),
        pay_in: usdc.clone(),
        amount_in: 1_000,
        settlement_token: usdc.clone(),
        settlement_recipient: account(&env, &usdc),
        min_amount_out: 1_000,
        fees: vec![
            &env,
            Fee {
                recipient: account(&env, &usdc),
                amount: 10,
            },
            Fee {
                recipient: Address::generate(&env).into(),
                amount: 20,
            },
        ],
        refund_address: account(&env, &usdc),
        expires_at: 1_800_000_000,
        swap: SwapRoute::None,
        implementation: BytesN::from_array(&env, &[5; 32]),
    };

    let id = env.register(Kite, (Address::generate(&env), transfer.clone()));
    let kite = KiteClient::new(&env, &id);

    Setup {
        env,
        kite,
        usdc,
        transfer,
    }
}

fn pay(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.usdc).mint(&s.kite.address, &amount);
}

fn balance(s: &Setup, who: &MuxedAddress) -> i128 {
    TokenClient::new(&s.env, &s.usdc).balance(&who.address())
}

#[test]
fn settles_an_exact_payment() {
    let s = setup();
    pay(&s, 1_000);

    assert_eq!(s.kite.settle(), 1_000);
    let settled = Settled {
        transfer_id: s.transfer.id.clone(),
        amount_out: 1_000,
        recipient: s.transfer.settlement_recipient.clone(),
        amount: 970,
    };
    assert_eq!(
        s.env
            .events()
            .all()
            .filter_by_contract(&s.kite.address)
            .events()
            .last(),
        Some(&settled.to_xdr(&s.env, &s.kite.address))
    );

    assert_eq!(s.kite.get_status(), Status::Settled);
    assert_eq!(balance(&s, &s.transfer.settlement_recipient), 970);
    assert_eq!(balance(&s, &s.transfer.fees.get_unchecked(0).recipient), 10);
    assert_eq!(balance(&s, &s.transfer.fees.get_unchecked(1).recipient), 20);
}

#[test]
fn refunds_an_overpayment_before_settling() {
    let s = setup();
    pay(&s, 1_500);

    s.kite.settle();
    assert_eq!(balance(&s, &s.transfer.refund_address), 500);
    assert_eq!(balance(&s, &s.transfer.settlement_recipient), 970);
}

#[test]
fn an_undeliverable_excess_does_not_block_settlement() {
    let mut s = setup();
    s.transfer.refund_address = untrusted(&s.env);
    s.kite = KiteClient::new(
        &s.env,
        &s.env
            .register(Kite, (Address::generate(&s.env), s.transfer.clone())),
    );
    pay(&s, 1_500);

    s.kite.settle();
    assert_eq!(s.kite.get_status(), Status::Settled);
    assert_eq!(
        TokenClient::new(&s.env, &s.usdc).balance(&s.kite.address),
        500
    );
}

#[test]
fn refuses_an_underpayment() {
    let s = setup();
    pay(&s, 999);

    assert_eq!(s.kite.try_settle(), Err(Ok(Error::InsufficientPayment)));
    assert_eq!(s.kite.get_status(), Status::Open);
}

#[test]
fn a_fee_or_merchant_without_trustline_fails_the_settlement() {
    for fee in [true, false] {
        let mut s = setup();
        let untrusted: MuxedAddress = untrusted(&s.env);
        if fee {
            s.transfer.fees.set(
                0,
                Fee {
                    recipient: untrusted,
                    amount: 10,
                },
            );
        } else {
            s.transfer.settlement_recipient = untrusted;
        }
        s.kite = KiteClient::new(
            &s.env,
            &s.env
                .register(Kite, (Address::generate(&s.env), s.transfer.clone())),
        );
        pay(&s, 1_000);

        assert!(s.kite.try_settle().is_err());
        assert_eq!(s.kite.get_status(), Status::Open);
        assert_eq!(s.kite.refund(), 1_000);
    }
}

#[test]
fn refunds_to_a_muxed_address() {
    let mut s = setup();
    let base = account(&s.env, &s.usdc);
    s.transfer.refund_address = MuxedAddress::new(base.clone(), 1234567890);
    s.kite = KiteClient::new(
        &s.env,
        &s.env
            .register(Kite, (Address::generate(&s.env), s.transfer.clone())),
    );
    pay(&s, 1_000);

    assert_eq!(s.kite.refund(), 1_000);

    // the SAC's transfer event carries the muxed id
    let sac_event = s
        .env
        .events()
        .all()
        .filter_by_contract(&s.usdc)
        .events()
        .last()
        .unwrap()
        .clone();
    let xdr::ContractEventBody::V0(body) = sac_event.body;
    let xdr::ScVal::Map(Some(data)) = body.data else {
        panic!("expected map data, got {:?}", body.data);
    };
    assert!(data.iter().any(
        |e| e.key == xdr::ScVal::Symbol("to_muxed_id".try_into().unwrap())
            && e.val == xdr::ScVal::U64(1234567890)
    ));

    assert_eq!(s.kite.get_status(), Status::Refunded);
    assert_eq!(balance(&s, &base), 1_000);
}

#[test]
fn an_undeliverable_refund_closes_and_leaves_funds_to_recover() {
    let mut s = setup();
    s.transfer.refund_address = untrusted(&s.env);
    s.kite = KiteClient::new(
        &s.env,
        &s.env
            .register(Kite, (Address::generate(&s.env), s.transfer.clone())),
    );
    pay(&s, 1_000);

    assert_eq!(s.kite.refund(), 0);
    assert_eq!(s.kite.get_status(), Status::Refunded);

    let to = account(&s.env, &s.usdc);
    assert_eq!(s.kite.recover(&s.usdc, &to), 1_000);
    assert_eq!(balance(&s, &to), 1_000);
}

#[test]
fn recover_refuses_own_tokens_while_open() {
    let s = setup();
    pay(&s, 1_000);
    let to = account(&s.env, &s.usdc);

    assert_eq!(
        s.kite.try_recover(&s.usdc, &to),
        Err(Ok(Error::RecoveryNotAllowed))
    );

    let other = token(&s.env);
    StellarAssetClient::new(&s.env, &other).mint(&s.kite.address, &5);
    let to = account(&s.env, &other);
    assert_eq!(s.kite.recover(&other, &to), 5);
}

#[test]
fn closed_transfers_stay_closed() {
    let s = setup();
    pay(&s, 2_000);
    s.kite.settle();

    assert_eq!(s.kite.try_settle(), Err(Ok(Error::NotOpen)));
    assert_eq!(s.kite.try_refund(), Err(Ok(Error::NotOpen)));
}

#[test]
fn only_the_factory_can_call() {
    let s = setup();
    pay(&s, 1_000);
    s.env.set_auths(&[]);

    assert!(s.kite.try_settle().is_err());
    assert!(s.kite.try_refund().is_err());
    assert_eq!(s.kite.get_status(), Status::Open);
}
