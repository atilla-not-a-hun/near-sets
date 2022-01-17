use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
    serde::{self, Deserialize, Serialize},
    AccountId, PanicOnDefault,
};

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenWithRatioValid {
    pub token_id: ValidAccountId,
    pub ratio: u32,
}

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenWithRatio {
    pub token_id: AccountId,
    pub ratio: u32,
}

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct MetadataReference {
    pub reference: String,
    pub reference_hash: Vec<u8>,
}
