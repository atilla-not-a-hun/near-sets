// Built from https://github.com/near-examples/contract-that-deploys-contracts-rs
/*
* This is an example of a Rust smart contract with two simple, symmetric functions:
*
* 1. set_greeting: accepts a greeting, such as "howdy", and records it for the user (account_id)
*    who sent the request
* 2. get_greeting: accepts an account_id and returns the greeting saved for it, defaulting to
*    "Hello"

* Learn more about writing NEAR smart contracts with Rust:
* https://github.com/near/near-sdk-rs
*
*/

use near_account::{Account, AccountDeposits, AccountInfoTrait, Accounts, NearAccounts, NewInfo};
// To conserve gas, efficient serialization is achieved through Borsh (http://borsh.io/)
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, Vector};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::serde_json::json;
use near_sdk::{
    assert_one_yocto, env, near_bindgen, setup_alloc, AccountId, Balance, Promise, PromiseOrValue,
    PromiseResult,
};
use near_sdk::{log, Gas};
use shared::{MetadataReference, TokenWithRatioValid};

setup_alloc!();
const BASE_GAS: Gas = 5_000_000_000_000;

#[derive(BorshDeserialize, BorshSerialize)]
pub struct AccountInfo {
    deployed_contracts: Vector<AccountId>,
}

impl NewInfo for AccountInfo {
    fn default_from_account_id(account_id: AccountId) -> Self {
        Self { deployed_contracts: Vector::new(format!("{}-b", account_id).as_bytes()) }
    }
}

impl AccountInfoTrait for AccountInfo {}

// Structs in Rust are similar to other languages, and may include impl keyword as shown below
// Note: the names of the structs are not important when calling the smart contract, but the function names are
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, NearAccounts)]
pub struct Contract {
    deposit_for_contract: Balance,
    accounts: Accounts<AccountInfo>,
}

impl Default for Contract {
    fn default() -> Self {
        let deposit_for_contract: u128 = 10 * 10_u128.pow(24);
        let contract = Self { accounts: Accounts::new(), deposit_for_contract };
        contract
    }
}

impl Contract {}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_all_sets(&self) -> Vec<AccountId> {
        let mut sets = vec![];
        for (_deployer, set_deployer_account) in self.accounts.accounts.iter() {
            for tok in set_deployer_account.info.deployed_contracts.iter() {
                sets.push(tok);
            }
        }
        sets
    }

    #[payable]
    pub fn deploy_contract_code(
        &mut self,
        contract_account_prefix: String,
        owner_id: ValidAccountId,
        name: String,
        symbol: String,
        icon_url: Option<String>,
        set_ratios: Vec<TokenWithRatioValid>,
        platform_fee: U128,
        platform_id: ValidAccountId,
        owner_fee: U128,
        updatable_fee: Option<bool>,
        metadata_reference: Option<MetadataReference>,
    ) {
        assert_one_yocto();
        let account_id = format!("{}.{}", contract_account_prefix, env::current_account_id());
        let caller = env::predecessor_account_id();

        let mut account = self.accounts.get_account_checked(&caller);
        let available_near = account.get_available_near();
        assert!(
            available_near >= self.deposit_for_contract,
            "Expected at least {} Yocto Near",
            self.deposit_for_contract
        );
        account.near_used_for_storage += self.deposit_for_contract;

        account.info.deployed_contracts.push(&account_id);
        self.accounts.insert_account_check_storage(&caller, &mut account);

        // These al are one 'action' and if a failure occurs, the transfer should be reverted
        let prom = Promise::new(account_id.clone())
            .create_account()
            .transfer(self.deposit_for_contract)
            .add_full_access_key(env::signer_account_pk())
            .deploy_contract(include_bytes!("../../res/token_set_fungible_token.wasm").to_vec())
            .function_call(
                b"new_default_meta".to_vec(),
                json!({
                        "owner_id": owner_id,
                        "name": name,
                        "symbol": symbol,
                        "icon_url": icon_url,
                        "set_ratios": set_ratios,
                        "platform_fee": platform_fee,
                        "platform_id": platform_id,
                        "owner_fee": owner_fee,
                        "updatable_fee": updatable_fee,
                        "metadata_reference": metadata_reference,
                })
                .to_string()
                .as_bytes()
                .to_vec(),
                0,
                BASE_GAS * 10,
            );

        prom.then(
            Promise::new(env::current_account_id()).function_call(
                b"resolve_contract_deploy".to_vec(),
                json!({
                    "caller": caller.clone(),
                    "contract_id": account_id.clone(),
                    "owner_id": owner_id,
                    "name": name,
                    "symbol": symbol,
                    "icon_url": icon_url,
                    "set_ratios": set_ratios,
                    "platform_fee": platform_fee,
                    "platform_id": platform_id,
                    "owner_fee": owner_fee,
                    "updatable_fee": updatable_fee,
                    "metadata_reference": metadata_reference,

                })
                .to_string()
                .as_bytes()
                .to_vec(),
                0,
                BASE_GAS * 2,
            ),
        );
    }

    #[private]
    pub fn resolve_contract_deploy(
        &mut self,
        caller: AccountId,
        contract_id: AccountId,
    ) -> PromiseOrValue<()> {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_v) => PromiseOrValue::Value(()),
            PromiseResult::Failed => {
                log!("Registering contract {} for caller {} failed", &contract_id, &caller);
                // Remove the contract from the vec
                let account = self.accounts.get_account(&caller);
                match account {
                    Some(mut account) => {
                        // Refund the near transferred to the new account
                        account.near_used_for_storage -= self.deposit_for_contract;

                        // Remove the contract from the list of tokens
                        let contract = account
                            .info
                            .deployed_contracts
                            .iter()
                            .enumerate()
                            .find(|(i, contr)| contr == &contract_id);
                        if contract.is_none() {
                            log!("Expected to find contract {}")
                        } else {
                            let (idx, _) = contract.unwrap();
                            account.info.deployed_contracts.swap_remove(idx as u64);
                            self.accounts.insert_account_check_storage(&caller, &mut account);
                        }
                    }
                    None => {
                        log!("The account was deleted")
                    }
                };
                PromiseOrValue::Value(())
            }
        }
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 *
 * To run from contract directory:
 * cargo test -- --nocapture
 *
 * From project root, to run in combination with frontend tests:
 * yarn test
 *
 */
