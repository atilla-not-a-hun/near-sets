use near_account::NewInfo;
use near_internal_balances_plugin::BalanceInfo;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::UnorderedMap,
    AccountId, Balance,
};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct AccountInfo {
    pub internal_balance: UnorderedMap<AccountId, Balance>,
}

impl NewInfo for AccountInfo {
    fn default_from_account_id(account_id: AccountId) -> Self {
        Self { internal_balance: UnorderedMap::new(format!("{}-bals", account_id).as_bytes()) }
    }
}

