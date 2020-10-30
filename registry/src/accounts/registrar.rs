use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serum_common::pack::*;
use solana_client_gen::solana_sdk::pubkey::Pubkey;

#[cfg(feature = "client")]
lazy_static::lazy_static! {
    pub static ref SIZE: u64 = Registrar::default()
                .size()
                .expect("Registrar has a fixed size");
}

/// Registry defines the account representing an instance of the program.
#[derive(Clone, Debug, Default, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Registrar {
    /// Set by the program on initialization.
    pub initialized: bool,
    /// Priviledged account.
    pub authority: Pubkey,
    /// Nonce to derive the program-derived address owning the vaults.
    pub nonce: u8,
    /// The amount of tokens that must be deposited to be eligible for rewards,
    /// denominated in SRM.
    pub reward_activation_threshold: u64,
    /// Number of seconds that must pass for a withdrawal to complete.
    pub withdrawal_timelock: i64,
    /// Number of seconds *in addition* to the withdrawal timelock it takes for
    /// an Entity account to be "deactivated", from the moment it's SRM
    /// equivalent amount drops below the required threshold.
    pub deactivation_timelock_premium: i64,
    /// Vault holding stake-intent tokens.
    pub vault: Pubkey,
    /// Vault holding stake-intent mega tokens.
    pub mega_vault: Pubkey,
    /// Address of the SRM staking pool.
    pub pool: Pubkey,
    /// Address of the MSRM staking pool.
    pub mega_pool: Pubkey,
    /// Withdrawal escrow, where funds sit during the pending withdrawal period.
    pub escrow: Escrow,
}

impl Registrar {
    pub fn deactivation_timelock(&self) -> i64 {
        self.deactivation_timelock_premium + self.withdrawal_timelock
    }
}

#[derive(Clone, Debug, Default, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Escrow {
    pub vault: Pubkey,
    pub mega_vault: Pubkey,
}

serum_common::packable!(Registrar);
