use near_sdk::{serde::{self, Serialize, Deserialize}, borsh::{self, BorshDeserialize, BorshSerialize}, PanicOnDefault, json_types::ValidAccountId, AccountId};

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize)]
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

#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MetadataReference {
    pub reference: String,
    pub reference_hash: Vec<u8>,
}
