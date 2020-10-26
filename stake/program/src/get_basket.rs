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

pub fn handler(
    ctx: &PoolContext,
    state: &PoolState,
    spt_amount: u64,
) -> Result<Basket, StakeError> {
    let basket = {
        if ctx.total_pool_tokens()? == 0 {
            Basket {
                quantities: vec![spt_amount as i64],
            }
        } else {
            ctx.get_simple_basket(spt_amount)?
        }
    };

    let retbuf_accs = ctx.retbuf.as_ref().expect("must have retbuf accounts");
    let offset: usize = 0;
    let mut data = offset.to_le_bytes().to_vec();
    data.append(&mut basket.try_to_vec().expect("basket must serialize"));

    let instr = Instruction {
        program_id: *retbuf_accs.program.key,
        accounts: vec![AccountMeta::new(*retbuf_accs.account.key, false)],
        data,
    };
    info!("invoking retbuf");
    solana_sdk::program::invoke(
        &instr,
        &[retbuf_accs.account.clone(), retbuf_accs.program.clone()],
    )?;
    info!("retbuf success");
    Ok(basket)
}
