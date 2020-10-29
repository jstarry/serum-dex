use crate::accounts::{vault, Entity, Member, Registrar};
use crate::error::RegistryError;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serum_common::pack::Pack;
use serum_common::pack::*;
use solana_client_gen::solana_sdk::pubkey::Pubkey;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::sysvar::clock::Clock;

/// PendingWithdrawal accounts are created to initiate a withdrawal.
/// Once the `end_ts` passes, the PendingWithdrawal can be burned in exchange
/// for the specified withdrawal amount.
#[derive(Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PendingWithdrawal {
    pub initialized: bool,
    /// One time token. True if the withdrawal has been completed.
    pub burned: bool,
    /// True if the delegate of the member account initiated the withdrawal.
    pub delegate: bool,
    /// Member this account belongs to.
    pub member: Pubkey,
    /// Unix timestamp when this account was initialized.
    pub start_ts: i64,
    /// Timestamp when the pending withdrawal completes.
    pub end_ts: i64,
    /// The number of staking pool tokens redeemed.
    pub spt_amount: u64,
    /// The pool being withdrawn from.
    pub pool: Pubkey,
    /// The amounts of the underlying assets to be received from this
    /// withdrawal. The ordering of each element here corresponds to
    /// the odrdering defined in `serum_pool_schema::PoolState::assets`.
    pub asset_amounts: Vec<u64>,
}

serum_common::packable!(PendingWithdrawal);
