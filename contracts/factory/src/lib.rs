#![no_std]

use kite_types::{Error, KiteClient, Transfer, TTL_EXTEND_TO, TTL_THRESHOLD};
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, token::TokenClient, Address, BytesN,
    ContractExecutable, Env, MuxedAddress,
};

#[contracttype]
enum DataKey {
    Admin,
    Implementation,
    Accepted(BytesN<32>),
    Operator(Address),
}

#[contractevent(topics = ["kite", "deployed"])]
pub struct Deployed {
    pub transfer_id: BytesN<32>,
    pub address: Address,
    pub implementation: BytesN<32>,
}

#[contractevent(topics = ["kite", "implementation_set"])]
pub struct ImplementationSet {
    pub implementation: BytesN<32>,
}

#[contractevent(topics = ["kite", "operator_granted"])]
pub struct OperatorGranted {
    pub operator: Address,
}

#[contractevent(topics = ["kite", "operator_revoked"])]
pub struct OperatorRevoked {
    pub operator: Address,
}

#[contract]
pub struct Factory;

fn extend(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn require_admin(env: &Env) {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    extend(env);
}

fn require_operator(env: &Env, operator: &Address) -> Result<(), Error> {
    operator.require_auth();
    extend(env);
    if !Factory::is_operator(env.clone(), operator.clone()) {
        return Err(Error::NotOperator);
    }
    Ok(())
}

fn accept(env: &Env, implementation: &BytesN<32>) {
    env.storage()
        .instance()
        .set(&DataKey::Implementation, implementation);
    env.storage()
        .instance()
        .set(&DataKey::Accepted(implementation.clone()), &true);
    ImplementationSet {
        implementation: implementation.clone(),
    }
    .publish(env);
}

fn grant(env: &Env, operator: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::Operator(operator.clone()), &true);
    OperatorGranted {
        operator: operator.clone(),
    }
    .publish(env);
}

// The transfer's deposit contract, deployed if it isn't yet and it holds at least `minimum`
// of the pay-in token.
fn deposit<'a>(
    env: &'a Env,
    transfer: &Transfer,
    minimum: i128,
    unpaid: Error,
) -> Result<KiteClient<'a>, Error> {
    let address = Factory::predict_address(env.clone(), transfer.clone())?;
    let kite = KiteClient::new(env, &address);
    if kite.try_get_status().is_ok() {
        return Ok(kite);
    }

    if TokenClient::new(env, &transfer.pay_in).balance(&address) < minimum {
        return Err(unpaid);
    }

    env.deployer()
        .with_current_contract(transfer.salt(env))
        .deploy_contract(
            ContractExecutable::Wasm(transfer.implementation.clone()),
            (env.current_contract_address(), transfer.clone()),
        );

    Deployed {
        transfer_id: transfer.id.clone(),
        address,
        implementation: transfer.implementation.clone(),
    }
    .publish(env);

    Ok(kite)
}

#[contractimpl]
impl Factory {
    pub fn __constructor(
        env: Env,
        admin: Address,
        operator: Option<Address>,
        implementation: BytesN<32>,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        if let Some(operator) = operator {
            grant(&env, &operator);
        }
        accept(&env, &implementation);
        extend(&env);
    }

    pub fn predict_address(env: Env, transfer: Transfer) -> Result<Address, Error> {
        transfer.validate()?;
        if !Self::is_implementation(env.clone(), transfer.implementation.clone()) {
            return Err(Error::UnknownImplementation);
        }

        Ok(env
            .deployer()
            .with_current_contract(transfer.salt(&env))
            .deployed_address())
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn is_operator(env: Env, operator: Address) -> bool {
        env.storage().instance().has(&DataKey::Operator(operator))
    }

    pub fn implementation(env: Env) -> BytesN<32> {
        env.storage()
            .instance()
            .get(&DataKey::Implementation)
            .unwrap()
    }

    pub fn is_implementation(env: Env, implementation: BytesN<32>) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Accepted(implementation))
    }

    pub fn settle(env: Env, operator: Address, transfer: Transfer) -> Result<i128, Error> {
        require_operator(&env, &operator)?;
        let kite = deposit(
            &env,
            &transfer,
            transfer.amount_in,
            Error::InsufficientPayment,
        )?;
        Ok(kite.settle())
    }

    /// Before `expires_at` only an operator can refund; after it, anyone can, passing no operator.
    pub fn refund(env: Env, transfer: Transfer, operator: Option<Address>) -> Result<i128, Error> {
        match operator {
            Some(operator) => require_operator(&env, &operator)?,
            None if env.ledger().timestamp() <= transfer.expires_at => {
                return Err(Error::NotExpired)
            }
            None => extend(&env),
        }
        Ok(deposit(&env, &transfer, 1, Error::NothingToRefund)?.refund())
    }

    pub fn recover(
        env: Env,
        operator: Address,
        transfer: Transfer,
        token: Address,
        destination: Option<MuxedAddress>,
    ) -> Result<i128, Error> {
        require_operator(&env, &operator)?;
        let address = Self::predict_address(env.clone(), transfer.clone())?;
        if TokenClient::new(&env, &token).balance(&address) == 0 {
            return Err(Error::NothingToRecover);
        }

        let kite = deposit(&env, &transfer, 0, Error::NothingToRecover)?;
        Ok(kite.recover(&token, destination.unwrap_or(transfer.refund_address)))
    }

    /// Accepts a newly uploaded `kite` and uses it for new transfers. Every version accepted
    /// before stays accepted, so open transfers settle with the code they were created with.
    pub fn set_implementation(env: Env, implementation: BytesN<32>) {
        require_admin(&env);
        accept(&env, &implementation);
    }

    pub fn grant_operator(env: Env, operator: Address) {
        require_admin(&env);
        grant(&env, &operator);
    }

    pub fn revoke_operator(env: Env, operator: Address) {
        require_admin(&env);
        env.storage()
            .instance()
            .remove(&DataKey::Operator(operator.clone()));
        OperatorRevoked { operator }.publish(&env);
    }

    pub fn set_admin(env: Env, admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        require_admin(&env);
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(wasm_hash));
    }
}
