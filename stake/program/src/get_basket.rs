use borsh::BorshSerialize;
use serum_pool::context::PoolContext;
use serum_pool_schema::{Basket, PoolState};
use serum_stake::error::StakeError;
use solana_program::info;
use solana_sdk::instruction::{AccountMeta, Instruction};

pub fn handler(
    ctx: &PoolContext,
    state: &PoolState,
    spt_amount: u64,
) -> Result<Basket, StakeError> {
    let basket = {
        // TODO: check the semantics of this make sense.
        //
        // If no pool tokens are in circulation, then to create `spt_amount`
        // one must deposit the same `spt_amount`. Otherwise, take a
        // `simple_basket`.
        if ctx.total_pool_tokens()? == 0 {
            let mut quantities = vec![spt_amount as i64];
            if state.assets.len() == 2 {
                quantities.push(0);
            }
            Basket { quantities }
        } else {
            ctx.get_simple_basket(spt_amount)?
        }
    };

    let retbuf_accs = ctx.retbuf.as_ref().expect("must have retbuf accounts");
    let offset: usize = 0;
    let mut data = offset.to_le_bytes().to_vec();
    data.append(&mut basket.try_to_vec().expect("basket must serialize"));
    let instr = Instruction {
        program_id: *retbuf_accs.program.key,
        accounts: vec![AccountMeta::new(*retbuf_accs.account.key, false)],
        data,
    };
    info!("invoking retbuf");
    solana_sdk::program::invoke(
        &instr,
        &[retbuf_accs.account.clone(), retbuf_accs.program.clone()],
    )?;
    Ok(basket)
}
