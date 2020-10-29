use crate::common::invoke_token_transfer;
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::{vault, PendingWithdrawal, Registrar};
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

    let pending_withdrawal_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let member_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let escrow_vault_acc_info = next_account_info(acc_infos)?;
    let mega_escrow_vault_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;
    let tok_program_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let user_acc_info = next_account_info(acc_infos)?;

    let delegate_owner_acc_info = {
        if delegate {
            Some(next_account_info(acc_infos)?)
        } else {
            None
        }
    };

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
            state_transition(StateTransitionRequest {
                pending_withdrawal,
                user_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar,
                registrar_acc_info,
                escrow_vault_acc_info,
                mega_escrow_vault_acc_info,
            })
            .map_err(Into::into)
        },
    )?;

    Ok(())
}

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

    // TODO: check delegate here.

    // Account validation.
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
        vault_authority_acc_info,
        tok_program_acc_info,
        registrar,
        registrar_acc_info,
        escrow_vault_acc_info,
        mega_escrow_vault_acc_info,
    } = req;

    // Send the funds from the escrow vault to the user.
    {
        let amount_0 = pending_withdrawal.asset_amount;
        if amount_0 > 0 {
            invoke_token_transfer(
                escrow_vault_acc_info,
                user_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                amount_0,
            )?;
        }
        let amount_1 = pending_withdrawal.mega_asset_amount;
        if amount_1 > 0 {
            invoke_token_transfer(
                mega_escrow_vault_acc_info,
                user_acc_info,
                vault_authority_acc_info,
                tok_program_acc_info,
                registrar_acc_info,
                registrar,
                amount_1,
            )?;
        }
    }

    // Burn the pending_withdrawal receipt.
    pending_withdrawal.burned = true;

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    registrar_acc_info: &'a AccountInfo<'b>,
    pending_withdrawal_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    delegate_owner_acc_info: Option<&'a AccountInfo<'b>>,
    entity_acc_info: &'a AccountInfo<'b>,
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
}

struct StateTransitionRequest<'a, 'b, 'c> {
    escrow_vault_acc_info: &'a AccountInfo<'b>,
    mega_escrow_vault_acc_info: &'a AccountInfo<'b>,
    vault_authority_acc_info: &'a AccountInfo<'b>,
    tok_program_acc_info: &'a AccountInfo<'b>,
    user_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    pending_withdrawal: &'c mut PendingWithdrawal,
    registrar: &'c Registrar,
}
