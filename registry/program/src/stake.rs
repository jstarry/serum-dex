use crate::entity::{with_entity, WithEntityRequest};
use crate::pool::{self, PoolApi, PoolConfig};
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::entity::StakeContext;
use serum_registry::accounts::{Entity, Member, Registrar};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_program::info;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;

pub fn handler(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    spt_amount: u64,
    is_mega: bool,
    is_delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: stake");

    let acc_infos = &mut accounts.iter();

    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let token_program_acc_info = next_account_info(acc_infos)?;

    // Pool accounts.
    let (stake_ctx, pool) = {
        let cfg = PoolConfig::Transact {
            registry_signer_acc_info: vault_authority_acc_info,
            registrar_acc_info,
            token_program_acc_info,
        };
        pool::parse_accounts(cfg, acc_infos, is_mega)?
    };

    // TODO: Must check the user token accounts. If we have a delegate stake
    //       then all creations/redemptions must go to accounts owned by
    //       the delegate_owner.

    // TODO: what validation do we need to do here? Ideally, we only check
    //       the pool address is correct, and the rest is done by the pool
    //       framework.

    with_entity(
        WithEntityRequest {
            entity: entity_acc_info,
            registrar: registrar_acc_info,
            clock: clock_acc_info,
            program_id,
            stake_ctx: &stake_ctx,
        },
        &mut |entity: &mut Entity, registrar: &Registrar, clock: &Clock| {
            access_control(AccessControlRequest {
                member_acc_info,
                registrar_acc_info,
                beneficiary_acc_info,
                entity_acc_info,
                token_program_acc_info,
                spt_amount,
                is_mega,
                is_delegate,
                entity,
                program_id,
                stake_ctx: &stake_ctx,
            })?;
            Member::unpack_mut(
                &mut member_acc_info.try_borrow_mut_data()?,
                &mut |member: &mut Member| {
                    state_transition(StateTransitionRequest {
                        entity,
                        member,
                        spt_amount,
                        is_delegate,
                        is_mega,
                        registrar,
                        clock,
                        pool: pool.clone(),
                        stake_ctx: &stake_ctx,
                    })
                    .map_err(Into::into)
                },
            )
            .map_err(Into::into)
        },
    )
}

fn access_control(req: AccessControlRequest) -> Result<(), RegistryError> {
    info!("access-control: stake");

    let AccessControlRequest {
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        token_program_acc_info,
        registrar_acc_info,
        spt_amount,
        is_mega,
        is_delegate,
        entity,
        program_id,
        stake_ctx,
    } = req;

    // Beneficiary (or delegate) authorization.
    if !beneficiary_acc_info.is_signer {
        return Err(RegistryErrorCode::Unauthorized)?;
    }

    // Account validation.
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    access_control::entity_check(entity, entity_acc_info, registrar_acc_info, program_id)?;
    let member = access_control::member(
        member_acc_info,
        entity_acc_info,
        beneficiary_acc_info,
        None,
        is_delegate,
        program_id,
    )?;
    // TODO: add pools here.

    // Stake specific.

    // All stake from a previous generation must be withdrawn before adding
    // stake for a new generation.
    if member.generation != entity.generation {
        if !member.stake_is_empty() {
            #[cfg(feature = "program")]
            solana_sdk::info!(&format!("member stkae not empty {:?}", member));
            return Err(RegistryErrorCode::StaleStakeNeedsWithdrawal)?;
        }
    }
    // Only activated nodes can stake.
    if !entity.meets_activation_requirements(stake_ctx, &registrar) {
        return Err(RegistryErrorCode::EntityNotActivated)?;
    }

    Ok(())
}

#[inline(always)]
fn state_transition(req: StateTransitionRequest) -> Result<(), RegistryError> {
    info!("state-transition: stake");

    let StateTransitionRequest {
        entity,
        member,
        spt_amount,
        is_mega,
        is_delegate,
        registrar,
        clock,
        pool,
        stake_ctx,
    } = req;

    // Transfer funds into the staking pool, issuing a staking pool token.
    pool.create(spt_amount, registrar.nonce)?;

    // Update accounts for bookeeping.
    {
        // The amount of SRM/MSRM to purchase `spt_amount` tokens.
        let purchase_price = stake_ctx.basket_quantities(spt_amount, is_mega)?;
        member.generation = entity.generation;
        member.spt_did_create(
            &stake_ctx,
            spt_amount,
            &purchase_price,
            is_mega,
            is_delegate,
        );
        entity.spt_did_create(spt_amount, is_mega);
        entity.transition_activation_if_needed(&stake_ctx, &registrar, &clock);
    }

    Ok(())
}

struct AccessControlRequest<'a, 'b, 'c> {
    member_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    token_program_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    program_id: &'a Pubkey,
    stake_ctx: &'c StakeContext,
    entity: &'c Entity,
    spt_amount: u64,
    is_delegate: bool,
    is_mega: bool,
}

struct StateTransitionRequest<'a, 'b, 'c> {
    pool: PoolApi<'a, 'b>,
    stake_ctx: &'c StakeContext,
    registrar: &'c Registrar,
    entity: &'c mut Entity,
    member: &'c mut Member,
    clock: &'c Clock,
    spt_amount: u64,
    is_delegate: bool,
    is_mega: bool,
}
