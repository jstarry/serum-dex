#![cfg_attr(feature = "strict", deny(warnings))]

use serum_common::pack::Pack;
use serum_registry::error::{RegistryError, RegistryErrorCode};
use serum_registry::instruction::RegistryInstruction;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::entrypoint::ProgramResult;
use solana_sdk::pubkey::Pubkey;

mod create_entity;
mod create_member;
mod end_stake_withdrawal;
mod entity;
mod initialize;
mod pool;
mod register_capability;
mod stake;
mod stake_intent;
mod stake_intent_withdrawal;
mod start_stake_withdrawal;
mod switch_entity;
mod transfer_stake_intent;
mod update_entity;
mod update_member;

solana_program::entrypoint!(entry);
fn entry(program_id: &Pubkey, accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    let instruction: RegistryInstruction = RegistryInstruction::unpack(instruction_data)
        .map_err(|_| RegistryError::ErrorCode(RegistryErrorCode::WrongSerialization))?;

    let result = match instruction {
        RegistryInstruction::Initialize {
            authority,
            nonce,
            withdrawal_timelock,
            deactivation_timelock_premium,
            reward_activation_threshold,
            pool,
            mega_pool,
        } => initialize::handler(
            program_id,
            accounts,
            authority,
            nonce,
            withdrawal_timelock,
            deactivation_timelock_premium,
            reward_activation_threshold,
            pool,
            mega_pool,
        ),
        RegistryInstruction::RegisterCapability {
            capability_id,
            capability_fee,
        } => register_capability::handler(program_id, accounts, capability_id, capability_fee),
        RegistryInstruction::CreateEntity => create_entity::handler(program_id, accounts),
        RegistryInstruction::UpdateEntity { leader } => {
            update_entity::handler(program_id, accounts, leader)
        }
        RegistryInstruction::CreateMember {
            beneficiary,
            delegate,
            watchtower,
        } => create_member::handler(program_id, accounts, beneficiary, delegate, watchtower),
        RegistryInstruction::UpdateMember {
            watchtower,
            delegate,
        } => update_member::handler(program_id, accounts, watchtower, delegate),
        RegistryInstruction::SwitchEntity => switch_entity::handler(program_id, accounts),
        RegistryInstruction::StakeIntent {
            amount,
            mega,
            delegate,
        } => stake_intent::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::StakeIntentWithdrawal {
            amount,
            mega,
            delegate,
        } => stake_intent_withdrawal::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::Stake {
            amount,
            mega,
            delegate,
        } => stake::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::StartStakeWithdrawal {
            amount,
            mega,
            delegate,
        } => start_stake_withdrawal::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::TransferStakeIntent {
            amount,
            mega,
            delegate,
        } => transfer_stake_intent::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::EndStakeWithdrawal => Err(RegistryError::ErrorCode(
            RegistryErrorCode::NotReadySeeNextMajorVersion,
        )),
    };

    result?;

    Ok(())
}
