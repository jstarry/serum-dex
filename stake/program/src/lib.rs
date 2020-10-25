//! Program entrypoint.

#![cfg_attr(feature = "strict", deny(warnings))]

use serum_common::pack::Pack;
use serum_pool::context::PoolContext;
use serum_pool::pool::Pool;
use serum_pool_schema::PoolState;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::entrypoint::ProgramResult;
use solana_sdk::info;
use solana_sdk::program_error::ProgramError;
use solana_sdk::pubkey::Pubkey;

mod creation;
mod initialize_pool;
mod redemption;

struct StakeProgram;

impl Pool for StakeProgram {
    fn initialize_pool(ctx: &PoolContext, state: &mut PoolState) -> Result<(), ProgramError> {
        initialize_pool::handler(ctx, state).map_err(Into::into)
    }

    fn process_creation(
        ctx: &PoolContext,
        state: &mut PoolState,
        spt_amount: u64,
    ) -> Result<(), ProgramError> {
        creation::handler(ctx, state, spt_amount).map_err(Into::into)
    }

    fn process_redemption(
        ctx: &PoolContext,
        state: &mut PoolState,
        spt_amount: u64,
    ) -> Result<(), ProgramError> {
        redemption::handler(ctx, state, spt_amount).map_err(Into::into)
    }
}

serum_pool::declare_pool_entrypoint!(StakeProgram);
