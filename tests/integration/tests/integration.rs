use integration::factory::{Error, Fee, Transfer};
use integration::kite::Status;
use integration::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke, MuxedAddress as _},
    vec, Address, BytesN, IntoVal, MuxedAddress, Val, Vec,
};

fn as_operator(k: &Kite, fn_name: &str, args: Vec<Val>) {
    k.env.mock_auths(&[MockAuth {
        address: &k.operator,
        invoke: &MockAuthInvoke {
            contract: &k.factory.address,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

fn settle(k: &Kite, t: &Transfer) -> Result<i128, Error> {
    as_operator(
        k,
        "settle",
        (k.operator.clone(), t.clone()).into_val(&k.env),
    );
    match k.factory.try_settle(&k.operator, t) {
        Ok(Ok(amount)) => Ok(amount),
        Err(Ok(error)) => Err(error),
        other => panic!("settle failed: {other:?}"),
    }
}

fn refund(k: &Kite, t: &Transfer) -> Result<i128, Error> {
    let operator = Some(k.operator.clone());
    as_operator(k, "refund", (t.clone(), operator.clone()).into_val(&k.env));
    match k.factory.try_refund(t, &operator) {
        Ok(Ok(amount)) => Ok(amount),
        Err(Ok(error)) => Err(error),
        other => panic!("refund failed: {other:?}"),
    }
}

fn recover(k: &Kite, t: &Transfer, token: &Address, to: &MuxedAddress) -> Result<i128, Error> {
    let to = Some(to.clone());
    as_operator(
        k,
        "recover",
        (k.operator.clone(), t.clone(), token.clone(), to.clone()).into_val(&k.env),
    );
    match k.factory.try_recover(&k.operator, t, token, &to) {
        Ok(Ok(amount)) => Ok(amount),
        Err(Ok(error)) => Err(error),
        other => panic!("recover failed: {other:?}"),
    }
}

fn deployed(k: &Kite, address: &Address) -> bool {
    kite::Client::new(&k.env, address).try_get_status().is_ok()
}

fn status(k: &Kite, address: &Address) -> Status {
    kite::Client::new(&k.env, address).get_status()
}

#[test]
fn a_sac_credits_an_undeployed_contract_address() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);

    pay(&k.env, &usdc, &address, 1_000);
    assert!(!deployed(&k, &address));
    assert_eq!(balance(&k.env, &usdc, &address), 1_000);

    assert_eq!(settle(&k, &t), Ok(1_000));
    assert!(deployed(&k, &address));
    assert_eq!(status(&k, &address), Status::Settled);
    assert_eq!(
        balance(&k.env, &usdc, &t.settlement_recipient.address()),
        990
    );
    assert_eq!(
        balance(&k.env, &usdc, &t.fees.get_unchecked(0).recipient.address()),
        10
    );
}

#[test]
fn an_overpayment_is_refunded_on_settlement() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_250);

    assert_eq!(settle(&k, &t), Ok(1_000));
    assert_eq!(balance(&k.env, &usdc, &t.refund_address.address()), 250);
    assert_eq!(balance(&k.env, &usdc, &address), 0);
}

#[test]
fn an_unpaid_transfer_fails_without_deploying() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);

    assert_eq!(settle(&k, &t), Err(Error::InsufficientPayment));
    assert_eq!(refund(&k, &t), Err(Error::NothingToRefund));
    assert!(!deployed(&k, &address));

    pay(&k.env, &usdc, &address, 999);
    assert_eq!(settle(&k, &t), Err(Error::InsufficientPayment));
    assert!(!deployed(&k, &address));
}

#[test]
fn a_same_token_transfer_never_touches_a_router() {
    let k = setup();
    let usdc = token(&k.env);
    // no Soroswap anywhere; the route must not be followed
    let t = transfer(&k, &usdc, &usdc, 1_000);
    pay(&k.env, &usdc, &k.factory.predict_address(&t), 1_000);

    assert_eq!(settle(&k, &t), Ok(1_000));
}

#[test]
fn settles_through_a_soroswap_swap() {
    let k = setup();
    let (xlm, usdc) = (token(&k.env), token(&k.env));
    let router = soroswap(&k.env, &[(&xlm, &usdc, 10_000_000, 10_000_000)]);

    let t = with_swap(
        transfer(&k, &xlm, &usdc, 2_000),
        &router,
        vec![&k.env, xlm.clone(), usdc.clone()],
    );
    let address = k.factory.predict_address(&t);
    pay(&k.env, &xlm, &address, 2_000);

    let out = settle(&k, &t).unwrap();
    assert!((1_000..2_000).contains(&out), "{out}");
    assert_eq!(balance(&k.env, &xlm, &address), 0);
    assert_eq!(balance(&k.env, &usdc, &address), 0);
    assert_eq!(
        balance(&k.env, &usdc, &t.settlement_recipient.address()),
        out - 10
    );
}

#[test]
fn settles_through_a_two_hop_swap() {
    let k = setup();
    let (eurc, xlm, usdc) = (token(&k.env), token(&k.env), token(&k.env));
    let router = soroswap(
        &k.env,
        &[
            (&eurc, &xlm, 10_000_000, 10_000_000),
            (&xlm, &usdc, 10_000_000, 10_000_000),
        ],
    );

    let t = with_swap(
        transfer(&k, &eurc, &usdc, 2_000),
        &router,
        vec![&k.env, eurc.clone(), xlm.clone(), usdc.clone()],
    );
    pay(&k.env, &eurc, &k.factory.predict_address(&t), 2_000);

    let out = settle(&k, &t).unwrap();
    assert!(out >= 1_000, "{out}");
    assert_eq!(
        balance(&k.env, &usdc, &t.settlement_recipient.address()),
        out - 10
    );
}

#[test]
fn a_swap_below_min_amount_out_fails_and_stays_open() {
    let k = setup();
    let (xlm, usdc) = (token(&k.env), token(&k.env));
    let router = soroswap(&k.env, &[(&xlm, &usdc, 10_000_000, 1_000_000)]);

    // about 10 XLM per USDC: 2,000 XLM buys ~199 USDC, under the 1,000 floor
    let t = with_swap(
        transfer(&k, &xlm, &usdc, 2_000),
        &router,
        vec![&k.env, xlm.clone(), usdc.clone()],
    );
    let address = k.factory.predict_address(&t);
    pay(&k.env, &xlm, &address, 2_000);

    as_operator(
        &k,
        "settle",
        (k.operator.clone(), t.clone()).into_val(&k.env),
    );
    assert!(k.factory.try_settle(&k.operator, &t).is_err());
    assert!(!deployed(&k, &address));
    assert_eq!(balance(&k.env, &xlm, &address), 2_000);

    // the transfer can still be refunded, in the token paid
    assert_eq!(refund(&k, &t), Ok(2_000));
    assert_eq!(balance(&k.env, &xlm, &t.refund_address.address()), 2_000);
}

#[test]
fn refunds_are_operator_only_until_expiry() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_000);

    assert_eq!(k.factory.try_refund(&t, &None), Err(Ok(Error::NotExpired)));

    let stranger = Address::generate(&k.env);
    k.env.mock_all_auths();
    assert_eq!(
        k.factory.try_refund(&t, &Some(stranger)),
        Err(Ok(Error::NotOperator))
    );
    k.env.set_auths(&[]);

    k.env.ledger().set_timestamp(EXPIRES_AT + 1);
    assert_eq!(k.factory.refund(&t, &None), 1_000);
    assert_eq!(status(&k, &address), Status::Refunded);
    assert_eq!(balance(&k.env, &usdc, &t.refund_address.address()), 1_000);
}

#[test]
fn an_operator_refunds_before_expiry() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    pay(&k.env, &usdc, &k.factory.predict_address(&t), 400);

    assert_eq!(refund(&k, &t), Ok(400));
    assert_eq!(balance(&k.env, &usdc, &t.refund_address.address()), 400);
}

#[test]
fn refunds_to_a_muxed_address() {
    let k = setup();
    let usdc = token(&k.env);
    let mut t = transfer(&k, &usdc, &usdc, 1_000);
    let exchange = account(&k.env, &[&usdc]);
    t.refund_address = MuxedAddress::new(exchange.clone(), 42);
    pay(&k.env, &usdc, &k.factory.predict_address(&t), 1_000);

    assert_eq!(refund(&k, &t), Ok(1_000));
    assert_eq!(balance(&k.env, &usdc, &exchange), 1_000);
}

#[test]
fn payouts_without_a_trustline() {
    let k = setup();
    let usdc = token(&k.env);

    // fee
    let mut t = transfer(&k, &usdc, &usdc, 1_000);
    t.fees.set(
        0,
        Fee {
            recipient: untrusted(&k.env).into(),
            amount: 10,
        },
    );
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_000);
    as_operator(
        &k,
        "settle",
        (k.operator.clone(), t.clone()).into_val(&k.env),
    );
    assert!(k.factory.try_settle(&k.operator, &t).is_err());
    assert_eq!(refund(&k, &t), Ok(1_000));

    // merchant
    let mut t = transfer(&k, &usdc, &usdc, 1_000);
    t.id = BytesN::from_array(&k.env, &[8; 32]);
    t.settlement_recipient = untrusted(&k.env).into();
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_000);
    as_operator(
        &k,
        "settle",
        (k.operator.clone(), t.clone()).into_val(&k.env),
    );
    assert!(k.factory.try_settle(&k.operator, &t).is_err());

    // refund: the transfer closes, and the payment waits for `recover`
    t.refund_address = untrusted(&k.env).into();
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_000);
    assert_eq!(refund(&k, &t), Ok(0));
    assert_eq!(status(&k, &address), Status::Refunded);
    let to = account(&k.env, &[&usdc]);
    assert_eq!(recover(&k, &t, &usdc, &to.clone().into()), Ok(1_000));
    assert_eq!(balance(&k.env, &usdc, &to), 1_000);
}

#[test]
fn recover_refuses_own_tokens_while_open() {
    let k = setup();
    let (usdc, other) = (token(&k.env), token(&k.env));
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 500);
    mint(&k.env, &other, &address, 7);

    let to: MuxedAddress = account(&k.env, &[&usdc, &other]).into();
    assert_eq!(recover(&k, &t, &usdc, &to), Err(Error::RecoveryNotAllowed));
    assert_eq!(recover(&k, &t, &other, &to), Ok(7));
    assert_eq!(status(&k, &address), Status::Open);

    // after settling, a late payment comes back out
    pay(&k.env, &usdc, &address, 500);
    settle(&k, &t).unwrap();
    pay(&k.env, &usdc, &address, 30);
    assert_eq!(recover(&k, &t, &usdc, &to), Ok(30));
}

#[test]
fn deploying_is_idempotent() {
    let k = setup();
    let (usdc, other) = (token(&k.env), token(&k.env));
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);

    // recover deploys first, then settle reuses the contract
    mint(&k.env, &other, &address, 7);
    let to: MuxedAddress = account(&k.env, &[&other]).into();
    assert_eq!(recover(&k, &t, &other, &to), Ok(7));
    assert!(deployed(&k, &address));

    pay(&k.env, &usdc, &address, 1_000);
    assert_eq!(settle(&k, &t), Ok(1_000));
}

#[test]
fn an_upgrade_keeps_old_transfers_on_their_implementation() {
    let k = setup();
    let usdc = token(&k.env);
    let old = transfer(&k, &usdc, &usdc, 1_000);
    let old_address = k.factory.predict_address(&old);
    pay(&k.env, &usdc, &old_address, 1_000);

    let new = k.env.deployer().upload_contract_wasm(soroswap::PAIR);
    k.env.mock_all_auths();
    k.factory.set_implementation(&new);
    k.env.set_auths(&[]);

    assert_eq!(k.factory.implementation(), new);
    assert!(k.factory.is_implementation(&k.implementation));
    assert_eq!(k.factory.predict_address(&old), old_address);
    assert_eq!(settle(&k, &old), Ok(1_000));

    let mut unknown = transfer(&k, &usdc, &usdc, 1_000);
    unknown.implementation = BytesN::from_array(&k.env, &[9; 32]);
    assert_eq!(
        k.factory.try_predict_address(&unknown),
        Err(Ok(Error::UnknownImplementation))
    );

    // the factory's own code can be replaced, keeping its storage
    let factory = k.env.deployer().upload_contract_wasm(factory::WASM);
    k.env.mock_all_auths();
    k.factory.upgrade(&factory);
    k.env.set_auths(&[]);
    assert_eq!(k.factory.admin(), k.admin);
    assert!(k.factory.is_operator(&k.operator));
}

#[test]
fn unauthorized_callers_are_refused() {
    let k = setup();
    let usdc = token(&k.env);
    let t = transfer(&k, &usdc, &usdc, 1_000);
    let address = k.factory.predict_address(&t);
    pay(&k.env, &usdc, &address, 1_000);
    let stranger = Address::generate(&k.env);

    // no signature at all
    assert!(k.factory.try_settle(&k.operator, &t).is_err());
    assert!(k.factory.try_refund(&t, &Some(k.operator.clone())).is_err());
    assert!(k.factory.try_set_implementation(&k.implementation).is_err());
    assert!(k.factory.try_grant_operator(&stranger).is_err());
    assert!(k.factory.try_revoke_operator(&k.operator).is_err());
    assert!(k.factory.try_set_admin(&stranger).is_err());
    assert!(k.factory.try_upgrade(&k.implementation).is_err());

    // signed, but not an operator
    k.env.mock_all_auths();
    assert_eq!(
        k.factory.try_settle(&stranger, &t),
        Err(Ok(Error::NotOperator))
    );
    let to: MuxedAddress = stranger.clone().into();
    assert_eq!(
        k.factory.try_recover(&stranger, &t, &usdc, &Some(to)),
        Err(Ok(Error::NotOperator))
    );
    k.env.set_auths(&[]);

    // the deposit contract only answers the factory
    let other = token(&k.env);
    mint(&k.env, &other, &address, 7);
    let to: MuxedAddress = account(&k.env, &[&other]).into();
    assert_eq!(recover(&k, &t, &other, &to), Ok(7));
    let kite = kite::Client::new(&k.env, &address);
    assert!(kite.try_settle().is_err());
    assert!(kite.try_refund().is_err());
    assert_eq!(kite.get_status(), Status::Open);

    // a revoked operator
    k.env.mock_all_auths();
    k.factory.revoke_operator(&k.operator);
    k.env.set_auths(&[]);
    assert!(!k.factory.is_operator(&k.operator));
    assert_eq!(settle(&k, &t), Err(Error::NotOperator));
}
