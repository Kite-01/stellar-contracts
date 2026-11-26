#![no_std]

use kite_types::{Error, Status, Transfer, TTL_EXTEND_TO, TTL_THRESHOLD};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractclient, contractevent, contractimpl, contracttype,
    token::TokenClient,
    vec, Address, BytesN, Env, IntoVal, MuxedAddress, Symbol, Vec,
};

/// Soroswap's router (soroswap/core `contracts/router`, commit 6eade00).
#[contractclient(name = "RouterClient")]
pub trait Router {
    fn router_pair_for(env: Env, token_a: Address, token_b: Address) -> Address;
    fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128>;
}

#[contracttype]
enum DataKey {
    Factory,
    Transfer,
    Status,
}

#[contractevent(topics = ["kite", "settled"])]
pub struct Settled {
    pub transfer_id: BytesN<32>,
    pub amount_out: i128,
    pub recipient: MuxedAddress,
    pub amount: i128,
}

#[contractevent(topics = ["kite", "fee_paid"])]
pub struct FeePaid {
    pub transfer_id: BytesN<32>,
    pub index: u32,
    pub recipient: MuxedAddress,
    pub amount: i128,
}

#[contractevent(topics = ["kite", "refunded"])]
pub struct Refunded {
    pub transfer_id: BytesN<32>,
    pub token: Address,
    pub refund_address: MuxedAddress,
    pub amount_refunded: i128,
}

#[contractevent(topics = ["kite", "excess_refunded"])]
pub struct ExcessRefunded {
    pub transfer_id: BytesN<32>,
    pub token: Address,
    pub to: MuxedAddress,
    pub amount: i128,
}

#[contractevent(topics = ["kite", "recovered"])]
pub struct Recovered {
    pub transfer_id: BytesN<32>,
    pub token: Address,
    pub to: MuxedAddress,
    pub amount: i128,
}

#[contract]
pub struct Kite;

fn status(env: &Env) -> Status {
    env.storage().instance().get(&DataKey::Status).unwrap()
}

fn set_status(env: &Env, status: Status) {
    env.storage().instance().set(&DataKey::Status, &status);
}

fn extend(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn enter(env: &Env) -> (Status, Transfer) {
    let factory: Address = env.storage().instance().get(&DataKey::Factory).unwrap();
    factory.require_auth();
    extend(env);

    let transfer = env.storage().instance().get(&DataKey::Transfer).unwrap();
    (status(env), transfer)
}

fn try_send(env: &Env, token: &Address, to: &MuxedAddress, amount: i128) -> i128 {
    let from = env.current_contract_address();
    match TokenClient::new(env, token).try_transfer(&from, to, &amount) {
        Ok(Ok(())) => amount,
        _ => 0,
    }
}

fn swap(env: &Env, transfer: &Transfer) -> i128 {
    let me = env.current_contract_address();
    let Some(route) = transfer.swap() else {
        return transfer.amount_in;
    };

    let settlement = TokenClient::new(env, &transfer.settlement_token);
    let before = settlement.balance(&me);

    let router = RouterClient::new(env, &route.router);
    let pair = router.router_pair_for(&route.path.get_unchecked(0), &route.path.get_unchecked(1));

    // the router pulls exactly `amount_in` of `pay_in` from here into the first pair
    env.authorize_as_current_contract(vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: transfer.pay_in.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: (me.clone(), pair, transfer.amount_in).into_val(env),
            },
            sub_invocations: vec![env],
        }),
    ]);

    router.swap_exact_tokens_for_tokens(
        &transfer.amount_in,
        &transfer.min_amount_out,
        &route.path,
        &me,
        &transfer.expires_at,
    );

    settlement.balance(&me) - before
}

#[contractimpl]
impl Kite {
    pub fn __constructor(env: Env, factory: Address, transfer: Transfer) {
        env.storage().instance().set(&DataKey::Factory, &factory);
        env.storage().instance().set(&DataKey::Transfer, &transfer);
        set_status(&env, Status::Open);
        extend(&env);
    }

    pub fn settle(env: Env) -> Result<i128, Error> {
        let (status, transfer) = enter(&env);
        if status != Status::Open {
            return Err(Error::NotOpen);
        }

        let me = env.current_contract_address();
        let pay_in = TokenClient::new(&env, &transfer.pay_in);
        let balance = pay_in.balance(&me);
        if balance < transfer.amount_in {
            return Err(Error::InsufficientPayment);
        }

        let excess = balance - transfer.amount_in;
        if excess > 0 {
            let sent = try_send(&env, &transfer.pay_in, &transfer.refund_address, excess);
            if sent > 0 {
                ExcessRefunded {
                    transfer_id: transfer.id.clone(),
                    token: transfer.pay_in.clone(),
                    to: transfer.refund_address.clone(),
                    amount: sent,
                }
                .publish(&env);
            }
        }

        let amount_out = swap(&env, &transfer);
        if amount_out < transfer.min_amount_out {
            return Err(Error::SlippageExceeded);
        }

        let settlement = TokenClient::new(&env, &transfer.settlement_token);
        for (index, fee) in transfer.fees.iter().enumerate() {
            settlement.transfer(&me, &fee.recipient, &fee.amount);
            FeePaid {
                transfer_id: transfer.id.clone(),
                index: index as u32,
                recipient: fee.recipient,
                amount: fee.amount,
            }
            .publish(&env);
        }

        let amount = amount_out - transfer.total_fees();
        settlement.transfer(&me, &transfer.settlement_recipient, &amount);
        set_status(&env, Status::Settled);

        Settled {
            transfer_id: transfer.id,
            amount_out,
            recipient: transfer.settlement_recipient,
            amount,
        }
        .publish(&env);

        Ok(amount_out)
    }

    /// Returns what the contract holds of `pay_in` to the refund address. A refund that can't be
    /// delivered still closes the transfer; the amount stays here for `recover`.
    pub fn refund(env: Env) -> Result<i128, Error> {
        let (status, transfer) = enter(&env);
        if status != Status::Open {
            return Err(Error::NotOpen);
        }

        let balance =
            TokenClient::new(&env, &transfer.pay_in).balance(&env.current_contract_address());
        if balance == 0 {
            return Err(Error::NothingToRefund);
        }

        let sent = try_send(&env, &transfer.pay_in, &transfer.refund_address, balance);
        set_status(&env, Status::Refunded);

        Refunded {
            transfer_id: transfer.id,
            token: transfer.pay_in,
            refund_address: transfer.refund_address,
            amount_refunded: sent,
        }
        .publish(&env);

        Ok(sent)
    }

    pub fn recover(env: Env, token: Address, destination: MuxedAddress) -> Result<i128, Error> {
        let (status, transfer) = enter(&env);
        if status == Status::Open
            && (token == transfer.pay_in || token == transfer.settlement_token)
        {
            return Err(Error::RecoveryNotAllowed);
        }

        let me = env.current_contract_address();
        let client = TokenClient::new(&env, &token);
        let amount = client.balance(&me);
        if amount == 0 {
            return Err(Error::NothingToRecover);
        }

        client.transfer(&me, &destination, &amount);
        Recovered {
            transfer_id: transfer.id,
            token,
            to: destination,
            amount,
        }
        .publish(&env);

        Ok(amount)
    }

    pub fn get_factory(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Factory).unwrap()
    }

    pub fn get_transfer(env: Env) -> Transfer {
        env.storage().instance().get(&DataKey::Transfer).unwrap()
    }

    pub fn get_status(env: Env) -> Status {
        status(&env)
    }
}

#[cfg(test)]
mod tests;
