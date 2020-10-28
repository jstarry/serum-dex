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

pub fn handler<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    spt_amount: u64,
    is_mega: bool,
    is_delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: stake");

    let acc_infos = &mut accounts.iter();

    // Lockup whitelist relay itnerface.

    let delegate_owner_acc_info = next_account_info(acc_infos)?;
    let depositor_tok_acc_info = next_account_info(acc_infos)?;
    let tok_authority_acc_info = next_account_info(acc_infos)?;
    let token_program_acc_info = next_account_info(acc_infos)?;

    // Program specific.

    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;

    // Pool accounts.
    let (stake_ctx, pool) = {
        let cfg = PoolConfig::Stake {
            registry_signer_acc_info: vault_authority_acc_info,
            registrar_acc_info,
            token_program_acc_info,
        };
        pool::parse_accounts(cfg, acc_infos, is_mega)?
    };

    // TODO: STAKING POOL ACCOUNTS HERE.
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
                tok_authority_acc_info,
                depositor_tok_acc_info,
                member_acc_info,
                delegate_owner_acc_info,
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
                        tok_authority_acc_info,
                        depositor_tok_acc_info,
                        member_acc_info,
                        beneficiary_acc_info,
                        entity_acc_info,
                        token_program_acc_info,
                        stake_ctx: &stake_ctx,
                        pool: &pool,
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
        tok_authority_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        delegate_owner_acc_info,
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
    // Delegate implies the signer is the program-derived-address of the
    // lockup program, the vault of which we have delegate access to.
    if is_delegate {
        if !delegate_owner_acc_info.is_signer {
            return Err(RegistryErrorCode::Unauthorized)?;
        }
    }
    // No delegate implies it's a regular transfer and so the owner must sign.
    else {
        if !tok_authority_acc_info.is_signer {
            return Err(RegistryErrorCode::Unauthorized)?;
        }
    }

    // Account validation.
    let registrar = access_control::registrar(registrar_acc_info, program_id)?;
    access_control::entity_check(entity, entity_acc_info, registrar_acc_info, program_id)?;
    let member = access_control::member(
        member_acc_info,
        entity_acc_info,
        beneficiary_acc_info,
        Some(delegate_owner_acc_info),
        is_delegate,
        program_id,
    )?;
    if is_delegate {
        // Match the signer to the Member account's delegate.
        if *delegate_owner_acc_info.key != member.books.delegate().owner {
            return Err(RegistryErrorCode::InvalidMemberDelegateOwner)?;
        }

        // TODO: the tok authority should be the pool vault.
        //
        //       Do we even need to check this?
        //if *tok_authority_acc_info.key != vault.owner {
        //    return Err(RegistryErrorCode::InvalidTokenAuthority)?;
        //}
    }
    // TODO: add pools here.

    // All stake from a previous generation must be withdrawn before adding
    // stake for a new generation.
    //
    // Does not include stake-intent.
    if member.generation != entity.generation {
        if !member.stake_is_empty() {
            return Err(RegistryErrorCode::StaleStakeNeedsWithdrawal)?;
        }
    }
    // Only activated nodes can stake. If this spt_amount puts us over the
    // activation threshold then allow it, since the node will be activated
    // once the funds are staked.
    let srm_equivalent = stake_ctx.srm_equivalent(spt_amount, is_mega);
    if srm_equivalent + entity.activation_amount(stake_ctx) < registrar.reward_activation_threshold
    {
        return Err(RegistryErrorCode::EntityNotActivated)?;
    }

    if srm_equivalent > member.stake_intent(is_mega, is_delegate) {
        return Err(RegistryErrorCode::InsufficientStakeIntentBalance)?;
    }

    info!("access-control: success");

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
        tok_authority_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        token_program_acc_info,
        registrar,
        clock,
        stake_ctx,
        pool,
    } = req;

    // Transfer funds into the staking pool and issue the staking pool tokens.
    pool.create(spt_amount, registrar.nonce)?;

    // Perform transfer in accounts for bookeeping.
    {
        let stake_intent_amount = stake_ctx.basket_primary_asset(spt_amount, is_mega);

        member.sub_stake_intent(stake_intent_amount, is_mega, is_delegate);
        entity.sub_stake_intent(stake_intent_amount, is_mega);

        member.spt_add(spt_amount, is_mega, is_delegate);
        entity.spt_add(spt_amount, is_mega);

        entity.transition_activation_if_needed(&stake_ctx, &registrar, &clock);
    }

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a, 'b, 'c> {
    tok_authority_acc_info: &'a AccountInfo<'b>,
    depositor_tok_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    token_program_acc_info: &'a AccountInfo<'b>,
    registrar_acc_info: &'a AccountInfo<'b>,
    delegate_owner_acc_info: &'a AccountInfo<'b>,
    is_mega: bool,
    is_delegate: bool,
    spt_amount: u64,
    entity: &'c Entity,
    program_id: &'c Pubkey,
    stake_ctx: &'c StakeContext,
}

struct StateTransitionRequest<'a, 'b, 'c> {
    entity: &'c mut Entity,
    member: &'c mut Member,
    registrar: &'c Registrar,
    clock: &'c Clock,
    spt_amount: u64,
    is_mega: bool,
    is_delegate: bool,
    tok_authority_acc_info: &'a AccountInfo<'b>,
    depositor_tok_acc_info: &'a AccountInfo<'b>,
    member_acc_info: &'a AccountInfo<'b>,
    beneficiary_acc_info: &'a AccountInfo<'b>,
    entity_acc_info: &'a AccountInfo<'b>,
    token_program_acc_info: &'a AccountInfo<'b>,
    stake_ctx: &'c StakeContext,
    pool: &'c PoolApi<'a, 'b>,
}
