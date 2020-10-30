use rand::rngs::OsRng;
use serum_common::client::rpc;
use serum_common_tests::Genesis;
use serum_lockup::accounts::WhitelistEntry;
use serum_lockup_client::{
    ClaimRequest, Client as LockupClient, CreateVestingRequest,
    InitializeRequest as LockupInitializeRequest, LockedStakeIntentRequest,
    LockedStakeIntentWithdrawalRequest, WhitelistAddRequest,
};
use serum_registry_client::*;
use solana_client_gen::prelude::*;
use solana_client_gen::solana_sdk::pubkey::Pubkey;
use solana_client_gen::solana_sdk::signature::{Keypair, Signer};
use spl_token::state::Account as TokenAccount;

#[test]
fn lifecycle() {
    // First test initiailze.
    let genesis = serum_common_tests::genesis::<Client>();

    let Genesis {
        client,
        srm_mint,
        msrm_mint,
        mint_authority: _,
        god,
        god_msrm,
        god_balance_before: _,
        god_msrm_balance_before: _,
        god_owner,
    } = genesis;

    // Initialize the registrar.
    let withdrawal_timelock = 1234;
    let deactivation_timelock_premium = 1000;
    let reward_activation_threshold = 10;
    let registrar_authority = Keypair::generate(&mut OsRng);
    let stake_pid: Pubkey = std::env::var("TEST_STAKE_PROGRAM_ID")
        .unwrap()
        .parse()
        .unwrap();
    let InitializeResponse {
        registrar,
        nonce,
        pool_vault_signer_nonce,
        pool,
        ..
    } = client
        .initialize(InitializeRequest {
            registrar_authority: registrar_authority.pubkey(),
            withdrawal_timelock,
            deactivation_timelock_premium,
            mint: srm_mint.pubkey(),
            mega_mint: msrm_mint.pubkey(),
            reward_activation_threshold,
            pool_program_id: stake_pid,
            pool_token_decimals: 3,
        })
        .unwrap();
    // Verify initialization.
    {
        let registrar = client.registrar(&registrar).unwrap();
        assert_eq!(registrar.initialized, true);
        assert_eq!(registrar.authority, registrar_authority.pubkey());
    }

    // Initialize the lockup program, vesting account, and whitelist the
    // registrar so that we can stake locked srm.
    let (l_client, safe, vesting, vesting_beneficiary, safe_vault_authority) = {
        let l_pid: Pubkey = std::env::var("TEST_LOCKUP_PROGRAM_ID")
            .unwrap()
            .parse()
            .unwrap();
        let l_client = serum_common_tests::client_at::<LockupClient>(l_pid);
        // Initialize.
        let init_resp = l_client
            .initialize(LockupInitializeRequest {
                mint: srm_mint.pubkey(),
                authority: l_client.payer().pubkey(),
            })
            .unwrap();
        // Whitelist the registrar.
        l_client
            .whitelist_add(WhitelistAddRequest {
                authority: l_client.payer(),
                safe: init_resp.safe,
                entry: WhitelistEntry::new(*client.program(), Some(registrar), nonce),
            })
            .unwrap();
        // Whitelist the two staking pools.
        l_client
            .whitelist_add(WhitelistAddRequest {
                authority: l_client.payer(),
                safe: init_resp.safe,
                entry: WhitelistEntry::new(stake_pid, Some(pool), pool_vault_signer_nonce),
            })
            .unwrap();
        // TODO: whitelist the msrm pool.
        // Create vesting.
        let current_ts = client
            .rpc()
            .get_block_time(client.rpc().get_slot().unwrap())
            .unwrap();
        let deposit_amount = 1_000;
        let c_vest_resp = l_client
            .create_vesting(CreateVestingRequest {
                depositor: god.pubkey(),
                depositor_owner: &god_owner,
                safe: init_resp.safe,
                beneficiary: client.payer().pubkey(),
                end_ts: current_ts + 60,
                period_count: 10,
                deposit_amount,
            })
            .unwrap();
        // Claim vesting.
        l_client
            .claim(ClaimRequest {
                beneficiary: client.payer(),
                safe: init_resp.safe,
                vesting: c_vest_resp.vesting,
            })
            .unwrap();

        (
            l_client,
            init_resp.safe,
            c_vest_resp.vesting,
            client.payer(),
            init_resp.vault_authority,
        )
    };

    // Create entity.
    let node_leader = Keypair::generate(&mut OsRng);
    let node_leader_pubkey = node_leader.pubkey();
    let entity = {
        let CreateEntityResponse { tx: _, entity } = client
            .create_entity(CreateEntityRequest {
                node_leader: &node_leader,
                registrar,
            })
            .unwrap();
        let entity_acc = client.entity(&entity).unwrap();
        assert_eq!(entity_acc.leader, node_leader_pubkey);
        assert_eq!(entity_acc.initialized, true);
        assert_eq!(entity_acc.balances.spt_amount, 0);
        assert_eq!(entity_acc.balances.spt_mega_amount, 0);
        entity
    };

    // Update entity.
    {
        let new_leader = Pubkey::new_rand();
        let _ = client
            .update_entity(UpdateEntityRequest {
                entity,
                leader: &node_leader,
                new_leader,
                registrar,
            })
            .unwrap();

        let entity_account = client.entity(&entity).unwrap();
        assert_eq!(entity_account.leader, new_leader);
    }

    // CreateMember.
    let beneficiary = Keypair::generate(&mut OsRng);
    let member = {
        let CreateMemberResponse { tx: _, member } = client
            .create_member(CreateMemberRequest {
                entity,
                registrar,
                beneficiary: beneficiary.pubkey(),
                delegate: safe_vault_authority,
                watchtower: Pubkey::new_from_array([0; 32]),
                watchtower_dest: Pubkey::new_from_array([0; 32]),
            })
            .unwrap();

        let member_account = client.member(&member).unwrap();
        assert_eq!(member_account.initialized, true);
        assert_eq!(member_account.entity, entity);
        assert_eq!(member_account.beneficiary, beneficiary.pubkey());
        assert_eq!(member_account.books.delegate().owner, safe_vault_authority,);
        assert_eq!(member_account.books.main().balances.spt_amount, 0);
        assert_eq!(member_account.books.main().balances.spt_mega_amount, 0);
        member
    };

    // Stake intent.
    let god_acc = rpc::get_token_account::<TokenAccount>(client.rpc(), &god.pubkey()).unwrap();
    let god_balance_before = god_acc.amount;
    let stake_intent_amount = 100;
    {
        client
            .stake_intent(StakeIntentRequest {
                member,
                beneficiary: &beneficiary,
                entity,
                depositor: god.pubkey(),
                depositor_authority: &god_owner,
                mega: false,
                registrar,
                amount: stake_intent_amount,
                pool_program_id: stake_pid,
            })
            .unwrap();
        let vault = client.stake_intent_vault(&registrar).unwrap();
        assert_eq!(stake_intent_amount, vault.amount);
        let god_acc = rpc::get_token_account::<TokenAccount>(client.rpc(), &god.pubkey()).unwrap();
        assert_eq!(god_acc.amount, god_balance_before - stake_intent_amount);
    }

    // Stake intent withdrawal.
    {
        client
            .stake_intent_withdrawal(StakeIntentWithdrawalRequest {
                member,
                beneficiary: &beneficiary,
                entity,
                depositor: god.pubkey(),
                mega: false,
                registrar,
                amount: stake_intent_amount,
                pool_program_id: stake_pid,
            })
            .unwrap();
        let vault = client.stake_intent_vault(&registrar).unwrap();
        assert_eq!(0, vault.amount);
        let god_acc = rpc::get_token_account::<TokenAccount>(client.rpc(), &god.pubkey()).unwrap();
        assert_eq!(god_acc.amount, god_balance_before);
    }

    // Stake intent from lockup.
    let l_vault_amount = l_client.vault(&safe).unwrap().amount;
    {
        l_client
            .locked_stake_intent(LockedStakeIntentRequest {
                amount: stake_intent_amount,
                mega: false,
                registry_pid: *client.program(),
                registrar,
                member,
                entity,
                beneficiary: vesting_beneficiary,
                stake_beneficiary: &beneficiary,
                vesting,
                safe,
                pool_program_id: stake_pid,
            })
            .unwrap();
        let vault = client.stake_intent_vault(&registrar).unwrap();
        assert_eq!(stake_intent_amount, vault.amount);
        let l_vault = l_client.vault(&safe).unwrap();
        assert_eq!(l_vault_amount - stake_intent_amount, l_vault.amount);
    }

    // Stake intent withdrawal back to lockup.
    {
        l_client
            .locked_stake_intent_withdrawal(LockedStakeIntentWithdrawalRequest {
                amount: stake_intent_amount,
                mega: false,
                registry_pid: *client.program(),
                registrar,
                member,
                entity,
                beneficiary: vesting_beneficiary,
                stake_beneficiary: &beneficiary,
                vesting,
                safe,
                pool_program_id: stake_pid,
            })
            .unwrap();
        let vault = client.stake_intent_vault(&registrar).unwrap();
        assert_eq!(0, vault.amount);
        let l_vault = l_client.vault(&safe).unwrap();
        assert_eq!(l_vault_amount, l_vault.amount);
    }

    // Activate the node, depositing 1 MSRM.
    {
        client
            .stake_intent(StakeIntentRequest {
                member,
                beneficiary: &beneficiary,
                entity,
                depositor: god_msrm.pubkey(),
                depositor_authority: &god_owner,
                registrar,
                amount: 1,
                pool_program_id: stake_pid,
                mega: true,
            })
            .unwrap();
    }

    // Stake 1 MSRM.
    {
        let StakeResponse {
            tx: _,
            depositor_pool_token,
        } = client
            .stake(StakeRequest {
                registrar,
                entity,
                member,
                beneficiary: &beneficiary,
                depositor_pool_token: None,
                pool_token_amount: 1,
                pool_program_id: stake_pid,
                mega: true,
            })
            .unwrap();
        let user_pool_token: TokenAccount =
            rpc::get_token_account(client.rpc(), &depositor_pool_token).unwrap();
        assert_eq!(user_pool_token.amount, 1);
        assert_eq!(user_pool_token.owner, god_owner.pubkey());
        // TODO: force the staking pool token owner to be beneficiary?
        // assert_eq!(user_pool_token.owner, beneficiary.pubkey());
        let (srm_vault, msrm_vault) = client.stake_mega_pool_asset_vaults(&registrar).unwrap();
        assert_eq!(srm_vault.amount, 0);
        assert_eq!(msrm_vault.amount, 1);
    }

    // Stake intent more SRM.
    {
        client
            .stake_intent(StakeIntentRequest {
                member,
                beneficiary: &beneficiary,
                entity,
                depositor: god.pubkey(),
                depositor_authority: &god_owner,
                registrar,
                amount: stake_intent_amount,
                pool_program_id: stake_pid,
                mega: false,
            })
            .unwrap();
    }

    // Stake SRM.
    let user_pool_token = {
        let StakeResponse {
            tx: _,
            depositor_pool_token,
        } = client
            .stake(StakeRequest {
                registrar,
                entity,
                member,
                beneficiary: &beneficiary,
                depositor_pool_token: None,
                pool_token_amount: stake_intent_amount,
                pool_program_id: stake_pid,
                mega: false,
            })
            .unwrap();
        let user_pool_token: TokenAccount =
            rpc::get_token_account(client.rpc(), &depositor_pool_token).unwrap();
        assert_eq!(user_pool_token.amount, stake_intent_amount);
        assert_eq!(user_pool_token.owner, god_owner.pubkey());
        let pool_vault = client.stake_pool_asset_vault(&registrar).unwrap();
        assert_eq!(pool_vault.amount, stake_intent_amount);

        depositor_pool_token
    };

    // Stake withdrawal start.
    {
        let new_account = rpc::create_token_account(
            client.rpc(),
            &srm_mint.pubkey(),
            &god_owner.pubkey(),
            client.payer(),
        )
        .unwrap()
        .pubkey();
        client
            .start_stake_withdrawal(StartStakeWithdrawalRequest {
                registrar,
                entity,
                member,
                beneficiary: &beneficiary,
                spt_amount: stake_intent_amount,
                mega: false,
                user_assets: vec![new_account],
                user_pool_token,
                user_token_authority: &god_owner,
                pool_program_id: stake_pid,
            })
            .unwrap();
        let user_asset_token: TokenAccount =
            rpc::get_token_account(client.rpc(), &new_account).unwrap();
        assert_eq!(user_asset_token.amount, stake_intent_amount);

        let user_pool_token: TokenAccount =
            rpc::get_token_account(client.rpc(), &user_pool_token).unwrap();
        assert_eq!(user_pool_token.amount, 0);
        let pool_vault = client.stake_pool_asset_vault(&registrar).unwrap();
        assert_eq!(pool_vault.amount, 0);
    }

    // Stake Withdrawal end.
    {
        // todo
    }

    // Stake locked.
    {
        // todo
    }

    // Stake withdrawal locked.
    {
        // todo
    }

    // Entity switch.
    {
        // todo
    }
}
