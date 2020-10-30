use crate::common::invoke_token_transfer;
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::{vault, Entity, Member, PendingWithdrawal, Registrar};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use std::convert::Into;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: end_stake_withdrawl");

    let acc_infos = &mut accounts.iter();

    // Lockup whitelist relay interface.
    let delegate_owner_acc_info = next_account_info(acc_infos)?;
    let _user_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;
    let tok_program_acc_info = next_account_info(acc_infos)?;

    // Program specific.
    let pending_withdrawal_acc_info = next_account_info(acc_infos)?;
    let escrow_vault_acc_info = next_account_info(acc_infos)?;
    let mega_escrow_vault_acc_info = next_account_info(acc_infos)?;

    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;

    let user_acc_info = next_account_info(acc_infos)?;
    let user_mega_acc_info = next_account_info(acc_infos)?;
    let mut user_delegate_acc_info = None;
    let mut user_delegate_mega_acc_info = None;
    if delegate {
        user_delegate_acc_info = Some(next_account_info(acc_infos)?);
        user_delegate_mega_acc_info = Some(next_account_info(acc_infos)?);
    }

    let AccessControlResponse { ref registrar } = access_control(AccessControlRequest {
        registrar_acc_info,
        pending_withdrawal_acc_info,
        beneficiary_acc_info,
        member_acc_info,
        entity_acc_info,
        delegate_owner_acc_info,
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
                                user_acc_info,
                                user_mega_acc_info,
                                user_delegate_acc_info,
                                user_delegate_mega_acc_info,
                                vault_authority_acc_info,
                                tok_program_acc_info,
                                registrar,
                                registrar_acc_info,
                                escrow_vault_acc_info,
                                mega_escrow_vault_acc_info,
                                entity,
                                member,
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

#[inline(always)]
fn access_control(req: AccessControlRequest) -> Result<AccessControlResponse, RegistryError> {
    info!("access-control: end_stake_withdrawal");

    let AccessControlRequest {
        registrar_acc_info,
        pending_withdrawal_acc_info,
        beneficiary_acc_info,
        member_acc_info,
        entity_acc_info,
        delegate_owner_acc_info,
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

    // TODO: check delegate and destination addresses.

    // Account validation.
    let clock = access_control::clock(clock_acc_info)?;
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    let _ = access_control::entity(entity_acc_info, registrar_acc_info, program_id)?;
    let _ = access_control::member(
        member_acc_info,
        entity_acc_info,
        beneficiary_acc_info,
        Some(delegate_owner_acc_info),
        delegate,
        program_id,
    )?;
    let pending_withdrawal =
        access_control::pending_withdrawal(pending_withdrawal_acc_info, program_id)?;

    // EndStakeWithdrawal specific.
    {
        if clock.unix_timestamp < pending_withdrawal.end_ts {
            return Err(RegistryErrorCode::WithdrawalTimelockNotPassed)?;
        }
    }

    Ok(AccessControlResponse { registrar })
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: end_stake_withdrawal");

    let StateTransitionRequest {
        pending_withdrawal,
        user_acc_info,
        user_mega_acc_info,
        user_delegate_acc_info,
        user_delegate_mega_acc_info,
        vault_authority_acc_info,
        tok_program_acc_info,
        registrar,
        registrar_acc_info,
        escrow_vault_acc_info,
        mega_escrow_vault_acc_info,
        entity,
        member,
    } = req;

    // Send the funds from the escrow vault to the user.
    {
        if pending_withdrawal.payment.asset_amount > 0 {
            invoke_token_transfer(
                escrow_vault_acc_info,
                user_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                pending_withdrawal.payment.asset_amount,
            )?;
        }
        if pending_withdrawal.payment.mega_asset_amount > 0 {
            invoke_token_transfer(
                mega_escrow_vault_acc_info,
                user_mega_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                pending_withdrawal.payment.mega_asset_amount,
            )?;
        }
        if pending_withdrawal.delegate_payment.asset_amount > 0 {
            let user_delegate_acc_info =
                user_delegate_acc_info.ok_or(RegistryErrorCode::DelegateAccountsNotProvided)?;
            invoke_token_transfer(
                escrow_vault_acc_info,
                user_delegate_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                pending_withdrawal.delegate_payment.asset_amount,
            )?;
        }
        if pending_withdrawal.delegate_payment.mega_asset_amount > 0 {
            let user_delegate_mega_acc_info = user_delegate_mega_acc_info
                .ok_or(RegistryErrorCode::DelegateAccountsNotProvided)?;
            invoke_token_transfer(
                mega_escrow_vault_acc_info,
                user_delegate_mega_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                pending_withdrawal.delegate_payment.mega_asset_amount,
            )?;
        }
    }

    // Burn for one time use.
    pending_withdrawal.burned = true;

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    registrar_acc_info: &'a AccountInfo<'b>,
    pending_withdrawal_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    delegate_owner_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    clock_acc_info: &'a AccountInfo<'b>,
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
    program_id: &'a Pubkey,
    delegate: bool,
}

struct AccessControlResponse {
    registrar: Registrar,
}

struct StateTransitionRequest<'a, 'b, 'c> {
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    user_acc_info: &'a AccountInfo<'b>,
    user_mega_acc_info: &'a AccountInfo<'b>,
    user_delegate_acc_info: Option<&'a AccountInfo<'b>>,
    user_delegate_mega_acc_info: Option<&'a AccountInfo<'b>>,
    registrar: &'c Registrar,
    pending_withdrawal: &'c mut PendingWithdrawal,
    entity: &'c mut Entity,
    member: &'c mut Member,
}
