use crate::access_control;
use crate::accounts::Registrar;
use crate::error::RegistryError;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use num_enum::IntoPrimitive;
use serum_common::pack::*;
use serum_pool_schema::Basket;
use solana_client_gen::solana_sdk::account_info::AccountInfo;
use solana_client_gen::solana_sdk::pubkey::Pubkey;
use solana_client_gen::solana_sdk::sysvar::clock::Clock;
use std::convert::Into;

#[cfg(feature = "client")]
lazy_static::lazy_static! {
    pub static ref SIZE: u64 = Entity::default()
                .size()
                .expect("Entity has a fixed size");
}

/// Entity is the account representing a single "node" that addresses can
/// stake with.
#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Entity {
    /// Set when this entity is registered with the program.
    pub initialized: bool,
    /// The registrar to which this Member belongs.
    pub registrar: Pubkey,
    /// Leader of the entity, i.e., the one responsible for fulfilling node
    /// duties.
    pub leader: Pubkey,
    /// Bitmap representing this entity's capabilities .
    pub capabilities: u32,
    /// Type of stake backing this entity (determines voting rights)
    /// of the stakers.
    pub stake_kind: StakeKind,
    /// Cumulative stake balances from all member accounts.
    pub balances: Balances,
    /// The activation generation number, incremented whenever EntityState
    /// transitions froom `Inactive` -> `Active`.
    pub generation: u64,
    /// State of the Entity. See the `EntityState` comments.
    pub state: EntityState,
}

// StakeContext represents the current state of the staking pool.
//
// Each Basket represents an exchange ratio of *1* staking pool token
// for the basket of underlying assets.
#[derive(Clone)]
pub struct StakeContext {
    // `basket` has as single quantity representing SRM.
    basket: Basket,
    // `mega_basket` has two quantities: MSRM and SRM.
    mega_basket: Basket,
}

impl StakeContext {
    pub fn new(basket: Basket, mega_basket: Basket) -> Self {
        Self {
            basket,
            mega_basket,
        }
    }
    pub fn basket_value(&self, spt_count: u64) -> u64 {
        assert!(self.basket.quantities.len() == 1);
        spt_count * self.basket.quantities[0] as u64
    }

    pub fn mega_basket_value(&self, mega_spt_count: u64) -> u64 {
        assert!(self.mega_basket.quantities.len() == 2);
        mega_spt_count * self.mega_basket.quantities[0] as u64 * 1_000_000
            + mega_spt_count * self.mega_basket.quantities[1] as u64
    }
}

// Public methods.
impl Entity {
    /// Returns the amount of stake contributing to the activation level.
    pub fn activation_amount(&self, ctx: &StakeContext) -> u64 {
        self.amount_equivalent(ctx) + self.stake_intent_equivalent()
    }

    /// Adds to the stake intent balance.
    pub fn add_stake_intent(&mut self, amount: u64, mega: bool) {
        if mega {
            self.balances.mega_stake_intent += amount;
        } else {
            self.balances.stake_intent += amount;
        }
    }

    /// Subtracts from the stake intent balance.
    pub fn sub_stake_intent(&mut self, amount: u64, mega: bool) {
        if mega {
            self.balances.mega_stake_intent -= amount;
        } else {
            self.balances.stake_intent -= amount;
        }
    }

    /// Adds to the stake balance.
    pub fn spt_add(&mut self, amount: u64, is_mega: bool) {
        if is_mega {
            self.balances.mega_stake_intent += amount;
        } else {
            self.balances.stake_intent += amount;
        }
    }

    /// Moves stake into the pending wtihdrawal state.
    pub fn spt_transfer_pending_withdrawal(&mut self, amount: u64, mega: bool) {
        if mega {
            self.balances.spt_mega_amount -= amount;
            self.balances.spt_mega_pending_withdrawals += amount;
        } else {
            self.balances.spt_amount -= amount;
            self.balances.spt_pending_withdrawals += amount;
        }
    }

    /// Transitions the EntityState finite state machine. This should be called
    /// immediately before processing any instruction relying on the most up
    /// to date status of the EntityState. It should also be called after any
    /// mutation to the SRM equivalent deposit of this entity to keep the state
    /// up to date.
    pub fn transition_activation_if_needed(
        &mut self,
        ctx: &StakeContext,
        registrar: &Registrar,
        clock: &Clock,
    ) {
        match self.state {
            EntityState::Inactive => {
                if self.meets_activation_requirements(ctx, registrar) {
                    self.state = EntityState::Active;
                    self.generation += 1;
                }
            }
            EntityState::PendingDeactivation {
                deactivation_start_ts,
            } => {
                if clock.unix_timestamp > deactivation_start_ts + registrar.deactivation_timelock()
                {
                    self.state = EntityState::Inactive;
                }
            }
            EntityState::Active => {
                if !self.meets_activation_requirements(ctx, registrar) {
                    self.state = EntityState::PendingDeactivation {
                        deactivation_start_ts: clock.unix_timestamp,
                    }
                }
            }
        }
    }

    /// Returns true if this Entity is capable of being "activated", i.e., can
    /// enter the staking pool.
    pub fn meets_activation_requirements(&self, ctx: &StakeContext, registrar: &Registrar) -> bool {
        self.activation_amount(ctx) >= registrar.reward_activation_threshold
            && self.balances.spt_mega_amount >= 1
    }
}

// Private methods.
impl Entity {
    fn amount_equivalent(&self, ctx: &StakeContext) -> u64 {
        ctx.basket_value(self.balances.spt_amount)
            + ctx.mega_basket_value(self.balances.spt_mega_amount)
    }
    fn stake_intent_equivalent(&self) -> u64 {
        self.balances.stake_intent + self.balances.mega_stake_intent * 1_000_000
    }
}

serum_common::packable!(Entity);

#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Balances {
    pub spt_amount: u64,
    pub spt_mega_amount: u64,
    pub spt_pending_withdrawals: u64,
    pub spt_mega_pending_withdrawals: u64,
    pub stake_intent: u64,
    pub mega_stake_intent: u64,
}

/// EntityState defines a finite-state-machine (FSM) determining the actions
/// a `Member` account can take with respect to staking an Entity and receiving
/// rewards.
///
/// FSM:
///
/// Inactive -> Active:
///  * Entity `generation` count gets incremented and Members may stake.
/// Active -> PendingDeactivation:
///  * Staking ceases and Member accounts should withdraw or add more
///    stake-intent.
/// PendingDeactivation -> Active:
///  * New stake is accepted and rewards continue.
/// PendingDeactivation -> Inactive:
///  * Stake not withdrawn will not receive accrued rewards (just original
///    deposit). If the Entity becomes active again, Members with deposits
///    from old "generations" must withdraw their entire deposit, before being
///    allowed to stake again.
///
#[derive(Debug, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq)]
pub enum EntityState {
    /// The entity is ineligble for rewards. Redeeming existing staking pool
    /// tokens will return less than or equal to the original staking deposit.
    Inactive,
    /// The Entity is on a deactivation countdown, lasting until the timestamp
    /// `deactivation_start_ts + Registrar.deactivation_timelock_premium`,
    /// at which point the EntityState transitions from PendingDeactivation
    /// to Inactive.
    ///
    /// During this time, either members  must stake more SRM or MSRM or they
    /// should withdraw their stake to retrieve their rewards.
    PendingDeactivation { deactivation_start_ts: i64 },
    /// The entity is eligble for rewards. Member accounts can stake with this
    /// entity and receive rewards.
    Active,
}

impl Default for EntityState {
    fn default() -> Self {
        Self::Inactive
    }
}

#[derive(
    Debug, PartialEq, IntoPrimitive, Clone, Copy, BorshSerialize, BorshDeserialize, BorshSchema,
)]
#[repr(u32)]
pub enum StakeKind {
    Voting,
    Delegated,
}

impl Default for StakeKind {
    fn default() -> Self {
        StakeKind::Delegated
    }
}
