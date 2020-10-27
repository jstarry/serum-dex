use serum_common::pack::Pack;
use serum_pool_schema::{Basket, PoolState};
use serum_registry::accounts::entity::StakeContext;
use serum_registry::accounts::vault;
use serum_registry::error::RegistryError;
use solana_sdk::account_info::{next_account_info, AccountInfo};
use solana_sdk::info;

// Methods here assume the proper validation has been done prior to constructing
// the context.
#[derive(Clone)]
pub struct PoolApi<'a, 'b> {
    pub pool_acc_info: &'a AccountInfo<'b>,
    pub pool_tok_mint_acc_info: &'a AccountInfo<'b>,
    pub pool_asset_vault_acc_infos: Vec<&'a AccountInfo<'b>>,
    pub pool_vault_authority_acc_info: &'a AccountInfo<'b>,
    pub pool_program_id_acc_info: &'a AccountInfo<'b>,
    pub retbuf_acc_info: &'a AccountInfo<'b>,
    pub retbuf_program_acc_info: &'a AccountInfo<'b>,
    // For creation.
    pub user_pool_tok_acc_info: Option<&'a AccountInfo<'b>>,
    pub user_asset_tok_acc_info: Option<&'a AccountInfo<'b>>,
    pub user_tok_auth_acc_info: Option<&'a AccountInfo<'b>>,
    pub token_program_acc_info: Option<&'a AccountInfo<'b>>,
    // Registry vault authority. `is_signer` must be true.
    pub vault_authority_acc_info: Option<&'a AccountInfo<'b>>,
    pub registrar_acc_info: Option<&'a AccountInfo<'b>>,
}

impl<'a, 'b> PoolApi<'a, 'b> {
    pub fn create(&self, spt_amount: u64, registrar_nonce: u8) -> Result<(), RegistryError> {
        let instr = serum_stake::instruction::creation(
            self.pool_program_id_acc_info.key,
            self.pool_acc_info.key,
            self.pool_tok_mint_acc_info.key,
            self.pool_asset_vault_acc_infos
                .iter()
                .map(|acc_info| acc_info.key)
                .collect(),
            self.pool_vault_authority_acc_info.key,
            self.user_pool_tok_acc_info.unwrap().key,
            self.user_asset_tok_acc_info.unwrap().key,
            self.user_tok_auth_acc_info.unwrap().key,
            self.vault_authority_acc_info.unwrap().key,
            spt_amount,
        );
        let signer_seeds =
            vault::signer_seeds(self.registrar_acc_info.unwrap().key, &registrar_nonce);
        info!("invoking creation");
        let mut acc_infos = vec![
            self.pool_acc_info.clone(),
            self.pool_tok_mint_acc_info.clone(),
        ];
        for acc_info in self.pool_asset_vault_acc_infos.clone() {
            acc_infos.push(acc_info.clone());
        }
        acc_infos.extend_from_slice(&[
            self.pool_vault_authority_acc_info.clone(),
            self.user_pool_tok_acc_info.unwrap().clone(),
            self.user_asset_tok_acc_info.unwrap().clone(),
            self.user_tok_auth_acc_info.unwrap().clone(),
            self.token_program_acc_info.unwrap().clone(),
            self.vault_authority_acc_info.unwrap().clone(),
            self.pool_program_id_acc_info.clone(),
        ]);
        solana_sdk::program::invoke_signed(&instr, &acc_infos, &[&signer_seeds])?;
        Ok(())
    }
    pub fn get_basket(&self, spt_amount: u64) -> Result<Basket, RegistryError> {
        let instr = serum_stake::instruction::get_basket(
            self.pool_program_id_acc_info.key,
            self.pool_acc_info.key,
            self.pool_tok_mint_acc_info.key,
            self.pool_asset_vault_acc_infos
                .iter()
                .map(|acc_info| acc_info.key)
                .collect(),
            self.pool_vault_authority_acc_info.key,
            self.retbuf_acc_info.key,
            self.retbuf_program_acc_info.key,
            spt_amount,
        );
        info!("invoking get_basket");
        let mut acc_infos = vec![
            self.pool_program_id_acc_info.clone(),
            self.pool_acc_info.clone(),
            self.pool_tok_mint_acc_info.clone(),
        ];
        for acc_info in self.pool_asset_vault_acc_infos.clone() {
            acc_infos.push(acc_info.clone());
        }
        acc_infos.extend_from_slice(&[
            self.pool_vault_authority_acc_info.clone(),
            self.retbuf_acc_info.clone().clone(),
            self.retbuf_program_acc_info.clone(),
        ]);
        solana_sdk::program::invoke(&instr, &acc_infos)?;
        Basket::unpack(&self.retbuf_acc_info.try_borrow_data()?).map_err(Into::into)
    }
}

pub enum PoolConfig<'a, 'b> {
    Stake {
        vault_authority_acc_info: &'a AccountInfo<'b>,
        registrar_acc_info: &'a AccountInfo<'b>,
        token_program_acc_info: &'a AccountInfo<'b>,
    },
    TransferStakeIntent,
}

pub fn parse_pools<'a, 'b>(
    cfg: PoolConfig<'a, 'b>,
    mut acc_infos: &mut dyn std::iter::Iterator<Item = &'a AccountInfo<'b>>,
    is_mega: bool,
) -> Result<(PoolApi<'a, 'b>, PoolApi<'a, 'b>), RegistryError> {
    let acc_infos = &mut acc_infos;

    // Program ids.
    let pool_program_id_acc_info = next_account_info(acc_infos)?;
    let retbuf_program_acc_info = next_account_info(acc_infos)?;
    // Main pool (for instruction).
    let pool_acc_info = next_account_info(acc_infos)?;
    let pool_tok_mint_acc_info = next_account_info(acc_infos)?;
    let mut pool_asset_vault_acc_infos = vec![next_account_info(acc_infos)?];
    if is_mega {
        pool_asset_vault_acc_infos.push(next_account_info(acc_infos)?);
    }
    let pool_vault_authority_acc_info = next_account_info(acc_infos)?;
    let retbuf_acc_info = next_account_info(acc_infos)?;
    // Alt pool.
    let alt_pool_acc_info = next_account_info(acc_infos)?;
    let alt_pool_tok_mint_acc_info = next_account_info(acc_infos)?;
    let mut alt_pool_asset_vault_acc_infos = vec![next_account_info(acc_infos)?];
    if !is_mega {
        alt_pool_asset_vault_acc_infos.push(next_account_info(acc_infos)?);
    }
    let alt_pool_vault_authority_acc_info = next_account_info(acc_infos)?;
    let alt_retbuf_acc_info = next_account_info(acc_infos)?;
    // Instruction specific params.
    let mut user_pool_tok_acc_info = None;
    let mut user_asset_tok_acc_info = None;
    let mut user_tok_auth_acc_info = None;
    let mut vault_authority_acc_info = None;
    let mut registrar_acc_info = None;
    let mut token_program_acc_info = None;
    if let PoolConfig::Stake {
        vault_authority_acc_info: _vault_authority_acc_info,
        registrar_acc_info: _registrar_acc_info,
        token_program_acc_info: _token_program_acc_info,
    } = cfg
    {
        user_pool_tok_acc_info = Some(next_account_info(acc_infos)?);
        // TODO: if mega then vec
        user_asset_tok_acc_info = Some(next_account_info(acc_infos)?);
        user_tok_auth_acc_info = Some(next_account_info(acc_infos)?);
        vault_authority_acc_info = Some(_vault_authority_acc_info);
        registrar_acc_info = Some(_registrar_acc_info);
        token_program_acc_info = Some(_token_program_acc_info);
    }

    let (pool, mega_pool) = {
        let pool = PoolApi {
            pool_program_id_acc_info,
            pool_acc_info,
            pool_tok_mint_acc_info,
            pool_asset_vault_acc_infos,
            pool_vault_authority_acc_info,
            retbuf_acc_info,
            retbuf_program_acc_info,
            user_pool_tok_acc_info,
            user_asset_tok_acc_info,
            user_tok_auth_acc_info,
            vault_authority_acc_info,
            registrar_acc_info,
            token_program_acc_info,
        };
        let alt_pool = PoolApi {
            pool_program_id_acc_info: pool_program_id_acc_info,
            pool_acc_info: alt_pool_acc_info,
            pool_tok_mint_acc_info: alt_pool_tok_mint_acc_info,
            pool_asset_vault_acc_infos: alt_pool_asset_vault_acc_infos,
            pool_vault_authority_acc_info: alt_pool_vault_authority_acc_info,
            retbuf_acc_info: alt_retbuf_acc_info,
            retbuf_program_acc_info: retbuf_program_acc_info,
            user_pool_tok_acc_info: None,
            user_asset_tok_acc_info: None,
            user_tok_auth_acc_info: None,
            vault_authority_acc_info: None,
            registrar_acc_info: None,
            token_program_acc_info: None,
        };
        if is_mega {
            (alt_pool, pool)
        } else {
            (pool, alt_pool)
        }
    };

    Ok((pool, mega_pool))
}
