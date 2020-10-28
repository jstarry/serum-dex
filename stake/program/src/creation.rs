use serum_pool::context::{PoolContext, UserAccounts};
use serum_pool_schema::{Basket, PoolState};
use serum_stake::error::{StakeError, StakeErrorCode};
use solana_program::info;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::program_error::ProgramError;
use solana_sdk::pubkey::Pubkey;
use spl_token::instruction as token_instruction;
use std::convert::TryInto;

pub fn handler(
    ctx: &PoolContext,
    state: &mut PoolState,
    spt_amount: u64,
) -> Result<(), StakeError> {
    info!("handler: creation");
    let &UserAccounts {
        pool_token_account,
        asset_accounts,
        authority,
    } = ctx
        .user_accounts
        .as_ref()
        .expect("transact requests have user accounts");

    // TODO: add a custom account to represent an authority over
    //       the tokent ransfer (for whitelist withdrawals).

    // Registry authorization.
    assert!(ctx.custom_accounts.len() == 1);
    let admin_acc_info = &ctx.custom_accounts[0];
    if !admin_acc_info.is_signer {
        return Err(StakeErrorCode::Unauthorized)?;
    }
    let expected_admin: Pubkey = state.admin_key.clone().expect("had admin key").into();
    if expected_admin != *admin_acc_info.key {
        return Err(StakeErrorCode::Unauthorized)?;
    }

    // TODO: this will fail for the MSRM pool.
    assert!(asset_accounts.len() == 1);
    assert!(ctx.pool_vault_accounts.len() == 1);
    let user_token_acc_info = &asset_accounts[0];
    let pool_token_vault_acc_info = &ctx.pool_vault_accounts[0];

    let asset_amount: u64 = {
        if ctx.total_pool_tokens()? == 0 {
            spt_amount
        } else {
            let Basket { quantities } = ctx.get_simple_basket(spt_amount)?;
            quantities[0]
                .try_into()
                .map_err(|_| StakeErrorCode::InvalidU64)?
        }
    };

    // Transfer `amount` of the state.assets[0] into the pool's vault.
    {
        info!("invoking token transfer");
        let transfer_instr = token_instruction::transfer(
            &spl_token::ID,
            user_token_acc_info.key,
            pool_token_vault_acc_info.key,
            authority.key,
            &[],
            asset_amount,
        )?;
        solana_sdk::program::invoke_signed(
            &transfer_instr,
            &[
                user_token_acc_info.clone(),
                pool_token_vault_acc_info.clone(),
                authority.clone(),
                ctx.spl_token_program.expect("must be provided").clone(),
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
            asset_amount,
        )?;
        solana_sdk::program::invoke_signed(
            &mint_tokens_instr,
            &[
                ctx.pool_token_mint.clone(),
                pool_token_account.clone(),
                ctx.pool_authority.clone(),
                ctx.spl_token_program.expect("must be provided").clone(),
            ],
            &[&[&[state.vault_signer_nonce]]],
        )?;
    }

    Ok(())
}
