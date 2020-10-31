use serum_common::client::rpc;
use serum_common::pack::*;
use serum_pool_schema::Basket;
use serum_pool_schema::PoolState;
use serum_registry::accounts::{pending_withdrawal, vault, Entity, Member, Registrar};
use serum_registry::client::{Client as InnerClient, ClientError as InnerClientError};
use solana_client_gen::prelude::*;
use solana_client_gen::solana_sdk::instruction::AccountMeta;
use solana_client_gen::solana_sdk::pubkey::Pubkey;
use solana_client_gen::solana_sdk::signature::Signature;
use solana_client_gen::solana_sdk::signature::{Keypair, Signer};
use spl_token::state::Account as TokenAccount;
use std::convert::Into;
use thiserror::Error;

mod inner;

pub struct Client {
    inner: InnerClient,
}

impl Client {
    pub fn new(inner: InnerClient) -> Self {
        Self { inner }
    }

    pub fn initialize(&self, req: InitializeRequest) -> Result<InitializeResponse, ClientError> {
        let InitializeRequest {
            registrar_authority,
            withdrawal_timelock,
            deactivation_timelock_premium,
            mint,
            mega_mint,
            reward_activation_threshold,
            pool_program_id,
            pool_token_decimals,
        } = req;
        let (
            tx,
            registrar,
            nonce,
            pool,
            pool_vault_signer_nonce,
            mega_pool,
            mega_pool_vault_signer_nonce,
        ) = inner::initialize(
            &self.inner,
            &mint,
            &mega_mint,
            &registrar_authority,
            withdrawal_timelock,
            deactivation_timelock_premium,
            reward_activation_threshold,
            &pool_program_id,
            pool_token_decimals,
        )?;
        Ok(InitializeResponse {
            tx,
            registrar,
            nonce,
            pool,
            pool_vault_signer_nonce,
            mega_pool,
            mega_pool_vault_signer_nonce,
        })
    }

    pub fn create_entity(
        &self,
        req: CreateEntityRequest,
    ) -> Result<CreateEntityResponse, ClientError> {
        let CreateEntityRequest {
            node_leader,
            registrar,
        } = req;
        let (tx, entity) = inner::create_entity_derived(&self.inner, registrar, node_leader)?;
        Ok(CreateEntityResponse { tx, entity })
    }

    pub fn update_entity(
        &self,
        req: UpdateEntityRequest,
    ) -> Result<UpdateEntityResponse, ClientError> {
        let UpdateEntityRequest {
            entity,
            leader,
            new_leader,
            registrar,
        } = req;
        let accounts = [
            AccountMeta::new(entity, false),
            AccountMeta::new_readonly(leader.pubkey(), true),
            AccountMeta::new_readonly(registrar, false),
        ];
        let tx = self.inner.update_entity_with_signers(
            &[leader, self.payer()],
            &accounts,
            new_leader,
        )?;
        Ok(UpdateEntityResponse { tx })
    }

    pub fn create_member(
        &self,
        req: CreateMemberRequest,
    ) -> Result<CreateMemberResponse, ClientError> {
        let CreateMemberRequest {
            entity,
            beneficiary,
            delegate,
            registrar,
            watchtower,
            watchtower_dest,
        } = req;
        let (tx, member) = inner::create_member_derived(
            &self.inner,
            registrar,
            entity,
            beneficiary,
            delegate,
            watchtower,
            watchtower_dest,
        )?;
        Ok(CreateMemberResponse { tx, member })
    }

    pub fn deposit(&self, req: DepositRequest) -> Result<DepositResponse, ClientError> {
        let DepositRequest {
            member,
            beneficiary,
            entity,
            depositor,
            depositor_authority,
            registrar,
            amount,
            pool_program_id,
            mega,
        } = req;
        let r = self.registrar(&registrar)?;
        let delegate = false;
        let mut accounts = vec![
            // Whitelist relay interface,
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummy.
            AccountMeta::new(depositor, false),
            AccountMeta::new(depositor_authority.pubkey(), true),
            AccountMeta::new_readonly(spl_token::ID, false),
            // Program specific.
            AccountMeta::new(member, false),
            AccountMeta::new_readonly(beneficiary.pubkey(), true),
            AccountMeta::new(entity, false),
            AccountMeta::new_readonly(registrar, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
            AccountMeta::new(r.vault, false),
            AccountMeta::new(r.mega_vault, false),
        ];
        let (pool_accs, _) = self.common_pool_accounts(pool_program_id, registrar, false)?;
        accounts.extend_from_slice(&pool_accs);
        let signers = [self.payer(), beneficiary, depositor_authority];

        let tx = self
            .inner
            .deposit_with_signers(&signers, &accounts, amount, mega, delegate)?;

        Ok(DepositResponse { tx })
    }

    pub fn withdraw(&self, req: WithdrawRequest) -> Result<WithdrawResponse, ClientError> {
        let WithdrawRequest {
            member,
            beneficiary,
            entity,
            depositor,
            mega,
            registrar,
            amount,
            pool_program_id,
        } = req;
        let r = self.registrar(&registrar)?;
        let vault_acc = rpc::get_token_account::<TokenAccount>(self.inner.rpc(), &r.vault)?;
        let delegate = false;
        let mut accounts = vec![
            // Whitelist relay interface.
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummy.
            AccountMeta::new(depositor, false),
            AccountMeta::new(vault_acc.owner, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            // Program specific.
            AccountMeta::new(member, false),
            AccountMeta::new_readonly(beneficiary.pubkey(), true),
            AccountMeta::new(entity, false),
            AccountMeta::new_readonly(registrar, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
            AccountMeta::new(r.vault, false),
            AccountMeta::new(r.mega_vault, false),
        ];
        let (pool_accs, _) = self.common_pool_accounts(pool_program_id, registrar, mega)?;
        accounts.extend_from_slice(&pool_accs);
        let signers = [self.payer(), beneficiary];

        let tx = self
            .inner
            .withdraw_with_signers(&signers, &accounts, amount, mega, delegate)?;

        Ok(WithdrawResponse { tx })
    }

    pub fn stake(&self, req: StakeRequest) -> Result<StakeResponse, ClientError> {
        let StakeRequest {
            member,
            beneficiary,
            entity,
            registrar,
            pool_token_amount,
            pool_program_id,
            user_pool_token,
            mega,
        } = req;
        let r = self.registrar(&registrar)?;
        let mut depositor_assets = vec![r.vault];
        if mega {
            depositor_assets.push(r.mega_vault);
        }
        let vault_authority = self.vault_authority(&registrar)?;
        let (mut pool_accounts, depositor_pool_token) = self.stake_pool_accounts(
            pool_program_id,
            registrar,
            mega,
            depositor_assets,
            user_pool_token,
            beneficiary,
            vault_authority,
            false,
        )?;

        let mut accounts = vec![
            AccountMeta::new(member, false),
            AccountMeta::new_readonly(beneficiary.pubkey(), true),
            AccountMeta::new(entity, false),
            AccountMeta::new_readonly(registrar, false),
            AccountMeta::new_readonly(self.vault_authority(&registrar)?, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
        ];

        accounts.append(&mut pool_accounts);

        let signers = [self.payer(), beneficiary];

        let tx = self.inner.stake_with_signers(
            &signers,
            &accounts,
            pool_token_amount,
            mega,
            false, // Not a delegate.
        )?;

        Ok(StakeResponse {
            tx,
            depositor_pool_token,
        })
    }

    pub fn start_stake_withdrawal(
        &self,
        req: StartStakeWithdrawalRequest,
    ) -> Result<StartStakeWithdrawalResponse, ClientError> {
        let StartStakeWithdrawalRequest {
            registrar,
            member,
            entity,
            beneficiary,
            spt_amount,
            mega,
            user_pool_token,
            pool_program_id,
            user_token,
            user_mega_token,
        } = req;
        let pending_withdrawal = Keypair::generate(&mut OsRng);

        let r = self.registrar(&registrar)?;

        let mut accs = vec![
            AccountMeta::new(pending_withdrawal.pubkey(), false),
            AccountMeta::new(r.escrow.vault, false),
            AccountMeta::new(r.escrow.mega_vault, false),
            //
            AccountMeta::new(member, false),
            AccountMeta::new_readonly(beneficiary.pubkey(), true),
            AccountMeta::new(entity, false),
            //
            AccountMeta::new_readonly(registrar, false),
            AccountMeta::new(self.vault_authority(&registrar)?, false),
            //
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false),
            //
            AccountMeta::new(user_token, false),
        ];

        if mega {
            if let Some(user_mega_token) = user_mega_token {
                accs.push(AccountMeta::new(user_mega_token, false));
            }
        }

        let mut assets = vec![r.escrow.vault];
        if mega {
            assets.push(r.escrow.mega_vault);
        }
        let (mut pool_accounts, _) = self.stake_pool_accounts(
            pool_program_id,
            registrar,
            mega,
            assets,
            Some(user_pool_token),
            beneficiary,          // Not used since pool token is given.
            beneficiary.pubkey(), // Owner of the pool token to burn.
            true,
        )?;

        accs.append(&mut pool_accounts);

        let instructions = {
            let create_pending_withdrawal_instr = {
                let lamports = self
                    .rpc()
                    .get_minimum_balance_for_rent_exemption(*pending_withdrawal::SIZE as usize)
                    .map_err(InnerClientError::RpcError)?;
                system_instruction::create_account(
                    &self.payer().pubkey(),
                    &pending_withdrawal.pubkey(),
                    lamports,
                    *pending_withdrawal::SIZE,
                    self.program(),
                )
            };
            let start_stake_withdrawal_instr = serum_registry::instruction::start_stake_withdrawal(
                *self.program(),
                &accs,
                spt_amount,
                mega,
                false,
            );
            [
                create_pending_withdrawal_instr,
                start_stake_withdrawal_instr,
            ]
        };
        let tx = {
            let (recent_hash, _fee_calc) = self
                .rpc()
                .get_recent_blockhash()
                .map_err(|e| InnerClientError::RawError(e.to_string()))?;
            let signers = [self.payer(), beneficiary, &pending_withdrawal];
            Transaction::new_signed_with_payer(
                &instructions,
                Some(&self.payer().pubkey()),
                &signers,
                recent_hash,
            )
        };

        self.rpc()
            .send_and_confirm_transaction_with_spinner_and_config(
                &tx,
                self.inner.options().commitment,
                self.inner.options().tx,
            )
            .map_err(ClientError::RpcError)
            .map(|tx| StartStakeWithdrawalResponse { tx })
    }

    pub fn end_stake_withdrawal(
        &self,
        req: EndStakeWithdrawalRequest,
    ) -> Result<EndStakeWithdrawalResponse, ClientError> {
        let EndStakeWithdrawalRequest {
            registrar,
            member,
            entity,
            beneficiary,
            pending_withdrawal,
            user_token_receiver,
            user_mega_token_receiver,
        } = req;
        let r = self.registrar(&registrar)?;

        let accs = vec![
            // Whitelist relay interface.
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummy.
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummu .
            AccountMeta::new(self.vault_authority(&registrar)?, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            // Program specific.
            AccountMeta::new(pending_withdrawal, false),
            AccountMeta::new(r.escrow.vault, false),
            AccountMeta::new(r.escrow.mega_vault, false),
            //
            AccountMeta::new(member, false),
            AccountMeta::new_readonly(beneficiary.pubkey(), true),
            AccountMeta::new(entity, false),
            //
            AccountMeta::new_readonly(registrar, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
            // User token accounts.
            AccountMeta::new(user_token_receiver, false),
            AccountMeta::new(user_mega_token_receiver, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummy.
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false), // Dummy.
        ];

        let instructions = [serum_registry::instruction::end_stake_withdrawal(
            *self.program(),
            &accs,
            false, // Delegate (not).
        )];

        let tx = {
            let (recent_hash, _fee_calc) = self
                .rpc()
                .get_recent_blockhash()
                .map_err(|e| InnerClientError::RawError(e.to_string()))?;
            let signers = [self.payer(), beneficiary];
            Transaction::new_signed_with_payer(
                &instructions,
                Some(&self.payer().pubkey()),
                &signers,
                recent_hash,
            )
        };

        self.rpc()
            .send_and_confirm_transaction_with_spinner_and_config(
                &tx,
                self.inner.options().commitment,
                self.inner.options().tx,
            )
            .map_err(ClientError::RpcError)
            .map(|tx| EndStakeWithdrawalResponse { tx })
    }

    pub fn common_pool_accounts(
        &self,
        pool_program_id: Pubkey,
        registrar: Pubkey,
        mega: bool,
    ) -> Result<(Vec<AccountMeta>, Pubkey), ClientError> {
        let r = self.registrar(&registrar)?;
        let (mut pool, pool_mint) = {
            let pool_state = self.stake_pool(&registrar)?;
            assert!(pool_state.assets.len() == 1);
            let pool_asset_vault = pool_state.assets[0].clone().vault_address.into();
            let retbuf = {
                let dummy_basket = Basket {
                    quantities: vec![0],
                };
                rpc::create_account_rent_exempt(
                    self.rpc(),
                    self.payer(),
                    dummy_basket.size().expect("always serializes") as usize,
                    &spl_shared_memory::ID,
                )?
                .pubkey()
            };
            let pool_tok_mint = pool_state.pool_token_mint.into();
            let accs = vec![
                AccountMeta::new(r.pool, false),
                AccountMeta::new(pool_tok_mint, false),
                AccountMeta::new(pool_asset_vault, false),
                AccountMeta::new_readonly(pool_state.vault_signer.into(), false),
                AccountMeta::new(retbuf, false),
            ];
            (accs, pool_tok_mint)
        };
        let (mut mega_pool, mega_pool_mint) = {
            let pool_state = self.stake_mega_pool(&registrar)?;
            assert!(pool_state.assets.len() == 2);
            let pool_asset_vault_1 = pool_state.assets[0].clone().vault_address.into();
            let pool_asset_vault_2 = pool_state.assets[1].clone().vault_address.into();
            let retbuf = {
                let dummy_basket = Basket {
                    quantities: vec![0, 0],
                };
                rpc::create_account_rent_exempt(
                    self.rpc(),
                    self.payer(),
                    dummy_basket.size().expect("always serializes") as usize,
                    &spl_shared_memory::ID,
                )?
                .pubkey()
            };
            let pool_tok_mint = pool_state.pool_token_mint.into();
            let accs = vec![
                AccountMeta::new(r.mega_pool, false),
                AccountMeta::new(pool_tok_mint, false),
                AccountMeta::new(pool_asset_vault_1, false),
                AccountMeta::new(pool_asset_vault_2, false),
                AccountMeta::new_readonly(pool_state.vault_signer.into(), false),
                AccountMeta::new(retbuf, false),
            ];
            (accs, pool_tok_mint)
        };

        let main_pool_mint = {
            if mega {
                mega_pool_mint
            } else {
                pool_mint
            }
        };

        // Create the pool token account (to issue tokens) if none was provided.

        let mut pids_pool = vec![
            AccountMeta::new_readonly(pool_program_id, false),
            AccountMeta::new_readonly(spl_shared_memory::ID, false),
        ];
        let mut accounts = vec![];
        accounts.append(&mut pids_pool);
        accounts.append(&mut pool);
        accounts.append(&mut mega_pool);
        Ok((accounts, main_pool_mint))
    }
    pub fn stake_pool_accounts(
        &self,
        pool_program_id: Pubkey,
        registrar: Pubkey,
        mega: bool,
        basket_assets: Vec<Pubkey>,
        pool_token: Option<Pubkey>,
        pool_token_owner: &Keypair,
        authority: Pubkey,
        auth_signer: bool,
    ) -> Result<(Vec<AccountMeta>, Pubkey), ClientError> {
        let (mut accounts, main_pool_mint) =
            self.common_pool_accounts(pool_program_id, registrar, mega)?;
        let pool_token = {
            if let Some(spt) = pool_token {
                spt
            } else {
                rpc::create_token_account_with_delegate(
                    self.rpc(),
                    &main_pool_mint.into(),
                    &pool_token_owner.pubkey(),
                    Some((&authority, 0, pool_token_owner)),
                    self.payer(),
                )?
                .pubkey()
            }
        };
        // Stake specific accounts.
        accounts.push(AccountMeta::new(pool_token, false));
        accounts.extend_from_slice(
            basket_assets
                .iter()
                .map(|pk| AccountMeta::new(*pk, false))
                .collect::<Vec<_>>()
                .as_slice(),
        );
        accounts.push(AccountMeta::new_readonly(authority, auth_signer));

        Ok((accounts, pool_token))
    }
}

// Account accessors.
impl Client {
    pub fn registrar(&self, registrar: &Pubkey) -> Result<Registrar, ClientError> {
        rpc::get_account::<Registrar>(self.inner.rpc(), registrar).map_err(Into::into)
    }
    pub fn entity(&self, entity: &Pubkey) -> Result<Entity, ClientError> {
        rpc::get_account::<Entity>(self.inner.rpc(), entity).map_err(Into::into)
    }
    pub fn member(&self, member: &Pubkey) -> Result<Member, ClientError> {
        rpc::get_account::<Member>(self.inner.rpc(), &member).map_err(Into::into)
    }
    pub fn member_seed() -> &'static str {
        inner::member_seed()
    }
    pub fn vault_authority(&self, registrar: &Pubkey) -> Result<Pubkey, ClientError> {
        let r = self.registrar(registrar)?;
        Pubkey::create_program_address(&vault::signer_seeds(registrar, &r.nonce), self.program())
            .map_err(|_| ClientError::Any(anyhow::anyhow!("invalid vault authority")))
    }
    pub fn stake_intent_vault(&self, registrar: &Pubkey) -> Result<TokenAccount, ClientError> {
        let r = self.registrar(registrar)?;
        rpc::get_token_account::<TokenAccount>(self.inner.rpc(), &r.vault).map_err(Into::into)
    }
    pub fn stake_intent_mega_vault(&self, registrar: &Pubkey) -> Result<TokenAccount, ClientError> {
        let r = self.registrar(registrar)?;
        rpc::get_token_account::<TokenAccount>(self.inner.rpc(), &r.mega_vault).map_err(Into::into)
    }

    pub fn stake_pool(&self, registrar: &Pubkey) -> Result<PoolState, ClientError> {
        let r = self.registrar(registrar)?;
        rpc::get_account::<PoolState>(self.inner.rpc(), &r.pool).map_err(Into::into)
    }

    pub fn stake_mega_pool(&self, registrar: &Pubkey) -> Result<PoolState, ClientError> {
        let r = self.registrar(registrar)?;
        rpc::get_account::<PoolState>(self.inner.rpc(), &r.mega_pool).map_err(Into::into)
    }

    pub fn stake_pool_asset_vault(&self, registrar: &Pubkey) -> Result<TokenAccount, ClientError> {
        let pool = self.stake_pool(registrar)?;
        if pool.assets.len() != 1 {
            return Err(ClientError::Any(anyhow::anyhow!("invalid asset length")));
        }
        rpc::get_token_account::<TokenAccount>(
            self.inner.rpc(),
            &pool.assets[0].vault_address.clone().into(),
        )
        .map_err(Into::into)
    }

    pub fn stake_mega_pool_asset_vaults(
        &self,
        registrar: &Pubkey,
    ) -> Result<(TokenAccount, TokenAccount), ClientError> {
        let pool = self.stake_mega_pool(registrar)?;
        if pool.assets.len() != 2 {
            return Err(ClientError::Any(anyhow::anyhow!("invalid asset length")));
        }
        let srm_vault = rpc::get_token_account::<TokenAccount>(
            self.inner.rpc(),
            &pool.assets[0].vault_address.clone().into(),
        )?;
        let msrm_vault = rpc::get_token_account::<TokenAccount>(
            self.inner.rpc(),
            &pool.assets[1].vault_address.clone().into(),
        )?;

        Ok((srm_vault, msrm_vault))
    }

    pub fn escrow_vaults(
        &self,
        registrar: &Pubkey,
    ) -> Result<(TokenAccount, TokenAccount), ClientError> {
        let r = self.registrar(registrar)?;
        let escrow = rpc::get_token_account::<TokenAccount>(self.inner.rpc(), &r.escrow.vault)?;
        let mega_escrow =
            rpc::get_token_account::<TokenAccount>(self.inner.rpc(), &r.escrow.vault)?;
        Ok((escrow, mega_escrow))
    }
}

impl ClientGen for Client {
    fn from_keypair_file(program_id: Pubkey, filename: &str, url: &str) -> anyhow::Result<Client> {
        Ok(Self::new(
            InnerClient::from_keypair_file(program_id, filename, url)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        ))
    }
    fn with_options(self, opts: RequestOptions) -> Client {
        Self::new(self.inner.with_options(opts))
    }
    fn rpc(&self) -> &RpcClient {
        self.inner.rpc()
    }
    fn payer(&self) -> &Keypair {
        self.inner.payer()
    }
    fn program(&self) -> &Pubkey {
        self.inner.program()
    }
}

pub struct InitializeRequest {
    pub registrar_authority: Pubkey,
    pub withdrawal_timelock: i64,
    pub deactivation_timelock_premium: i64,
    pub mint: Pubkey,
    pub mega_mint: Pubkey,
    pub reward_activation_threshold: u64,
    pub pool_program_id: Pubkey,
    pub pool_token_decimals: u8,
}

pub struct InitializeResponse {
    pub tx: Signature,
    pub registrar: Pubkey,
    pub nonce: u8,
    pub pool: Pubkey,
    pub pool_vault_signer_nonce: u8,
    pub mega_pool: Pubkey,
    pub mega_pool_vault_signer_nonce: u8,
}

pub struct CreateEntityRequest<'a> {
    pub node_leader: &'a Keypair,
    pub registrar: Pubkey,
}

pub struct CreateEntityResponse {
    pub tx: Signature,
    pub entity: Pubkey,
}

pub struct UpdateEntityRequest<'a> {
    pub entity: Pubkey,
    pub leader: &'a Keypair,
    pub new_leader: Pubkey,
    pub registrar: Pubkey,
}

pub struct UpdateEntityResponse {
    pub tx: Signature,
}

pub struct CreateMemberRequest {
    pub entity: Pubkey,
    pub delegate: Pubkey,
    pub registrar: Pubkey,
    // TODO: take in keypair instead?
    pub beneficiary: Pubkey,
    pub watchtower: Pubkey,
    pub watchtower_dest: Pubkey,
}

pub struct CreateMemberResponse {
    pub tx: Signature,
    pub member: Pubkey,
}

pub struct StakeRequest<'a> {
    pub member: Pubkey,
    pub beneficiary: &'a Keypair,
    pub entity: Pubkey,
    pub registrar: Pubkey,
    pub pool_token_amount: u64,
    pub pool_program_id: Pubkey,
    pub user_pool_token: Option<Pubkey>,
    pub mega: bool,
}

pub struct StakeResponse {
    pub tx: Signature,
    pub depositor_pool_token: Pubkey,
}

pub struct DepositRequest<'a> {
    pub member: Pubkey,
    pub beneficiary: &'a Keypair,
    pub entity: Pubkey,
    pub depositor: Pubkey,
    pub depositor_authority: &'a Keypair,
    pub mega: bool,
    pub registrar: Pubkey,
    pub amount: u64,
    pub pool_program_id: Pubkey,
}

pub struct DepositResponse {
    pub tx: Signature,
}

pub struct WithdrawRequest<'a> {
    pub member: Pubkey,
    pub beneficiary: &'a Keypair,
    pub entity: Pubkey,
    pub depositor: Pubkey,
    pub mega: bool,
    pub registrar: Pubkey,
    pub amount: u64,
    pub pool_program_id: Pubkey,
}

pub struct WithdrawResponse {
    pub tx: Signature,
}

pub struct StartStakeWithdrawalRequest<'a> {
    pub registrar: Pubkey,
    pub member: Pubkey,
    pub entity: Pubkey,
    pub beneficiary: &'a Keypair,
    pub spt_amount: u64,
    pub mega: bool,
    pub user_pool_token: Pubkey,
    pub pool_program_id: Pubkey,
    pub user_token: Pubkey,
    pub user_mega_token: Option<Pubkey>,
}

pub struct StartStakeWithdrawalResponse {
    pub tx: Signature,
}

pub struct EndStakeWithdrawalRequest<'a> {
    pub registrar: Pubkey,
    pub member: Pubkey,
    pub entity: Pubkey,
    pub beneficiary: &'a Keypair,
    pub pending_withdrawal: Pubkey,
    pub user_token_receiver: Pubkey,
    pub user_mega_token_receiver: Pubkey,
}

pub struct EndStakeWithdrawalResponse {
    pub tx: Signature,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Client error {0}")]
    InnerError(#[from] InnerClientError),
    #[error("Error invoking rpc: {0}")]
    RpcError(#[from] solana_client::client_error::ClientError),
    #[error("Any error: {0}")]
    Any(#[from] anyhow::Error),
}
