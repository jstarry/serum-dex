use serum_pool::context::{PoolContext, UserAccounts};
use serum_pool_schema::{Basket, PoolState};
use serum_stake::accounts::vault;
use serum_stake::error::{StakeError, StakeErrorCode};
use solana_program::info;
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
        pool_token_account, // Owned by Registry's member beneficiary.
        asset_accounts,     // Owned by `authority`.
        authority,          // Registry's program-derived address.
    } = ctx
        .user_accounts
        .as_ref()
        .expect("transact requests have user accounts");

    assert!(ctx.custom_accounts.len() == 1);
    assert!(asset_accounts.len() == 1 || asset_accounts.len() == 2);
    assert!(ctx.pool_vault_accounts.len() == asset_accounts.len());

    // Registry authorization.
    let registry_signer_acc_info = &ctx.custom_accounts[0];
    if !registry_signer_acc_info.is_signer {
        return Err(StakeErrorCode::Unauthorized)?;
    }
    let expected_admin: Pubkey = state.admin_key.clone().expect("must have admin key").into();
    if expected_admin != *registry_signer_acc_info.key {
        return Err(StakeErrorCode::Unauthorized)?;
    }

    // Quantities needed to create the `spt_amount` of staking pool tokens.
    let basket = {
        if ctx.total_pool_tokens()? == 0 {
            if asset_accounts.len() == 1 {
                Basket {
                    quantities: vec![spt_amount
                        .try_into()
                        .map_err(|_| StakeErrorCode::FailedCast)?],
                }
            } else {
                Basket {
                    quantities: vec![
                        0,
                        spt_amount
                            .try_into()
                            .map_err(|_| StakeErrorCode::FailedCast)?,
                    ],
                }
            }
        } else {
            ctx.get_simple_basket(spt_amount)?
        }
    };

    // Sign all CPI invocations, in the event that any of the token
    // transfers into the vault are (optionally) delegate transfers, where
    // this program is the approved delegate.
    let signer_seeds = vault::signer_seeds(ctx.pool_account.key, &state.vault_signer_nonce);

    // Transfer the SRM *into* the pool.
    {
        let user_token_acc_info = &asset_accounts[0];
        let pool_token_vault_acc_info = &ctx.pool_vault_accounts[0];
        let asset_amount = basket.quantities[0]
            .try_into()
            .map_err(|_| StakeErrorCode::FailedCast)?;
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
            &[&signer_seeds],
        )?;
    }

    // Transfer the MSRM *into* the pool, if this is indeed the MSRM pool.
    if asset_accounts.len() == 2 {
        let user_token_acc_info = &asset_accounts[1];
        let pool_token_vault_acc_info = &ctx.pool_vault_accounts[1];
        let asset_amount = basket.quantities[1]
            .try_into()
            .map_err(|_| StakeErrorCode::FailedCast)?;
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
            &[&signer_seeds],
        )?;
    }

    // Mint `spt_amount` of staking pool tokens to the Registry's beneficiary.
    {
        let mint_tokens_instr = token_instruction::mint_to(
            &spl_token::ID,
            ctx.pool_token_mint.key,
            pool_token_account.key,
            ctx.pool_authority.key,
            &[],
            spt_amount,
        )?;
        solana_sdk::program::invoke_signed(
            &mint_tokens_instr,
            &[
                ctx.pool_token_mint.clone(),
                pool_token_account.clone(),
                ctx.pool_authority.clone(),
                ctx.spl_token_program.expect("must be provided").clone(),
            ],
            &[&signer_seeds],
        )?;
    }

    Ok(())
}
