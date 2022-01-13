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

use near_account::{Account, AccountDeposits, Accounts, NearAccounts, NewInfo};
// To conserve gas, efficient serialization is achieved through Borsh (http://borsh.io/)
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, Vector};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::serde_json::json;
use near_sdk::{
    assert_one_yocto, env, near_bindgen, setup_alloc, AccountId, Balance, Promise, PromiseResult,
};
use near_sdk::{log, Gas};

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
        let deposit_for_contract: u128 = 2 * 10_u128.pow(24);
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

    #[payable]
    pub fn deploy_contract_code(&mut self, account_id: ValidAccountId) {
        let account_id = account_id.into();
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

        let prom = Promise::new(account_id.clone())
            .create_account()
            .transfer(self.deposit_for_contract)
            .add_full_access_key(env::signer_account_pk())
            .deploy_contract(include_bytes!("../../res/token_set_fungible_token.wasm").to_vec());
        prom.then(
            Promise::new(env::current_account_id()).function_call(
                b"resolve_contract_deploy".to_vec(),
                json!({
                    "caller": caller.clone(),
                    "contract_id": account_id.clone()
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
    pub fn resolve_contract_deploy(&mut self, caller: AccountId, contract_id: AccountId) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_v) => {}
            PromiseResult::Failed => {
                log!("Registering contract {} for caller {} failed", &contract_id, &caller);
                // Remove the contract from the vec
                let mut account = self.accounts.get_account_checked(&caller);
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
