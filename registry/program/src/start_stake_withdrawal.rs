use crate::pool::{self, PoolApi, PoolConfig};
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::entity::{EntityState, StakeContext};
use serum_registry::accounts::{vault, Entity, Member, PendingWithdrawal, Registrar};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;
use spl_token::instruction as token_instruction;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    spt_amount: u64,
    mega: bool,
    delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: initiate_stake_withdrawal");

    let acc_infos = &mut accounts.iter();

    let pending_withdrawal_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let member_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let rent_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let escrow_vault_acc_info = next_account_info(acc_infos)?;
    let mega_escrow_vault_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;
    let tok_program_acc_info = next_account_info(acc_infos)?;

    let delegate_owner_acc_info = {
        if delegate {
            Some(next_account_info(acc_infos)?)
        } else {
            None
        }
    };

    // Pool accounts.
    let (stake_ctx, pool) = {
        // TODO: figure out the right config needed here.
        let cfg = PoolConfig::ReadBasket;
        pool::parse_accounts(cfg, acc_infos, mega)?
    };

    let AccessControlResponse {
        ref registrar,
        ref clock,
    } = access_control(AccessControlRequest {
        pending_withdrawal_acc_info,
        beneficiary_acc_info,
        registrar_acc_info,
        member_acc_info,
        delegate_owner_acc_info,
        entity_acc_info,
        rent_acc_info,
        clock_acc_info,
        program_id,
        delegate,
        escrow_vault_acc_info,
        mega_escrow_vault_acc_info,
        vault_authority_acc_info,
        tok_program_acc_info,
    })?;

    PendingWithdrawal::unpack_mut(
        &mut pending_withdrawal_acc_info.try_borrow_mut_data()?,
        &mut |pending_withdrawal: &mut PendingWithdrawal| {
            Entity::unpack_mut(
                &mut entity_acc_info.try_borrow_mut_data()?,
                &mut |entity: &mut Entity| {
                    Member::unpack_mut(
                        &mut member_acc_info.try_borrow_mut_data()?,
                        &mut |member: &mut Member| {
                            state_transition(StateTransitionRequest {
                                pending_withdrawal,
                                registrar,
                                member,
                                entity,
                                member_acc_info,
                                clock,
                                mega,
                                delegate,
                                spt_amount,
                                stake_ctx: &stake_ctx,
                                pool: &pool,
                                escrow_vault_acc_info,
                                mega_escrow_vault_acc_info,
                                vault_authority_acc_info,
                                tok_program_acc_info,
                            })
                            .map_err(Into::into)
                        },
                    )
                },
            )
        },
    )?;

    Ok(())
}

fn access_control(req: AccessControlRequest) -> Result<AccessControlResponse, RegistryError> {
    info!("access-control: initiate_stake_withdrawal");

    let AccessControlRequest {
        registrar_acc_info,
        pending_withdrawal_acc_info,
        beneficiary_acc_info,
        member_acc_info,
        entity_acc_info,
        delegate_owner_acc_info,
        rent_acc_info,
        clock_acc_info,
        program_id,
        delegate,
        escrow_vault_acc_info,
        mega_escrow_vault_acc_info,
        vault_authority_acc_info,
        tok_program_acc_info,
    } = req;

    // Beneficiary/delegate authorization.
    if !beneficiary_acc_info.is_signer {
        return Err(RegistryErrorCode::Unauthorized)?;
    }

    // Account validation.
    let rent = access_control::rent(rent_acc_info)?;
    let clock = access_control::clock(clock_acc_info)?;
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    let _ = access_control::entity(entity_acc_info, registrar_acc_info, program_id)?;
    let _ = access_control::member(
        member_acc_info,
        entity_acc_info,
        beneficiary_acc_info,
        delegate_owner_acc_info,
        delegate,
        program_id,
    )?;
    // TODO: check the program's escrow vault is valid.

    // StartStakeWithdrawal specific.
    {
        let pw = PendingWithdrawal::unpack(&pending_withdrawal_acc_info.try_borrow_data()?)?;
        if pending_withdrawal_acc_info.owner != program_id {
            return Err(RegistryErrorCode::InvalidAccountOwner)?;
        }
        if pw.initialized {
            return Err(RegistryErrorCode::AlreadyInitialized)?;
        }
        // TODO: this doesn't actually need to be rent exempt, since the account
        //       only needs to live during the pending withdrawal window.
        if !rent.is_exempt(
            pending_withdrawal_acc_info.lamports(),
            pending_withdrawal_acc_info.try_data_len()?,
        ) {
            return Err(RegistryErrorCode::NotRentExempt)?;
        }
        // TODO: check amount being withdraw.
    }

    info!("access-control: success");

    Ok(AccessControlResponse { registrar, clock })
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: initiate_stake_withdrawal");

    let StateTransitionRequest {
        pending_withdrawal,
        registrar,
        entity,
        member,
        member_acc_info,
        clock,
        spt_amount,
        delegate,
        mega,
        pool,
        stake_ctx,
        escrow_vault_acc_info,
        mega_escrow_vault_acc_info,
        vault_authority_acc_info,
        tok_program_acc_info,
    } = req;

    // Redeem the `spt_amount` tokens, transferring the underlying basket
    // of assets into this program's escrow vaults.
    pool.redeem(spt_amount, registrar.nonce)?;

    // The amounts that were transferred into the escrow vaults from `redeem`.
    let mut asset_amounts = stake_ctx.basket_quantities(spt_amount, mega)?;

    // Inactive entities don't receive rewards while inactive, so return the
    // excess amounts back into the pool.
    if entity.state == EntityState::Inactive {
        asset_amounts = pool_return_forfeited_assets(
            pool,
            member,
            asset_amounts,
            escrow_vault_acc_info,
            mega_escrow_vault_acc_info,
            vault_authority_acc_info,
            tok_program_acc_info,
            registrar.nonce,
            spt_amount,
            mega,
        )?;
    }

    // Balances bookeeping.
    member.spt_transfer_pending_withdrawal(spt_amount, mega, delegate);
    entity.spt_transfer_pending_withdrawal(spt_amount, mega);
    entity.transition_activation_if_needed(&stake_ctx, &registrar, &clock);

    // Print the pending withdrawal receipt.
    pending_withdrawal.initialized = true;
    pending_withdrawal.member = *member_acc_info.key;
    pending_withdrawal.start_ts = clock.unix_timestamp;
    pending_withdrawal.end_ts = clock.unix_timestamp + registrar.deactivation_timelock();
    pending_withdrawal.spt_amount = spt_amount;
    pending_withdrawal.delegate = delegate;
    pending_withdrawal.asset_amounts = asset_amounts.clone();

    info!("state-transition: success");

    Ok(())
}

// Returns the basket amount the staker should get when withdrawing from an
// inactive node entity.
//
// If the node is inactive, mark the price of the staking pool token
// to the price at the last time this member staked. Transfer any excess
// tokens back into the pool (i.e., when marking to the current price).
fn pool_return_forfeited_assets<'a, 'b, 'c>(
    pool: &'c PoolApi<'a, 'b>,
    member: &'c Member,
    current_asset_amounts: Vec<u64>,
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
    registrar_nonce: u8,
    spt_amount: u64,
    mega: bool,
) -> Result<Vec<u64>, RegistryError> {
    let last_stake_ctx = &member.last_active_stake_ctx;
    // The basket amounts the user will actually receive upon withdrawal.
    let marked_asset_amounts = last_stake_ctx.basket_quantities(spt_amount, mega)?;
    assert!(current_asset_amounts.len() == marked_asset_amounts.len());
    assert!(
        (mega && current_asset_amounts.len() == 2) || (!mega && current_asset_amounts.len() == 1)
    );
    // The basket amounts to return to the pool.
    let excess_asset_amounts: Vec<u64> = current_asset_amounts
        .iter()
        .zip(marked_asset_amounts.iter())
        .map(|(current, marked)| current - marked)
        .collect();
    assert!(
        (mega && pool.pool_asset_vault_acc_infos.len() == 2)
            || (!mega && pool.pool_asset_vault_acc_infos.len() == 1)
    );
    let signer_seeds = vault::signer_seeds(pool.registrar_acc_info.unwrap().key, &registrar_nonce);
    // Asset order matters depending on the pool.
    // SRM pool has one single SRM asset.
    // MSRM pool has two assets: MSRM, SRM, in that order.
    let (primary_escrow, secondary_escrow) = {
        if mega {
            (mega_escrow_vault_acc_info, Some(escrow_vault_acc_info))
        } else {
            (escrow_vault_acc_info, None)
        }
    };
    let transfer_instr = token_instruction::transfer(
        &spl_token::ID,
        primary_escrow.key,
        pool.pool_asset_vault_acc_infos[0].key,
        vault_authority_acc_info.key,
        &[],
        excess_asset_amounts[0],
    )?;
    solana_sdk::program::invoke_signed(
        &transfer_instr,
        &[
            primary_escrow.clone(),
            pool.pool_asset_vault_acc_infos[0].clone(),
            vault_authority_acc_info.clone(),
            tok_program_acc_info.clone(),
        ],
        &[&signer_seeds],
    )?;
    if let Some(secondary_escrow) = secondary_escrow {
        let transfer_instr = token_instruction::transfer(
            &spl_token::ID,
            secondary_escrow.key,
            pool.pool_asset_vault_acc_infos[1].key,
            vault_authority_acc_info.key,
            &[],
            excess_asset_amounts[1],
        )?;
        solana_sdk::program::invoke_signed(
            &transfer_instr,
            &[
                secondary_escrow.clone(),
                pool.pool_asset_vault_acc_infos[1].clone(),
                vault_authority_acc_info.clone(),
                tok_program_acc_info.clone(),
            ],
            &[&signer_seeds],
        )?;
    }
    Ok(marked_asset_amounts)
}

struct AccessControlRequest<'a, 'b> {
    registrar_acc_info: &'a AccountInfo<'b>,
    pending_withdrawal_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    delegate_owner_acc_info: Option<&'a AccountInfo<'b>>,
    entity_acc_info: &'a AccountInfo<'b>,
    rent_acc_info: &'a AccountInfo<'b>,
    clock_acc_info: &'a AccountInfo<'b>,
    program_id: &'a Pubkey,
    delegate: bool,
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
}

struct AccessControlResponse {
    registrar: Registrar,
    clock: Clock,
}

struct StateTransitionRequest<'a, 'b, 'c> {
    member_acc_info: &'a AccountInfo<'b>,
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
    pending_withdrawal: &'c mut PendingWithdrawal,
    pool: &'c PoolApi<'a, 'b>,
    entity: &'c mut Entity,
    member: &'c mut Member,
    registrar: &'c Registrar,
    stake_ctx: &'c StakeContext,
    clock: &'c Clock,
    spt_amount: u64,
    delegate: bool,
    mega: bool,
}
