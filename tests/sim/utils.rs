use std::convert::TryFrom;

use defi::DeFiContract;
use fungible_token::ContractContract as FtContract;
use deployer_contract::ContractContract as DeployerContract;
use near_sdk::AccountId;
use token_set_fungible_token::{ContractContract as TokenSetContract, TokenWithRatioValid};

use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::serde_json::json;
use near_sdk_sim::{
    deploy, init_simulator, to_yocto, ContractAccount, UserAccount, DEFAULT_GAS, STORAGE_AMOUNT,
};

// Load in contract bytes at runtime
near_sdk_sim::lazy_static_include::lazy_static_include_bytes! {
    TOKEN_SET_WASM_BYTES => "res/token_set_fungible_token.wasm",
    FT_WASM_BYTES => "res/fungible_token.wasm",
    DEFI_WASM_BYTES => "res/defi.wasm",
    DEPLOY_WASM_BYTES => "res/deployer_contract.wasm",
}

const TOKEN_SET_ID: &str = "token-set";
const DEFI_ID: &str = "defi";
const DEPLOY_ID: &str = "deploy";

// Register the given `user` with FT contract
pub fn register_user(
    ft_ids: &Vec<AccountId>,
    user: &near_sdk_sim::UserAccount,
    register_set_ft: bool,
) {
    user.call(
        TOKEN_SET_ID.to_string(),
        "accounts_storage_deposit",
        &json!({
            "account_id": user.valid_account_id()
        })
        .to_string()
        .into_bytes(),
        near_sdk_sim::DEFAULT_GAS / 2,
        near_sdk::env::storage_byte_cost() * 1_000, // attached deposit
    )
    .assert_success();
    if register_set_ft {
        user.call(
            TOKEN_SET_ID.to_string(),
            "storage_deposit",
            &json!({
                "account_id": user.valid_account_id()
            })
            .to_string()
            .into_bytes(),
            near_sdk_sim::DEFAULT_GAS / 2,
            near_sdk::env::storage_byte_cost() * 125, // attached deposit
        )
        .assert_success();
    }
    ft_ids.iter().for_each(|ft_id| {
        user.call(
            ft_id.to_string(),
            "storage_deposit",
            &json!({
                "account_id": user.valid_account_id()
            })
            .to_string()
            .into_bytes(),
            near_sdk_sim::DEFAULT_GAS / 2,
            near_sdk::env::storage_byte_cost() * 125, // attached deposit
        )
        .assert_success();
    });
}

// pub fn init_no_macros(initial_balance: u128) -> (UserAccount, UserAccount, UserAccount) {
//     let root = init_simulator(None);

//     let ft = root.deploy(&FT_WASM_BYTES, FT_ID.into(), STORAGE_AMOUNT);

//     ft.call(
//         FT_ID.into(),
//         "new_default_meta",
//         &json!({
//             "owner_id": root.valid_account_id(),
//             "total_supply": U128::from(initial_balance),
//         })
//         .to_string()
//         .into_bytes(),
//         DEFAULT_GAS / 2,
//         0, // attached deposit
//     )
//     .assert_success();

//     let alice = root.create_user("alice".to_string(), to_yocto("100"));
//     register_user(&alice);

//     (root, ft, alice)
// }

pub fn init_with_macros(
    ratios: Vec<u32>,
    platform_fee: Option<u128>,
    owner_fee: Option<u128>,
    init_token_supply: u128,
) -> (
    UserAccount,
    UserAccount,
    ContractAccount<TokenSetContract>,
    ContractAccount<DeFiContract>,
    ContractAccount<DeployerContract>,
    Vec<ContractAccount<FtContract>>,
    UserAccount,
) {
    let root = init_simulator(None);
    let name = "YOUR MOM TOKEN".to_string();
    let symbol = "YR MOM".to_string();

    let ft_ids: Vec<String> = (0..ratios.len()).map(|i| format!("ft-{}", i)).collect();

    let ft_contracts: Vec<ContractAccount<FtContract>> = (0..ratios.len())
        .map(|i| {
            let ft = deploy!(
                // Contract Proxy
                contract: FtContract,
                // Contract account id
                contract_id: ft_ids[i],
                // Bytes of contract
                bytes: &FT_WASM_BYTES,
                // User deploying the contract,
                signer_account: root,
                // init method
                init_method: new_default_meta(
                    root.valid_account_id(),
                   U128(init_token_supply)
                )
            );
            ft
        })
        .collect();

    let ratios: Vec<TokenWithRatioValid> = ft_contracts
        .iter()
        .enumerate()
        .map(|(i, ft_c)| TokenWithRatioValid {
            token_id: ValidAccountId::try_from(ft_c.account_id()).unwrap(),
            ratio: ratios[i],
        })
        .collect();

    let owner_bob = root.create_user("owner_bob".to_string(), to_yocto("100"));

    let deployer_contract = deploy!(
        contract: DeployerContract,
        contract_id: DEPLOY_ID,
        bytes: &DEPLOY_WASM_BYTES,
        signer_account: root,
        init_method: new()
    );
    // TODO
    // uses default values for deposit and gas
    let token_set = deploy!(
        // Contract Proxy
        contract: TokenSetContract,
        // Contract account id
        contract_id: TOKEN_SET_ID,
        // Bytes of contract
        bytes: &TOKEN_SET_WASM_BYTES,
        // User deploying the contract,
        signer_account: root,
        // init method
        init_method: new_default_meta(
            owner_bob.valid_account_id(),
            name,
            symbol,
            None,
            ratios,
            U128::from(platform_fee.unwrap_or(0)),
            root.valid_account_id(),
            U128::from(owner_fee.unwrap_or(0)),
            None,
            None
        )
    );
    let alice = root.create_user("alice".to_string(), to_yocto("100"));
    register_user(&ft_ids, &alice, true);

    // No need to register the platform or owner with the ft as this is done automatically on
    // contract initialization
    register_user(&ft_ids, &root, false);
    register_user(&ft_ids, &owner_bob, false);
    register_user(&ft_ids, &token_set.user_account, false);

    let defi = deploy!(
        contract: DeFiContract,
        contract_id: DEFI_ID,
        bytes: &DEFI_WASM_BYTES,
        signer_account: root,
        init_method: new(
            token_set.valid_account_id()
        )
    );

    (root, owner_bob, token_set, defi, deployer_contract, ft_contracts, alice)
}
