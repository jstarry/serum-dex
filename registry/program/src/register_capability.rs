use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::registrar::{Registrar, CAPABILITY_LEN};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    capability_id: u8,
    capability_fee: u32,
) -> Result<(), RegistryError> {
    info!("handler: register_capability");

    let acc_infos = &mut accounts.iter();

    let registrar_authority_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;

    access_control(AccessControlRequest {
        registrar_authority_acc_info,
        registrar_acc_info,
        capability_id,
        program_id,
    })?;

    Registrar::unpack_mut(
        &mut registrar_acc_info.try_borrow_mut_data()?,
        &mut |registrar: &mut Registrar| {
            state_transition(StateTransitionRequest {
                registrar,
                capability_id,
                capability_fee,
            })
            .map_err(Into::into)
        },
    )?;

    Ok(())
}

fn access_control(req: AccessControlRequest) -> Result<(), RegistryError> {
    info!("access-control: register_capability");

    let AccessControlRequest {
        registrar_authority_acc_info,
        registrar_acc_info,
        capability_id,
        program_id,
    } = req;

    // Governance authorization.
    let _ =
        access_control::governance(program_id, registrar_acc_info, registrar_authority_acc_info)?;

    // RegisterCapability specific.
    if capability_id as usize >= CAPABILITY_LEN {
        return Err(RegistryErrorCode::InvalidCapabilityId)?;
    }

    info!("access-control: success");

    Ok(())
}

fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: register_capability");

    let StateTransitionRequest {
        mut registrar,
        capability_id,
        capability_fee,
    } = req;

    registrar.capabilities_fees[capability_id as usize] = capability_fee;

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    registrar_authority_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    capability_id: u8,
    program_id: &'a Pubkey,
}

struct StateTransitionRequest<'a> {
    registrar: &'a mut Registrar,
    capability_id: u8,
    capability_fee: u32,
}
