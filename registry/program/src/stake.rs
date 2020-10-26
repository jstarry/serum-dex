use serum_common::pack::Pack;
use serum_pool_schema::Basket;
use serum_registry::access_control;
use serum_registry::accounts::entity::{with_entity, WithEntityRequest};
use serum_registry::accounts::{vault, Entity, Member, Registrar};
use serum_registry::error::{RegistryError, RegistryErrorCode};
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::info;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;

pub fn handler<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    amount: u64,
    is_mega: bool,
    is_delegate: bool,
) -> Result<(), RegistryError> {
    info!("handler: stake");

    let acc_infos = &mut accounts.iter();

    // Lockup whitelist relay interface.

    let delegate_owner_acc_info = next_account_info(acc_infos)?;
    let depositor_tok_acc_info = next_account_info(acc_infos)?;
    let vault_acc_info = next_account_info(acc_infos)?;
    let depositor_tok_owner_acc_info = next_account_info(acc_infos)?;
    let token_program_acc_info = next_account_info(acc_infos)?;

    // Program specific.

    let member_acc_info = next_account_info(acc_infos)?;
    let beneficiary_acc_info = next_account_info(acc_infos)?;
    let entity_acc_info = next_account_info(acc_infos)?;
    let registrar_acc_info = next_account_info(acc_infos)?;
    let clock_acc_info = next_account_info(acc_infos)?;
    let vault_authority_acc_info = next_account_info(acc_infos)?;
    let pool_program_id_acc_info = next_account_info(acc_infos)?;

    // Pool interface.

    let pool_acc_info = next_account_info(acc_infos)?;
    let pool_tok_mint_acc_info = next_account_info(acc_infos)?;
    let pool_asset_vault_acc_info = next_account_info(acc_infos)?;
    assert!(vault_acc_info.key == pool_asset_vault_acc_info.key);
    let pool_vault_authority_acc_info = next_account_info(acc_infos)?;
    let user_pool_tok_acc_info = next_account_info(acc_infos)?;
    let user_asset_tok_acc_info = next_account_info(acc_infos)?;
    assert!(user_asset_tok_acc_info.key == depositor_tok_acc_info.key);
    let user_tok_auth_acc_info = next_account_info(acc_infos)?;
    assert!(user_tok_auth_acc_info.key == depositor_tok_owner_acc_info.key);
    let retbuf_acc_info = user_tok_auth_acc_info.clone(); //next_account_info(acc_infos)?;
    let retbuf_program_acc_info = user_tok_auth_acc_info.clone(); //next_account_info(acc_infos)?;

    let pool = PoolApi {
        pool_program_id_acc_info: pool_program_id_acc_info.clone(),
        pool_acc_info: pool_acc_info.clone(),
        pool_tok_mint_acc_info: pool_tok_mint_acc_info.clone(),
        pool_asset_vault_acc_info: pool_asset_vault_acc_info.clone(),
        pool_vault_authority_acc_info: pool_vault_authority_acc_info.clone(),
        user_pool_tok_acc_info: user_pool_tok_acc_info.clone(),
        user_asset_tok_acc_info: user_asset_tok_acc_info.clone(),
        user_tok_auth_acc_info: user_tok_auth_acc_info.clone(),
        vault_authority_acc_info: vault_authority_acc_info.clone(),
        registrar_acc_info: registrar_acc_info.clone(),
        token_program_acc_info: token_program_acc_info.clone(),
        retbuf_acc_info: retbuf_acc_info.clone(),
        retbuf_program_acc_info: retbuf_program_acc_info.clone(),
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
        },
        &mut |entity: &mut Entity, registrar: &Registrar, clock: &Clock| {
            access_control(AccessControlRequest {
                depositor_tok_owner_acc_info,
                depositor_tok_acc_info,
                member_acc_info,
                registrar_acc_info,
                delegate_owner_acc_info,
                beneficiary_acc_info,
                entity_acc_info,
                token_program_acc_info,
                amount,
                is_mega,
                is_delegate,
                entity,
                program_id,
            })?;
            Member::unpack_mut(
                &mut member_acc_info.try_borrow_mut_data()?,
                &mut |member: &mut Member| {
                    state_transition(StateTransitionRequest {
                        entity,
                        member,
                        amount,
                        is_delegate,
                        is_mega,
                        registrar,
                        registrar_acc_info,
                        clock,
                        depositor_tok_owner_acc_info,
                        depositor_tok_acc_info,
                        member_acc_info,
                        beneficiary_acc_info,
                        entity_acc_info,
                        token_program_acc_info,
                        vault_authority_acc_info,
                        pool: pool.clone(),
                    })
                    .map_err(Into::into)
                },
            )
            .map_err(Into::into)
        },
    )
}

#[inline(always)]
fn access_control(req: AccessControlRequest) -> Result<(), RegistryError> {
    info!("access-control: stake");

    let AccessControlRequest {
        depositor_tok_owner_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        delegate_owner_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        token_program_acc_info,
        registrar_acc_info,
        amount,
        is_mega,
        is_delegate,
        entity,
        program_id,
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
        Some(delegate_owner_acc_info),
        is_delegate,
        program_id,
    )?;
    // TODO: add pools here.

    // Stake specific.

    // All stake from a previous generation must be withdrawn before adding
    // stake for a new generation.
    if member.generation != entity.generation {
        if !member.stake_is_empty() {
            return Err(RegistryErrorCode::StaleStakeNeedsWithdrawal)?;
        }
    }
    // Only activated nodes can stake. If this amount puts us over the
    // activation threshold then allow it.
    if amount + entity.activation_amount() < registrar.reward_activation_threshold {
        return Err(RegistryErrorCode::EntityNotActivated)?;
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
        amount,
        is_mega,
        is_delegate,
        depositor_tok_owner_acc_info,
        depositor_tok_acc_info,
        member_acc_info,
        beneficiary_acc_info,
        entity_acc_info,
        token_program_acc_info,
        registrar,
        registrar_acc_info,
        clock,
        vault_authority_acc_info,
        pool,
    } = req;

    // Transfer funds into the staking pool, issuing a staking pool token.
    pool.create(amount, registrar.nonce);

    // Translate stake token ammount to underlying asset/basket amount.
    info!(&format!("BEFORE_AMOUNT {:?}", amount));
    let basket_asset_amount = amount; //pool.get_basket(amount)?;
    info!(&format!("AFTER_AMOUNT {:?}", basket_asset_amount));
    // Update accounts for bookeeping.
    {
        // TODO: add stake pool token amount to the member and entity.
        //       Need to translate between pool asset and pool token when
        //       calculating activation threshold.
        //
        //       Probably need to complete get rid of the stake amount field
        //       adn replace it with the stake token and dynamically loookup
        //       what the basket is worth on each transaction.
        //
        //
        member.add_stake(basket_asset_amount, is_mega, is_delegate);
        member.generation = entity.generation;

        entity.add_stake(basket_asset_amount, is_mega, &registrar, &clock);
    }

    info!("state-transition: success");

    Ok(())
}

struct AccessControlRequest<'a, 'b> {
    depositor_tok_owner_acc_info: &'a AccountInfo<'a>,
    depositor_tok_acc_info: &'a AccountInfo<'a>,
    member_acc_info: &'a AccountInfo<'a>,
    delegate_owner_acc_info: &'a AccountInfo<'a>,
    beneficiary_acc_info: &'a AccountInfo<'a>,
    entity_acc_info: &'a AccountInfo<'a>,
    token_program_acc_info: &'a AccountInfo<'a>,
    registrar_acc_info: &'a AccountInfo<'a>,
    is_mega: bool,
    is_delegate: bool,
    amount: u64,
    entity: &'b Entity,
    program_id: &'a Pubkey,
}

struct StateTransitionRequest<'a, 'b> {
    entity: &'b mut Entity,
    member: &'b mut Member,
    registrar: &'b Registrar,
    clock: &'b Clock,
    amount: u64,
    is_mega: bool,
    is_delegate: bool,
    vault_authority_acc_info: &'a AccountInfo<'a>,
    registrar_acc_info: &'a AccountInfo<'a>,
    depositor_tok_owner_acc_info: &'a AccountInfo<'a>,
    depositor_tok_acc_info: &'a AccountInfo<'a>,
    member_acc_info: &'a AccountInfo<'a>,
    beneficiary_acc_info: &'a AccountInfo<'a>,
    entity_acc_info: &'a AccountInfo<'a>,
    token_program_acc_info: &'a AccountInfo<'a>,
    pool: PoolApi<'a>,
}

#[derive(Clone)]
struct PoolApi<'a> {
    pub pool_acc_info: AccountInfo<'a>,
    pub pool_tok_mint_acc_info: AccountInfo<'a>,
    pub pool_asset_vault_acc_info: AccountInfo<'a>,
    pub pool_vault_authority_acc_info: AccountInfo<'a>,
    pub user_pool_tok_acc_info: AccountInfo<'a>,
    pub user_asset_tok_acc_info: AccountInfo<'a>,
    pub user_tok_auth_acc_info: AccountInfo<'a>,
    pub token_program_acc_info: AccountInfo<'a>,
    pub vault_authority_acc_info: AccountInfo<'a>,
    pub pool_program_id_acc_info: AccountInfo<'a>,
    pub registrar_acc_info: AccountInfo<'a>,
    pub retbuf_acc_info: AccountInfo<'a>,
    pub retbuf_program_acc_info: AccountInfo<'a>,
}

impl<'a> PoolApi<'a> {
    pub fn create(&self, spt_amount: u64, registrar_nonce: u8) -> Result<(), RegistryError> {
        let instr = serum_stake::instruction::creation(
            self.pool_program_id_acc_info.key,
            self.pool_acc_info.key,
            self.pool_tok_mint_acc_info.key,
            self.pool_asset_vault_acc_info.key,
            self.pool_vault_authority_acc_info.key,
            self.user_pool_tok_acc_info.key,
            self.user_asset_tok_acc_info.key,
            self.user_tok_auth_acc_info.key,
            self.vault_authority_acc_info.key,
            spt_amount,
        );
        let signer_seeds = vault::signer_seeds(self.registrar_acc_info.key, &registrar_nonce);
        solana_sdk::program::invoke_signed(
            &instr,
            &[
                self.pool_acc_info.clone(),
                self.pool_tok_mint_acc_info.clone(),
                self.pool_asset_vault_acc_info.clone(),
                self.pool_vault_authority_acc_info.clone(),
                self.user_pool_tok_acc_info.clone(),
                self.user_asset_tok_acc_info.clone(),
                self.user_tok_auth_acc_info.clone(),
                self.token_program_acc_info.clone(),
                self.vault_authority_acc_info.clone(),
                self.pool_program_id_acc_info.clone(),
            ],
            &[&signer_seeds],
        )?;
        Ok(())
    }
    pub fn get_basket(self, spt_amount: u64) -> Result<u64, RegistryError> {
        let instr = serum_stake::instruction::get_basket(
            self.pool_program_id_acc_info.key,
            self.pool_acc_info.key,
            self.pool_tok_mint_acc_info.key,
            self.pool_asset_vault_acc_info.key,
            self.pool_vault_authority_acc_info.key,
            self.retbuf_acc_info.key,
            self.retbuf_program_acc_info.key,
            spt_amount,
        );
        solana_sdk::program::invoke(
            &instr,
            &[
                self.pool_program_id_acc_info,
                self.pool_acc_info,
                self.pool_tok_mint_acc_info,
                self.pool_asset_vault_acc_info,
                self.pool_vault_authority_acc_info,
                self.retbuf_acc_info.clone(),
                self.retbuf_program_acc_info,
            ],
        )?;
        let basket = Basket::unpack(&self.retbuf_acc_info.try_borrow_data()?)?;
        Ok(basket.quantities[0] as u64)
    }
}
