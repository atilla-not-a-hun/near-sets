use near_sdk::env;
use uint::construct_uint;

use crate::Contract;

pub(crate) fn assert_1_yocto() {
    // TODO: in sep function
    assert_eq!(env::attached_deposit(), 1, "Expected an attached deposit of 1");
}

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}
