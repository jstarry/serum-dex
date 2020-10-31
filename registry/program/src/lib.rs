#![cfg_attr(feature = "strict", deny(warnings))]

use serum_common::pack::Pack;
use serum_registry::error::{RegistryError, RegistryErrorCode};
use serum_registry::instruction::RegistryInstruction;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::entrypoint::ProgramResult;
use solana_sdk::pubkey::Pubkey;

mod common;
mod create_entity;
mod create_member;
mod deposit;
mod end_stake_withdrawal;
mod entity;
mod initialize;
mod pool;
mod stake;
mod start_stake_withdrawal;
mod switch_entity;
mod update_entity;
mod update_member;
mod withdraw;

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
        RegistryInstruction::Deposit {
            amount,
            mega,
            delegate,
        } => deposit::handler(program_id, accounts, amount, mega, delegate),
        RegistryInstruction::Withdraw {
            amount,
            mega,
            delegate,
        } => withdraw::handler(program_id, accounts, amount, mega, delegate),
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
        RegistryInstruction::EndStakeWithdrawal { delegate } => {
            end_stake_withdrawal::handler(program_id, accounts, delegate)
        }
    };

    result?;

    Ok(())
}
