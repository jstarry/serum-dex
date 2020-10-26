use borsh::BorshSerialize;
use serum_pool_schema::{InitializePoolRequest, PoolAction, PoolRequest, PoolRequestInner};
use solana_client_gen::solana_sdk::instruction::{AccountMeta, Instruction};
use solana_client_gen::solana_sdk::pubkey::Pubkey;

pub fn initialize(
    program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vault: &Pubkey,
    pool_vault_authority: &Pubkey,
    registrar_vault_authority: &Pubkey,
    vault_signer_nonce: u8,
) -> Instruction {
    let accounts = vec![
        // Pool accounts.
        AccountMeta::new_readonly(*pool, false),
        AccountMeta::new_readonly(*pool_token_mint, false),
        AccountMeta::new_readonly(*pool_asset_vault, false),
        AccountMeta::new_readonly(*pool_vault_authority, false),
        // Stake specific accounts.
        AccountMeta::new_readonly(*registrar_vault_authority, false),
    ];
    let req = PoolRequest {
        tag: Default::default(),
        inner: PoolRequestInner::Initialize(InitializePoolRequest {
            vault_signer_nonce,
            assets_length: 1,
            custom_state_length: 0,
        }),
    };
    Instruction {
        program_id: *program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}

pub fn creation(
    program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vault: &Pubkey,
    pool_vault_authority: &Pubkey,
    user_pool_token: &Pubkey,
    user_pool_asset_token: &Pubkey,
    user_authority: &Pubkey,
    registry_signer: &Pubkey,
    amount: u64,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*pool, false),
        AccountMeta::new(*pool_token_mint, false),
        AccountMeta::new(*pool_asset_vault, false),
        AccountMeta::new_readonly(*pool_vault_authority, false),
        AccountMeta::new(*user_pool_token, false),
        AccountMeta::new(*user_pool_asset_token, false),
        AccountMeta::new_readonly(*user_authority, true),
        AccountMeta::new_readonly(spl_token::ID, false),
        // Program specific accounts.
        AccountMeta::new_readonly(*registry_signer, true),
    ];
    let req = PoolRequest {
        tag: Default::default(),
        inner: PoolRequestInner::Transact(PoolAction::Create(amount)),
    };
    Instruction {
        program_id: *program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}

pub fn redemption(
    program_id: &Pubkey,
    pool: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_asset_vault: &Pubkey,
    pool_vault_authority: &Pubkey,
    user_pool_token: &Pubkey,
    user_pool_asset_token: &Pubkey,
    user_authority: &Pubkey,
    registry_signer: &Pubkey,
    amount: u64,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*pool, false),
        AccountMeta::new(*pool_token_mint, false),
        AccountMeta::new(*pool_asset_vault, false),
        AccountMeta::new_readonly(*pool_vault_authority, false),
        AccountMeta::new(*user_pool_token, false),
        AccountMeta::new(*user_pool_asset_token, false),
        AccountMeta::new_readonly(*user_authority, true),
        AccountMeta::new_readonly(spl_token::ID, false),
        // Program specific accounts.
        AccountMeta::new_readonly(*registry_signer, true),
    ];
    let req = PoolRequest {
        tag: Default::default(),
        inner: PoolRequestInner::Transact(PoolAction::Create(amount)),
    };
    Instruction {
        program_id: *program_id,
        accounts,
        data: req.try_to_vec().expect("PoolRequest serializes"),
    }
}
