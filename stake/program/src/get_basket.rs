use borsh::BorshSerialize;
use serum_pool::context::{PoolContext, UserAccounts};
use serum_pool_schema::{Basket, PoolState};
use serum_stake::error::{StakeError, StakeErrorCode};
use solana_sdk::account_info::AccountInfo;
use solana_sdk::info;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::program_error::ProgramError;
use solana_sdk::pubkey::Pubkey;
use spl_token::instruction as token_instruction;
use std::convert::TryInto;

pub fn handler(ctx: &PoolContext, state: &PoolState, request: u64) -> Result<Basket, StakeError> {
    let basket = ctx.get_simple_basket(request)?;

    let retbuf_accs = ctx.retbuf.as_ref().expect("must have retbuf accounts");

    let instr = Instruction {
        program_id: *retbuf_accs.program.key,
        accounts: vec![AccountMeta::new(*retbuf_accs.account.key, false)],
        data: basket.try_to_vec().expect("basket must serialize"),
    };
    solana_sdk::program::invoke(
        &instr,
        &[retbuf_accs.account.clone(), retbuf_accs.program.clone()],
    );

    Ok(basket)
}
