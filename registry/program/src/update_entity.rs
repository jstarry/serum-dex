use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::Entity;
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    leader: Pubkey,
) -> Result<(), RegistryError> {
    info!("handler: update_entity");

    let acc_infos = &mut accounts.iter();

    let entity_acc_info = next_account_info(acc_infos)?;
    let entity_leader_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;

    access_control(AccessControlRequest {
        entity_acc_info,
        entity_leader_acc_info,
        registrar_acc_info,
        program_id,
    })?;

    Entity::unpack_mut(
        &mut entity_acc_info.try_borrow_mut_data()?,
        &mut |entity: &mut Entity| {
            state_transition(StateTransitionRequest { entity, leader }).map_err(Into::into)
        },
    )?;

    Ok(())
}

fn access_control(req: AccessControlRequest) -> Result<(), RegistryError> {
    info!("access-control: update_entity");

    let AccessControlRequest {
        entity_acc_info,
        entity_leader_acc_info,
        registrar_acc_info,
        program_id,
    } = req;

    // Node leader authorization.
    if !entity_leader_acc_info.is_signer {
        return Err(RegistryErrorCode::Unauthorized)?;
    }

    let _ = access_control::registrar(registrar_acc_info, program_id)?;
    let entity = access_control::entity(entity_acc_info, registrar_acc_info, program_id)?;
    if entity.leader != entity.leader {
        return Err(RegistryErrorCode::EntityLeaderMismatch)?;
    }

    info!("access-control: success");

    Ok(())
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: update_entity");

    let StateTransitionRequest { entity, leader } = req;

    entity.leader = leader;

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    entity_acc_info: &'a AccountInfo<'b>,
    entity_leader_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    program_id: &'a Pubkey,
}

struct StateTransitionRequest<'a> {
    entity: &'a mut Entity,
    leader: Pubkey,
}
