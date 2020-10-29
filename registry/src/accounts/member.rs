use crate::accounts::entity::StakeContext;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serum_common::pack::*;
use solana_client_gen::solana_sdk::pubkey::Pubkey;

#[cfg(feature = "client")]
lazy_static::lazy_static! {
    pub static ref SIZE: u64 = Member::default()
                .size()
                .expect("Member has a fixed size");
}

/// Member account tracks membership with a node `Entity`.
#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Member {
    /// Set by the program on creation.
    pub initialized: bool,
    /// Registrar the member belongs to.
    pub registrar: Pubkey,
    /// Entity account providing membership.
    pub entity: Pubkey,
    /// The key that is allowed to redeem assets from the staking pool.
    pub beneficiary: Pubkey,
    /// The entity's activation counter to which the stake belongs.
    pub generation: u64,
    /// The Watchtower that can withdraw the `Member` account's `main` `Book`.
    pub watchtower: Watchtower,
    /// The balance subbaccounts that partition the Member's stake balance.
    pub books: MemberBooks,
    /// The *last* stake context used when creating a staking pool token.
    /// This is used to mark the price of a staking pool token to its underlying
    /// basket, when a withdrawal on an inactive entity happens.
    ///
    /// Marking the price this ways relies on the fact that the price of
    /// a staking pool token can only go up (since the underlying basket can't
    /// be removed or destroyed without redeeming a staking pool token).
    ///
    /// Additionally, it implies that withdrawing from the staking pool on
    /// an inactive entity *might* yield less of the underlying asset than
    /// if a withdrawal happens on an active entity (since rewards might have
    /// been dropped on the staking pool after this member deposited, and
    /// before the entity became inactive, pushing the price up.)
    pub last_active_stake_ctx: Option<StakeContext>,
}

impl Member {
    pub fn stake_intent(&self, mega: bool, delegate: bool) -> u64 {
        if delegate {
            if mega {
                self.books.delegate.balances.mega_stake_intent
            } else {
                self.books.delegate.balances.stake_intent
            }
        } else {
            if mega {
                self.books.main.balances.mega_stake_intent
            } else {
                self.books.main.balances.stake_intent
            }
        }
    }
    pub fn add_stake_intent(&mut self, amount: u64, mega: bool, delegate: bool) {
        if delegate {
            if mega {
                self.books.delegate.balances.mega_stake_intent += amount;
            } else {
                self.books.delegate.balances.stake_intent += amount;
            }
        } else {
            if mega {
                self.books.main.balances.mega_stake_intent += amount;
            } else {
                self.books.main.balances.stake_intent += amount;
            }
        }
    }
    pub fn sub_stake_intent(&mut self, amount: u64, mega: bool, delegate: bool) {
        if delegate {
            if mega {
                self.books.delegate.balances.mega_stake_intent -= amount;
            } else {
                self.books.delegate.balances.stake_intent -= amount;
            }
        } else {
            if mega {
                self.books.main.balances.mega_stake_intent -= amount;
            } else {
                self.books.main.balances.stake_intent -= amount;
            }
        }
    }
    pub fn spt_add(&mut self, stake_ctx: &StakeContext, amount: u64, mega: bool, delegate: bool) {
        if delegate {
            if mega {
                self.books.delegate.balances.spt_mega_amount += amount;
            } else {
                self.books.delegate.balances.spt_amount += amount;
            }
        } else {
            if mega {
                self.books.main.balances.spt_mega_amount += amount;
            } else {
                self.books.main.balances.spt_amount += amount;
            }
        }
        self.last_active_stake_ctx = Some(stake_ctx.clone());
    }
    pub fn spt_transfer_pending_withdrawal(&mut self, spt_amount: u64, mega: bool, delegate: bool) {
        if delegate {
            if mega {
                self.books.delegate.balances.spt_mega_amount -= spt_amount;
                self.books.delegate.balances.spt_mega_pending_withdrawals += spt_amount;
            } else {
                self.books.delegate.balances.spt_amount -= spt_amount;
                self.books.delegate.balances.spt_pending_withdrawals += spt_amount;
            }
        } else {
            if mega {
                self.books.main.balances.spt_mega_amount -= spt_amount;
                self.books.main.balances.spt_mega_pending_withdrawals += spt_amount;
            } else {
                self.books.main.balances.spt_amount -= spt_amount;
                self.books.main.balances.spt_pending_withdrawals += spt_amount;
            }
        }
    }
    pub fn stake_is_empty(&self) -> bool {
        self.books.main.balances.spt_amount != 0
            || self.books.main.balances.spt_mega_amount != 0
            || self.books.delegate.balances.spt_amount != 0
            || self.books.delegate.balances.spt_mega_amount != 0
    }
    pub fn set_delegate(&mut self, delegate: Pubkey) {
        assert!(self.books.delegate.balances.spt_amount == 0);
        self.books.delegate = Book {
            owner: delegate,
            balances: Default::default(),
        };
    }
}

/// Watchtower defines an (optional) authority that can update a Member account
/// on behalf of the `beneficiary`.
#[derive(Default, Clone, Copy, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Watchtower {
    /// The signing key that can withdraw stake from this Member account in
    /// the case of a pending deactivation.
    authority: Pubkey,
    /// The destination *token* address the staked funds are sent to in the
    /// case of a withdrawal by a watchtower.
    ///
    /// Note that a watchtower can only withdraw deposits *not* sent from a
    /// delegate. Withdrawing more will result in tx failure.
    ///
    /// For all delegated funds, the watchtower should follow the protocol
    /// defined by the delegate.
    ///
    /// In the case of locked SRM, this means invoking the `WhitelistDeposit`
    /// instruction on the Serum Lockup program to transfer funds from the
    /// Registry back into the Lockup.
    dst: Pubkey,
}

impl Watchtower {
    pub fn new(authority: Pubkey, dst: Pubkey) -> Self {
        Self { authority, dst }
    }
}

#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct MemberBooks {
    main: Book,
    /// Delegate authorized to deposit or withdraw from the staking pool
    /// on behalf of the beneficiary. Although these funds are part of the
    /// Member account, they are not directly accessible by the beneficiary.
    /// All transactions affecting the delegate must be signed by *both* the
    /// `delegate` and the `beneficiary`.
    ///
    /// The only expected use case as of now is the Lockup program.
    delegate: Book,
}

impl MemberBooks {
    pub fn new(beneficiary: Pubkey, delegate: Pubkey) -> Self {
        Self {
            main: Book {
                owner: beneficiary,
                balances: Default::default(),
            },
            delegate: Book {
                owner: delegate,
                balances: Default::default(),
            },
        }
    }

    pub fn delegate(&self) -> &Book {
        &self.delegate
    }

    pub fn main(&self) -> &Book {
        &self.main
    }
}

#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Book {
    pub owner: Pubkey,
    pub balances: Balances,
}

#[derive(Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Balances {
    // Denominated in staking pool tokens (spt).
    pub spt_amount: u64,
    pub spt_mega_amount: u64,
    pub spt_pending_withdrawals: u64,
    pub spt_mega_pending_withdrawals: u64,
    // Denominated in SRM/MSRM.
    pub stake_intent: u64,
    pub mega_stake_intent: u64,
}

impl Balances {
    pub fn is_empty(&self) -> bool {
        self.spt_amount
            + self.spt_mega_amount
            + self.spt_pending_withdrawals
            + self.spt_mega_pending_withdrawals
            + self.stake_intent
            + self.mega_stake_intent
            == 0
    }
}

serum_common::packable!(Member);
