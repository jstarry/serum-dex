use serum_pool::context::{PoolContext, UserAccounts};
use serum_pool_schema::{Basket, PoolState};
use serum_stake::error::{StakeError, StakeErrorCode};
use solana_sdk::account_info::AccountInfo;
use solana_sdk::program_error::ProgramError;
use spl_token::instruction as token_instruction;
use std::convert::TryInto;

pub fn handler(
    ctx: &PoolContext,
    state: &mut PoolState,
    spt_amount: u64,
) -> Result<(), StakeError> {
    let &UserAccounts {
        pool_token_account,
        asset_accounts,
        authority,
    } = ctx
        .user_accounts
        .as_ref()
        .expect("transact requests have user accounts");

    assert!(ctx.custom_accounts.len() == 1);
    let admin_acc_info = &ctx.custom_accounts[0];
    if !admin_acc_info.is_signer {
        return Err(StakeErrorCode::Unauthorized)?;
    }

    assert!(asset_accounts.len() == 1);
    assert!(ctx.pool_vault_accounts.len() == 1);
    let user_token_acc_info = &asset_accounts[0];
    let pool_token_vault_acc_info = &ctx.pool_vault_accounts[0];

    let Basket { quantities } = ctx.get_simple_basket(spt_amount)?;
    let asset_amount = quantities[0];

    // Transfer `amount` of the state.assets[0] into the pool's vault.
    {
        let transfer_instr = token_instruction::transfer(
            &spl_token::ID,
            user_token_acc_info.key,
            pool_token_vault_acc_info.key,
            ctx.pool_authority.key,
            &[],
            asset_amount
                .try_into()
                .map_err(|_| StakeErrorCode::InvalidU64)?,
        )?;
        solana_sdk::program::invoke_signed(
            &transfer_instr,
            &[
                user_token_acc_info.clone(),
                pool_token_vault_acc_info.clone(),
                ctx.pool_authority.clone(),
            ],
            &[],
        )?;
    }

    // Mint `amount` of state.pool_token_mint.
    {
        let mint_tokens_instr = token_instruction::mint_to(
            &spl_token::ID,
            ctx.pool_token_mint.key,
            pool_token_account.key,
            ctx.pool_authority.key,
            &[],
            asset_amount
                .try_into()
                .map_err(|_| StakeErrorCode::InvalidU64)?,
        )?;
        solana_sdk::program::invoke_signed(
            &mint_tokens_instr,
            &[
                ctx.pool_token_mint.clone(),
                pool_token_account.clone(),
                ctx.pool_authority.clone(),
            ],
            &[&[&[state.vault_signer_nonce]]],
        )?;
    }

    Ok(())
}
