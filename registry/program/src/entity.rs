use crate::pool::PoolApi;
use serum_common::pack::Pack;
use serum_registry::access_control;
use serum_registry::accounts::entity::StakeContext;
use serum_registry::accounts::{vault, Entity, Member, Registrar};
use serum_registry::error::RegistryError;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::clock::Clock;

// with_entity should be used for any instruction relying on the most up to
// date `state` of an Entity.
//
//
// As time, passes, it's possible an Entity's internal FSM *should* have
// transitioned (i.e., from PendingDeactivation -> Inactive), but didn't
// because no transaction was invoked.
//
// This method transitions the Entity's state, before performing the action
// provided by the given closure.
pub fn with_entity<F>(req: WithEntityRequest, f: &mut F) -> Result<(), RegistryError>
where
    F: FnMut(&mut Entity, &StakeContext, &Registrar, &Clock) -> Result<(), RegistryError>,
{
    let WithEntityRequest {
        pool,
        mega_pool,
        entity,
        registrar,
        clock,
        program_id,
    } = req;
    Entity::unpack_mut(
        &mut entity.try_borrow_mut_data()?,
        &mut |entity: &mut Entity| {
            let stake_ctx = StakeContext::new(pool.get_basket(1)?, mega_pool.get_basket(1)?);
            let clock = access_control::clock(&clock)?;
            let registrar = access_control::registrar(&registrar, program_id)?;
            entity.transition_activation_if_needed(&stake_ctx, &registrar, &clock);

            f(entity, &stake_ctx, &registrar, &clock).map_err(Into::into)
        },
    )?;
    Ok(())
}

pub struct WithEntityRequest<'a, 'b, 'c> {
    pub pool: &'a PoolApi<'b, 'c>,
    pub mega_pool: &'a PoolApi<'b, 'c>,
    pub entity: &'a AccountInfo<'c>,
    pub registrar: &'a AccountInfo<'c>,
    pub clock: &'a AccountInfo<'c>,
    pub program_id: &'a Pubkey,
}
