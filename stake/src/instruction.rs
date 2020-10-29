use borsh::BorshSerialize;
use serum_pool_schema::{InitializePoolRequest, PoolAction, PoolRequest, PoolRequestInner};
use solana_client_gen::solana_sdk::instruction::{AccountMeta, Instruction};
use solana_client_gen::solana_sdk::pubkey::Pubkey;
use std::convert::TryInto;

pub fn initialize(
    program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vaults: Vec<&Pubkey>,
    pool_vault_authority: &Pubkey,
    registrar_vault_authority: &Pubkey,
    vault_signer_nonce: u8,
) -> Instruction {
    let assets_length = pool_asset_vaults
        .len()
        .try_into()
        .expect("assets must fit into u8");
    let mut accounts = vec![
        // Pool accounts.
        AccountMeta::new_readonly(*pool, false),
        AccountMeta::new_readonly(*pool_token_mint, false),
    ];
    for pool_asset_vault in pool_asset_vaults {
        accounts.push(AccountMeta::new_readonly(*pool_asset_vault, false));
    }
    accounts.append(&mut vec![
        AccountMeta::new_readonly(*pool_vault_authority, false),
        // Stake specific accounts.
        AccountMeta::new_readonly(*registrar_vault_authority, false),
    ]);
    let req = PoolRequest {
        tag: Default::default(),
        inner: PoolRequestInner::Initialize(InitializePoolRequest {
            vault_signer_nonce,
            assets_length,
            custom_state_length: 0,
        }),
    };
    Instruction {
        program_id: *program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}

pub fn get_basket(
    pool_program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vaults: Vec<&Pubkey>,
    pool_vault_authority: &Pubkey,
    retbuf: &Pubkey,
    retbuf_pid: &Pubkey,
    spt_amount: u64,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(*pool, false),
        AccountMeta::new(*pool_token_mint, false),
    ];
    for v in pool_asset_vaults {
        accounts.push(AccountMeta::new(*v, false));
    }
    accounts.extend_from_slice(&[
        AccountMeta::new_readonly(*pool_vault_authority, false),
        AccountMeta::new(*retbuf, false),
        AccountMeta::new_readonly(*retbuf_pid, false),
    ]);
    let req = PoolRequest {
        tag: Default::default(),
        // Note: create/redeem makes no difference here.
        inner: PoolRequestInner::GetBasket(PoolAction::Create(spt_amount)),
    };
    Instruction {
        program_id: *pool_program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}

pub fn transact(
    program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vaults: Vec<&Pubkey>,
    pool_vault_authority: &Pubkey,
    user_pool_token: &Pubkey,
    user_pool_asset_tokens: Vec<&Pubkey>,
    user_authority: &Pubkey,
    registry_signer: &Pubkey,
    action: PoolAction,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(*pool, false),
        AccountMeta::new(*pool_token_mint, false),
    ];
    accounts.extend_from_slice(
        &pool_asset_vaults
            .iter()
            .map(|v| AccountMeta::new(**v, false))
            .collect::<Vec<_>>(),
    );
    accounts.extend_from_slice(&[
        AccountMeta::new_readonly(*pool_vault_authority, false),
        AccountMeta::new(*user_pool_token, false),
    ]);
    accounts.extend_from_slice(
        &user_pool_asset_tokens
            .iter()
            .map(|t| AccountMeta::new(**t, false))
            .collect::<Vec<_>>(),
    );
    accounts.extend_from_slice(&[
        AccountMeta::new_readonly(*user_authority, true),
        AccountMeta::new_readonly(spl_token::ID, false),
        // Program specific accounts.
        AccountMeta::new_readonly(*registry_signer, true),
    ]);
    let req = PoolRequest {
        tag: Default::default(),
        inner: PoolRequestInner::Transact(action),
    };
    Instruction {
        program_id: *program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}
