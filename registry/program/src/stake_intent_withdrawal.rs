use crate::common::invoke_token_transfer;
use crate::pool::{self, PoolConfig};
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::entity::StakeContext;
use serum_registry::accounts::{vault, Entity, Member, Registrar};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
    is_mega: bool,
    is_delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: stake-intent-withdrawal");

    let acc_infos = &mut accounts.iter();

    // Lockup whitelist relay interface.
    let delegate_owner_acc_info = next_account_info(acc_infos)?;
    let depositor_tok_acc_info = next_account_info(acc_infos)?;
    let tok_authority_acc_info = next_account_info(acc_infos)?;
    let token_program_acc_info = next_account_info(acc_infos)?;

    // Program specfic.
    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let vault_acc_info = next_account_info(acc_infos)?;

    let (stake_ctx, _pool) = {
        let cfg = PoolConfig::GetBasket;
        pool::parse_accounts(cfg, acc_infos, false)?
    };

    let AccessControlResponse { clock, registrar } = access_control(AccessControlRequest {
        delegate_owner_acc_info,
        tok_authority_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        vault_acc_info,
        token_program_acc_info,
        is_delegate,
        is_mega,
        program_id,
        registrar_acc_info,
        amount,
        clock_acc_info,
    })?;

    Entity::unpack_mut(
        &mut entity_acc_info.try_borrow_mut_data()?,
        &mut |entity: &mut Entity| {
            Member::unpack_mut(
                &mut member_acc_info.try_borrow_mut_data()?,
                &mut |member: &mut Member| {
                    state_transition(StateTransitionRequest {
                        entity,
                        member,
                        amount,
                        registrar: &registrar,
                        clock: &clock,
                        registrar_acc_info,
                        vault_acc_info,
                        tok_authority_acc_info,
                        depositor_tok_acc_info,
                        member_acc_info,
                        beneficiary_acc_info,
                        entity_acc_info,
                        token_program_acc_info,
                        is_delegate,
                        is_mega,
                        stake_ctx: &stake_ctx,
                    })
                    .map_err(Into::into)
                },
            )
        },
    )?;

    Ok(())
}

fn access_control(req: AccessControlRequest) -> Result<AccessControlResponse, RegistryError> {
    info!("access-control: stake-intent-withdrawal");

    let AccessControlRequest {
        delegate_owner_acc_info,
        tok_authority_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        vault_acc_info,
        token_program_acc_info,
        registrar_acc_info,
        clock_acc_info,
        program_id,
        is_delegate,
        is_mega,
        amount,
    } = req;

    // Authorization.
    if !beneficiary_acc_info.is_signer {
        return Err(RegistryErrorCode::Unauthorized)?;
    }
    if is_delegate {
        if !delegate_owner_acc_info.is_signer {
            return Err(RegistryErrorCode::Unauthorized)?;
        }
    }

    // Account validation.
    let clock = access_control::clock(clock_acc_info)?;
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    let _ = access_control::entity(entity_acc_info, registrar_acc_info, program_id)?;
    let member = access_control::member(
        member_acc_info,
        entity_acc_info,
        beneficiary_acc_info,
        Some(delegate_owner_acc_info),
        is_delegate,
        program_id,
    )?;
    let vault = access_control::vault(
        vault_acc_info,
        registrar_acc_info,
        &registrar,
        program_id,
        is_mega,
    )?;
    // Match the vault authority to the vault.
    if vault.owner != *tok_authority_acc_info.key {
        return Err(RegistryErrorCode::InvalidVaultAuthority)?;
    }
    if is_delegate {
        // Match the signer to the Member account's delegate.
        if *delegate_owner_acc_info.key != member.books.delegate().owner {
            return Err(RegistryErrorCode::InvalidMemberDelegateOwner)?;
        }
    }

    // StakeIntentWithdrawal specific.
    if amount > member.stake_intent(is_mega, is_delegate) {
        return Err(RegistryErrorCode::InsufficientStakeIntentBalance)?;
    }

    info!("access-control: success");

    Ok(AccessControlResponse { clock, registrar })
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: stake-intent-withdrawal");

    let StateTransitionRequest {
        entity,
        member,
        amount,
        registrar,
        clock,
        registrar_acc_info,
        tok_authority_acc_info,
        depositor_tok_acc_info,
        vault_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        token_program_acc_info,
        is_delegate,
        is_mega,
        stake_ctx,
    } = req;

    // Transfer funds from the program vault back to the original depositor.
    invoke_token_transfer(
        vault_acc_info,
        depositor_tok_acc_info,
        tok_authority_acc_info,
        token_program_acc_info,
        registrar_acc_info,
        registrar,
        amount,
    )?;

    member.stake_intent_did_withdraw(amount, is_mega, is_delegate);
    entity.stake_intent_did_withdraw(amount, is_mega);
    entity.transition_activation_if_needed(stake_ctx, registrar, clock);

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    delegate_owner_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    program_id: &'a Pubkey,
    tok_authority_acc_info: &'a AccountInfo<'b>,
    depositor_tok_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    token_program_acc_info: &'a AccountInfo<'b>,
    vault_acc_info: &'a AccountInfo<'b>,
    clock_acc_info: &'a AccountInfo<'b>,
    is_delegate: bool,
    is_mega: bool,
    amount: u64,
}

struct AccessControlResponse {
    clock: Clock,
    registrar: Registrar,
}

struct StateTransitionRequest<'a, 'b, 'c> {
    entity: &'c mut Entity,
    member: &'c mut Member,
    is_mega: bool,
    is_delegate: bool,
    registrar: &'c Registrar,
    clock: &'c Clock,
    amount: u64,
    registrar_acc_info: &'a AccountInfo<'b>,
    vault_acc_info: &'a AccountInfo<'b>,
    tok_authority_acc_info: &'a AccountInfo<'b>,
    depositor_tok_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    token_program_acc_info: &'a AccountInfo<'b>,
    stake_ctx: &'c StakeContext,
}
