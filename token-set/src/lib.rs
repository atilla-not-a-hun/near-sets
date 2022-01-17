/*!
Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use account_info::AccountInfo;
use near_account::{AccountInfoTrait, Accounts, NearAccounts};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_internal_balances_plugin::impl_near_balance_plugin;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, Vector};
use near_sdk::json_types::{Base64VecU8, ValidAccountId, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, Balance, PanicOnDefault, PromiseOrValue,
};
use shared::{MetadataReference, TokenWithRatio, TokenWithRatioValid};

mod account_info;
mod token_set_info;
mod utils;

near_sdk::setup_alloc!();

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
/// Contains the fees for minting tokens.
pub struct FeeReceiver {
    /// The fee for the owner of the token set
    owner_fee: u128,
    /// The fee for the "platform" which created the token set contract
    platform_fee: u128,
    /// The platform account to receive te token
    platform_id: AccountId,
    /// Whether the fee can be updated after instantiation
    updatable: bool,
}
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct SetInfo {
    ratios: Vector<TokenWithRatio>,
    fee: FeeReceiver,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, PanicOnDefault)]
#[serde(crate = "near_sdk::serde")]
pub struct SetMetadata {
    ft_metadata: FungibleTokenMetadata,
    set_fee: FeeReceiver,
    set_ratios: Vec<TokenWithRatio>,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, NearAccounts)]
pub struct Contract {
    owner_id: AccountId,
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    accounts: Accounts<AccountInfo>,
    set_info: SetInfo,
}

// Implement the internal balance traits
impl_near_balance_plugin!(Contract, accounts, AccountInfo, internal_balance);

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new_default_meta(
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
    ) -> Self {
        Self::new(
            owner_id,
            FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name: name,
                symbol: symbol,
                icon: icon_url,
                reference: metadata_reference.as_ref().map(|r| r.reference.clone()),
                reference_hash: metadata_reference
                    .as_ref()
                    .map(|r| Base64VecU8::from(r.reference_hash.clone())),
                decimals: 24,
            },
            set_ratios,
            FeeReceiver {
                platform_fee: platform_fee.0,
                owner_fee: owner_fee.0,
                platform_id: platform_id.to_string(),
                updatable: updatable_fee.unwrap_or(false),
            },
        )
    }

    #[payable]
    pub fn wrap(&mut self, amount: Option<U128>) {
        assert_one_yocto();
        self.wrap_internal(&self.owner_id.clone(), amount.map(|a| a.0));
    }

    #[payable]
    pub fn unwrap(&mut self, amount: U128) {
        assert_one_yocto();
        self.unwrap_token(amount.into())
    }

    #[payable]
    pub fn update_owner_fee(&mut self, new_fee: u128) {
        assert_one_yocto();
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Only the owner can update the fee"
        );

        self.change_owner_fee(new_fee);
    }

    // TODO: let's think about,
    // if there account was deleted that means we have to do something with the balance
    // maybe we j transfer to platform?
    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
        let platform_id = self.set_info.fee.platform_id.clone();
        self.on_burn(platform_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
        self.on_burn(account_id, amount);
    }
}

impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    pub fn new(
        owner_id: ValidAccountId,
        metadata: FungibleTokenMetadata,
        set_ratios: Vec<TokenWithRatioValid>,
        set_initial_fee: FeeReceiver,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        let owner = &String::from(owner_id.clone());
        let platform = &set_initial_fee.platform_id.clone();

        metadata.assert_valid();
        let numb_tokens = set_ratios.len();

        let mut this = Self {
            owner_id: owner_id.to_string(),
            token: FungibleToken::new(b"a".to_vec()),
            metadata: LazyOption::new(b"m".to_vec(), Some(&metadata)),
            accounts: Accounts::new(),
            set_info: SetInfo::new(set_ratios, set_initial_fee),
        };

        // Register the platform and owner with the token
        this.token.internal_register_account(owner);
        // Register the platform if it is different from the owner
        if platform != owner {
            this.token.internal_register_account(platform);
        }
        // Calculate the minimum account storage required for wrapping
        let account_storage_min = this.accounts.default_min_storage_bal
            + (numb_tokens as u128 * this.get_storage_cost_for_one_balance());

        this.accounts.default_min_storage_bal = account_storage_min;

        this
    }
}

near_contract_standards::impl_fungible_token_core!(Contract, token, on_tokens_burned);
near_contract_standards::impl_fungible_token_storage!(Contract, token, on_account_closed);

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

impl Contract {
    fn assert_owner(&self) {
        assert_eq!(
            self.owner_id,
            env::predecessor_account_id(),
            "Expected the caller to be the owner"
        );
    }
}

/// Metadata updating functions
#[near_bindgen]
impl Contract {
    pub fn set_metadata(&self) -> SetMetadata {
        SetMetadata {
            ft_metadata: self.metadata.get().unwrap(),
            set_fee: self.set_info.fee.clone(),
            set_ratios: self.set_info.ratios.to_vec(),
        }
    }

    pub fn update_metadata_reference(&mut self, new_reference: Option<MetadataReference>) {
        self.assert_owner();
        let mut metadata = self.metadata.get().unwrap();
        if let Some(new_reference) = new_reference {
            let reference = new_reference.reference;
            let reference_hash = new_reference.reference_hash;
            metadata.reference = Some(reference);
            metadata.reference_hash = Some(Base64VecU8::from(reference_hash));
        } else {
            metadata.reference = None;
            metadata.reference_hash = None;
        }
        self.metadata.set(&metadata);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::convert::TryFrom;

    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};
    use near_sdk::{MockedBlockchain, VMConfig};

    use super::*;

    fn get_context(predecessor_account_id: ValidAccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    fn register_user(
        contract: &mut Contract,
        context: &mut VMContextBuilder,
        account: ValidAccountId,
    ) {
        let context = context.attached_deposit(contract.storage_balance_bounds().min.0);
        testing_env!(context.build());
        contract.storage_deposit(Some(account.clone()), None);

        let cost_for_account = contract.accounts_storage_balance_bounds().min;
        println!("Cost for account: {}", cost_for_account.0);
        let context = context.attached_deposit(cost_for_account.0);
        testing_env!(context.build());
        contract.accounts_storage_deposit(Some(account.clone()), None);

        context.attached_deposit(1);
    }

    #[test]
    fn test_new() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let platform_id = accounts(4);
        let token_id = accounts(5);

        let contract = Contract::new_default_meta(
            accounts(2).into(),
            "YOUR MOM".to_string(),
            "YOUR MOM".to_string(),
            None,
            vec![TokenWithRatioValid { token_id, ratio: 1 }],
            0.into(),
            platform_id,
            0.into(),
            None,
            None,
        );
        testing_env!(context.is_view(true).build());
        assert_eq!(contract.ft_total_supply().0, 0);
        assert_eq!(contract.ft_balance_of(accounts(1)).0, 0);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    #[should_panic(expected = "Each token in the ratio must be unique")]
    fn test_non_unique_tokens() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new_default_meta(
            accounts(2).into(),
            "YOUR MOM".to_string(),
            "YOUR MOM".to_string(),
            None,
            vec![
                TokenWithRatioValid { token_id: accounts(0), ratio: 1 },
                TokenWithRatioValid { token_id: accounts(0), ratio: 1 },
            ],
            0.into(),
            accounts(1),
            0.into(),
            None,
            None,
        );
    }

    #[test]
    fn test_metadata_update() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let token_id = accounts(5);
        let mut contract = Contract::new_default_meta(
            accounts(1),
            "YY".to_string(),
            "YY".to_string(),
            None,
            vec![TokenWithRatioValid { token_id, ratio: 1 }],
            0.into(),
            ValidAccountId::try_from(format!("platform{}", 1)).unwrap(),
            0.into(),
            None,
            None,
        );
        let metadata_ref =
            MetadataReference { reference: "ref".to_string(), reference_hash: vec![] };
        contract.update_metadata_reference(Some(metadata_ref.clone()));
        let meta = contract.set_metadata().ft_metadata;
        assert_eq!(meta.reference.unwrap(), metadata_ref.reference);
        assert_eq!(meta.reference_hash.unwrap(), Base64VecU8::from(metadata_ref.reference_hash));

        contract.update_metadata_reference(None);

        let meta = contract.set_metadata().ft_metadata;
        assert!(meta.reference.as_ref().is_none());
        assert!(meta.reference_hash.as_ref().is_none());
    }

    #[test]
    /// Check that minimum storage linearly increases with the number of tokens
    /// in a set
    /// TODO: not working
    fn test_min_account_storage() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut prior_min_bal: u128 = 0;
        let mut expected_diff: u128 = 0;
        for i in 1..3 {
            let context = get_context(accounts(1));
            testing_env!(context.build());
            let token_ratios = (0..i)
                .map(|x| TokenWithRatioValid {
                    token_id: ValidAccountId::try_from(format!("account{}", x)).unwrap(),
                    ratio: 1,
                })
                .collect();
            println!("aa;{:?}", token_ratios);
            let contract = Contract::new_default_meta(
                ValidAccountId::try_from(format!("owner{}", i)).unwrap(),
                i.to_string(),
                i.to_string(),
                None,
                token_ratios,
                0.into(),
                ValidAccountId::try_from(format!("platform{}", i)).unwrap(),
                0.into(),
                None,
                None,
            );
            let storage_min = contract.accounts_storage_balance_bounds().min.0;

            if i == 2 {
                expected_diff = storage_min - prior_min_bal;
            } else if i > 2 {
                assert_eq!(storage_min - prior_min_bal, expected_diff);
            }
            prior_min_bal = storage_min;
        }
    }

    #[test]
    fn test_wrap_transfer() {
        let mut context = get_context(accounts(2));
        testing_env!(context.build());
        let platform_id = accounts(4);
        let token_id = accounts(5);
        let mut contract = Contract::new_default_meta(
            accounts(2).into(),
            "YOUR MOM".to_string(),
            "YOUR MOM".to_string(),
            None,
            vec![TokenWithRatioValid { token_id: token_id.clone(), ratio: 1 }],
            0.into(),
            platform_id,
            0.into(),
            None,
            None,
        );

        // Paying for account registration, aka storage deposit
        register_user(&mut contract, &mut context, accounts(1));

        let amount_transfer = 100;
        contract.increase_balance(
            &accounts(1).to_string(),
            &token_id.clone().to_string(),
            amount_transfer,
        );

        // Paying for account registration, aka storage deposit
        register_user(&mut contract, &mut context, accounts(2));

        // Paying for account registration, aka storage deposit
        register_user(&mut contract, &mut context, accounts(4));

        assert_eq!(
            contract
                .get_ft_balance_internal(&accounts(1).to_string(), &token_id.clone().to_string()),
            amount_transfer
        );

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(1))
            .build());
        // Paying for account registration, aka storage deposit

        contract.wrap(None);
        assert_eq!(
            contract
                .get_ft_balance_internal(&accounts(1).to_string(), &token_id.clone().to_string()),
            0
        );

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(1))
            .build());

        assert_eq!(contract.ft_balance_of(accounts(1)).0, amount_transfer);
        contract.ft_transfer(accounts(2), amount_transfer.into(), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert_eq!(contract.ft_balance_of(accounts(1)).0, (0));
        assert_eq!(contract.ft_balance_of(accounts(2)).0, amount_transfer);
    }
}
