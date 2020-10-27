use crate::pool::{self, PoolConfig};
use serum_common::pack::*;
use serum_registry::access_control;
use serum_registry::accounts::entity::StakeContext;
use serum_registry::accounts::{Entity, Member, Registrar, Watchtower};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;
use std::convert::Into;

pub fn handler<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
) -> Result<(), RegistryError> {
    info!("handler: update_member");

    let acc_infos = &mut accounts.iter();

    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let curr_entity_acc_info = next_account_info(acc_infos)?;
    let new_entity_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;

    let (stake_ctx, _pool) = {
        let cfg = PoolConfig::ReadBasket;
        pool::parse_accounts(cfg, acc_infos, false)?
    };

    let AccessControlResponse { registrar, clock } = access_control(AccessControlRequest {
        member_acc_info,
        beneficiary_acc_info,
        program_id,
        registrar_acc_info,
        curr_entity_acc_info,
        new_entity_acc_info,
        clock_acc_info,
    })?;

    Entity::unpack_mut(
        &mut curr_entity_acc_info.try_borrow_mut_data()?,
        &mut |curr_entity: &mut Entity| {
            Entity::unpack_mut(
                &mut new_entity_acc_info.try_borrow_mut_data()?,
                &mut |new_entity: &mut Entity| {
                    Member::unpack_mut(
                        &mut member_acc_info.try_borrow_mut_data()?,
                        &mut |member: &mut Member| {
                            state_transition(StateTransitionRequest {
                                member,
                                curr_entity,
                                new_entity,
                                clock: &clock,
                                registrar: &registrar,
                                stake_ctx: &stake_ctx,
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
    info!("access-control: switch_entity");

    let AccessControlRequest {
        member_acc_info,
        beneficiary_acc_info,
        program_id,
        registrar_acc_info,
        curr_entity_acc_info,
        new_entity_acc_info,
        clock_acc_info,
    } = req;

    // Beneficiary authorization.
    if !beneficiary_acc_info.is_signer {
        return Err(RegistryErrorCode::Unauthorized)?;
    }

    // Account validation.
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    let member = access_control::member(
        member_acc_info,
        curr_entity_acc_info,
        beneficiary_acc_info,
        None,
        false,
        program_id,
    )?;
    let _curr_entity =
        access_control::entity(curr_entity_acc_info, registrar_acc_info, program_id)?;
    let _new_entity = access_control::entity(new_entity_acc_info, registrar_acc_info, program_id)?;
    let clock = access_control::clock(clock_acc_info)?;

    info!("access-control: success");

    Ok(AccessControlResponse { registrar, clock })
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: switch_entity");

    let StateTransitionRequest {
        member,
        curr_entity,
        new_entity,
        stake_ctx,
        registrar,
        clock,
    } = req;

    curr_entity.remove(member);
    curr_entity.transition_activation_if_needed(stake_ctx, registrar, clock);

    new_entity.add(member);
    new_entity.transition_activation_if_needed(stake_ctx, registrar, clock);

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a> {
    member_acc_info: &'a AccountInfo<'a>,
    beneficiary_acc_info: &'a AccountInfo<'a>,
    program_id: &'a Pubkey,
    registrar_acc_info: &'a AccountInfo<'a>,
    curr_entity_acc_info: &'a AccountInfo<'a>,
    new_entity_acc_info: &'a AccountInfo<'a>,
    clock_acc_info: &'a AccountInfo<'a>,
}

struct AccessControlResponse {
    registrar: Registrar,
    clock: Clock,
}

struct StateTransitionRequest<'a> {
    member: &'a mut Member,
    curr_entity: &'a mut Entity,
    new_entity: &'a mut Entity,
    stake_ctx: &'a StakeContext,
    registrar: &'a Registrar,
    clock: &'a Clock,
}
