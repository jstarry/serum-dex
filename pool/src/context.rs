use std::convert::TryInto;

use solana_sdk;
use solana_sdk::account_info::next_account_info;
use solana_sdk::program_option::COption;
use solana_sdk::program_pack::Pack;
use solana_sdk::{account_info::AccountInfo, info, program_error::ProgramError, pubkey::Pubkey};
use spl_token::state::{Account as TokenAccount, Mint};

use serum_pool_schema::{Address, Basket, PoolRequestInner, PoolState};

pub struct PoolContext<'a, 'b> {
    pub program_id: &'a Pubkey,

    /// Account that holds the `PoolState`.
    pub pool_account: &'a AccountInfo<'b>,

    /// Token mint account for the pool token.
    pub pool_token_mint: &'a AccountInfo<'b>,
    /// Token accounts for each of the assets owned by the pool.
    pub pool_vault_accounts: &'a [AccountInfo<'b>],
    /// Signer for `pool_token_mint` and `pool_vault_accounts`.
    pub pool_authority: &'a AccountInfo<'b>,

    /// Present for `GetBasket` requests.
    pub retbuf: Option<RetbufAccounts<'a, 'b>>,

    /// Present for `Transact` requests.
    pub user_accounts: Option<UserAccounts<'a, 'b>>,

    /// Present for `Transact` requests.
    pub spl_token_program: Option<&'a AccountInfo<'b>>,

    /// Accounts from `PoolState::account_params`. Present for `GetBasket` and `Transact` requests.
    pub account_params: Option<&'a [AccountInfo<'b>]>,

    /// Any additional accounts that were passed into the instruction.
    pub custom_accounts: &'a [AccountInfo<'b>],
}

pub struct UserAccounts<'a, 'b> {
    pub pool_token_account: &'a AccountInfo<'b>,
    pub asset_accounts: &'a [AccountInfo<'b>],
    pub authority: &'a AccountInfo<'b>,
}

pub struct RetbufAccounts<'a, 'b> {
    pub account: &'a AccountInfo<'b>,
    pub program: &'a AccountInfo<'b>,
}

impl<'a, 'b> PoolContext<'a, 'b> {
    pub fn new(
        program_id: &'a Pubkey,
        accounts: &'a [AccountInfo<'b>],
        state: &PoolState,
        request: &PoolRequestInner,
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.into_iter();

        let pool_account = next_account_info(accounts_iter)?;
        let pool_token_mint = next_account_info(accounts_iter)?;
        let pool_vault_accounts = next_account_infos(accounts_iter, state.assets.len())?;
        let pool_authority = next_account_info(accounts_iter)?;
        let mut context = PoolContext {
            program_id,
            pool_account,
            pool_token_mint,
            pool_vault_accounts,
            pool_authority,
            retbuf: None,
            user_accounts: None,
            spl_token_program: None,
            account_params: None,
            custom_accounts: &[],
        };

        check_account_address(context.pool_token_mint, &state.pool_token_mint)?;
        for (asset_info, vault_account) in
            state.assets.iter().zip(context.pool_vault_accounts.iter())
        {
            info!("checking pool context token account");
            check_account_address(vault_account, &asset_info.vault_address)?;
            check_token_account(
                vault_account,
                asset_info.mint.as_ref(),
                state.vault_signer.as_ref(),
            )?;
        }
        check_account_address(context.pool_authority, &state.vault_signer)?;

        match request {
            PoolRequestInner::GetBasket(_) => {
                let retbuf_account = next_account_info(accounts_iter)?;
                let retbuf_program = next_account_info(accounts_iter)?;
                context.retbuf = Some(RetbufAccounts::new(retbuf_account, retbuf_program)?);
                context.account_params = Some(next_account_infos(
                    accounts_iter,
                    state.account_params.len(),
                )?);
            }
            PoolRequestInner::Transact(_) => {
                let pool_token_account = next_account_info(accounts_iter)?;
                let asset_accounts = next_account_infos(accounts_iter, state.assets.len())?;
                let authority = next_account_info(accounts_iter)?;
                context.user_accounts = Some(UserAccounts::new(
                    state,
                    pool_token_account,
                    asset_accounts,
                    authority,
                )?);
                context.spl_token_program = Some(next_account_info(accounts_iter)?);
                context.account_params = Some(next_account_infos(
                    accounts_iter,
                    state.account_params.len(),
                )?);
            }
            _ => {}
        }

        if let Some(spl_token_program) = context.spl_token_program {
            if spl_token_program.key != &spl_token::ID {
                info!("Incorrect spl-token program ID");
                return Err(ProgramError::InvalidArgument);
            }
        }

        if let Some(account_params) = context.account_params {
            for (param_desc, account_info) in state.account_params.iter().zip(account_params.iter())
            {
                check_account_address(account_info, &param_desc.address)?;
            }
        }

        context.custom_accounts = accounts_iter.as_slice();

        Ok(context)
    }
}

impl<'a, 'b> UserAccounts<'a, 'b> {
    pub fn new(
        state: &PoolState,
        pool_token_account: &'a AccountInfo<'b>,
        asset_accounts: &'a [AccountInfo<'b>],
        authority: &'a AccountInfo<'b>,
    ) -> Result<Self, ProgramError> {
        info!("checking user accounts token account");
        check_token_account(
            pool_token_account,
            state.pool_token_mint.as_ref(),
            authority.key,
        )?;
        for (asset_info, account) in state.assets.iter().zip(asset_accounts.iter()) {
            info!("checking user accounts token account");
            check_token_account(account, asset_info.mint.as_ref(), authority.key)?;
        }
        Ok(UserAccounts {
            pool_token_account,
            asset_accounts,
            authority,
        })
    }
}

impl<'a, 'b> RetbufAccounts<'a, 'b> {
    pub fn new(
        account: &'a AccountInfo<'b>,
        program: &'a AccountInfo<'b>,
    ) -> Result<Self, ProgramError> {
        if account.owner != program.key {
            info!("Incorrect retbuf account owner");
            return Err(ProgramError::IncorrectProgramId);
        }
        Ok(RetbufAccounts { account, program })
    }
}

impl<'a, 'b> PoolContext<'a, 'b> {
    /// Total number of pool tokens currently in existence.
    pub fn total_pool_tokens(&self) -> Result<u64, ProgramError> {
        let mint = Mint::unpack(&self.pool_token_mint.try_borrow_data()?)?;
        Ok(mint.supply)
    }

    /// For each token in `PoolState::assets`, the quantity of that token currently
    /// held by the pool.
    pub fn pool_asset_quantities(&self) -> Result<Vec<u64>, ProgramError> {
        self.pool_vault_accounts
            .iter()
            .map(|account| -> Result<u64, ProgramError> {
                let token_account = TokenAccount::unpack(&account.try_borrow_data()?)?;
                Ok(token_account.amount)
            })
            .collect()
    }

    pub fn get_simple_basket(&self, pool_tokens_requested: u64) -> Result<Basket, ProgramError> {
        let total_pool_tokens = self.total_pool_tokens()?;
        let basket_quantities: Option<Vec<i64>> = self
            .pool_asset_quantities()?
            .iter()
            .map(|pool_quantity| {
                pool_quantity
                    .checked_mul(pool_tokens_requested)?
                    .checked_div(total_pool_tokens)?
                    .try_into()
                    .ok()
            })
            .collect();
        // TODO: add an error type
        Ok(Basket {
            quantities: basket_quantities.ok_or(ProgramError::Custom(123))?,
        })
    }
}

fn next_account_infos<'a, 'b: 'a>(
    iter: &mut std::slice::Iter<'a, AccountInfo<'b>>,
    count: usize,
) -> Result<&'a [AccountInfo<'b>], ProgramError> {
    let accounts = iter.as_slice();
    if accounts.len() < count {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let (accounts, remaining) = accounts.split_at(count);
    *iter = remaining.into_iter();
    Ok(accounts)
}

fn check_account_address(account: &AccountInfo, address: &Address) -> Result<(), ProgramError> {
    if account.key != address.as_ref() {
        info!("Incorrect account address");
        return Err(ProgramError::InvalidArgument);
    }
    Ok(())
}

fn check_token_account(
    account: &AccountInfo,
    mint: &Pubkey,
    authority: &Pubkey,
) -> Result<(), ProgramError> {
    if account.owner != &spl_token::ID {
        info!("Account now owned by spl-token program");
        return Err(ProgramError::IncorrectProgramId);
    }
    let token_account = TokenAccount::unpack(&account.try_borrow_data()?)?;
    if &token_account.mint != mint {
        info!("Incorrect mint");
        return Err(ProgramError::InvalidArgument);
    }
    if &token_account.owner != authority && token_account.delegate != COption::Some(*authority) {
        info!(&format!(
            "Incorrect spl-token account owner {:?}, {:?}",
            token_account, authority
        ));
        return Err(ProgramError::InvalidArgument);
    }
    Ok(())
}
