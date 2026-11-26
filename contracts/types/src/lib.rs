#![no_std]

use soroban_sdk::{
    contractclient, contracterror, contracttype, xdr::ToXdr, Address, BytesN, Env, MuxedAddress,
    Vec,
};

pub const MAX_FEES: u32 = 10;
pub const MAX_PATH: u32 = 4;

pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
pub const TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotOperator = 1,
    InvalidId = 2,
    InvalidAmount = 3,
    TooManyFees = 4,
    InvalidFee = 5,
    FeesExceedMinAmountOut = 6,
    InvalidSwap = 7,
    UnknownImplementation = 8,
    NotOpen = 9,
    InsufficientPayment = 10,
    NothingToRefund = 11,
    NotExpired = 12,
    RecoveryNotAllowed = 13,
    SlippageExceeded = 14,
    NothingToRecover = 15,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fee {
    pub recipient: MuxedAddress,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Swap {
    pub router: Address,
    pub path: Vec<Address>,
}

/// `Option<Swap>` as an enum: soroban-sdk can't derive the test conversions for an
/// `Option` of a struct inside a struct.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SwapRoute {
    None,
    Some(Swap),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    pub id: BytesN<32>,
    pub pay_in: Address,
    pub amount_in: i128,
    pub settlement_token: Address,
    pub settlement_recipient: MuxedAddress,
    pub min_amount_out: i128,
    pub fees: Vec<Fee>,
    pub refund_address: MuxedAddress,
    pub expires_at: u64,
    pub swap: SwapRoute,
    pub implementation: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Open,
    Settled,
    Refunded,
}

impl Transfer {
    pub fn needs_swap(&self) -> bool {
        self.pay_in != self.settlement_token
    }

    pub fn swap(&self) -> Option<Swap> {
        match &self.swap {
            SwapRoute::None => None,
            SwapRoute::Some(swap) => Some(swap.clone()),
        }
    }

    pub fn total_fees(&self) -> i128 {
        self.fees.iter().map(|f| f.amount).sum()
    }

    /// The deploy salt: sha256 of the transfer's `ScVal` XDR.
    pub fn salt(&self, env: &Env) -> BytesN<32> {
        env.crypto().sha256(&self.clone().to_xdr(env)).into()
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.id.to_array() == [0; 32] {
            return Err(Error::InvalidId);
        }

        if self.amount_in <= 0 || self.min_amount_out <= 0 {
            return Err(Error::InvalidAmount);
        }

        if self.fees.len() > MAX_FEES {
            return Err(Error::TooManyFees);
        }

        let mut total: i128 = 0;
        for fee in self.fees.iter() {
            if fee.amount <= 0 {
                return Err(Error::InvalidFee);
            }
            total = total.checked_add(fee.amount).ok_or(Error::InvalidFee)?;
        }

        if total > self.min_amount_out {
            return Err(Error::FeesExceedMinAmountOut);
        }

        match (self.swap(), self.needs_swap()) {
            (None, false) => Ok(()),
            (Some(swap), true) => {
                if !(2..=MAX_PATH).contains(&swap.path.len())
                    || swap.path.first() != Some(self.pay_in.clone())
                    || swap.path.last() != Some(self.settlement_token.clone())
                {
                    return Err(Error::InvalidSwap);
                }
                Ok(())
            }
            _ => Err(Error::InvalidSwap),
        }
    }
}

/// The `kite` contract, as the factory calls it.
#[contractclient(name = "KiteClient")]
pub trait KiteInterface {
    fn settle(env: Env) -> i128;
    fn refund(env: Env) -> i128;
    fn recover(env: Env, token: Address, destination: MuxedAddress) -> i128;
    fn get_status(env: Env) -> Status;
}

#[cfg(test)]
mod vector;
