use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serum_common::pack::Pack;
use serum_common::pack::*;
use solana_client_gen::solana_sdk::pubkey::Pubkey;

#[cfg(feature = "client")]
lazy_static::lazy_static! {
    pub static ref SIZE: u64 = PendingWithdrawal::default()
                .size()
                .expect("Vesting has a fixed size");
}

/// PendingWithdrawal accounts are created to initiate a withdrawal.
/// Once the `end_ts` passes, the PendingWithdrawal can be burned in exchange
/// for the specified withdrawal amount.
#[derive(Debug, Default, BorshSerialize, BorshDeserialize, BorshSchema)]
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
    /// Payment to be sent to the member account's main owner.
    pub payment: PendingPayment,
    /// Payment to be sent to the member account's delegate owner.
    pub delegate_payment: PendingPayment,
}

#[derive(Debug, Default, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PendingPayment {
    /// The amount of the underlying asset to be received (SRM).
    pub asset_amount: u64,
    /// The amount of the underlying mega asset to be received (MSRM).
    pub mega_asset_amount: u64,
    /// SRM SPL token account.
    pub recipient: Pubkey,
    /// MSRM SPL token account.
    pub mega_recipient: Pubkey,
}

serum_common::packable!(PendingWithdrawal);
