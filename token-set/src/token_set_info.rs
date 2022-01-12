use near_internal_balances_plugin::SudoInternalBalanceFungibleToken;
use std::collections::HashSet;

use near_sdk::{collections::Vector, env, AccountId, Balance};

use crate::{utils::U256, Contract, FeeReceiver, SetInfo, TokenWithRatio, TokenWithRatioValid};

const FEE_DENOMINATOR: u128 = 1_000_000_000_000_000;

impl SetInfo {
    pub(crate) fn new(set_ratios: Vec<TokenWithRatioValid>, set_initial_fee: FeeReceiver) -> Self {
        // TODO: check each token_id ratio unique....
        if set_ratios.len() == 0 {
            panic!("Expected at least one token in the set");
        }

        let mut token_ids: HashSet<AccountId> = HashSet::default();

        let mut ratios = Vector::new(b"set-ratio".to_vec());

        for ratio in set_ratios {
            let not_present = token_ids.insert(ratio.token_id.clone().to_string());
            if !not_present {
                panic!("Each token in the ratio must be unique");
            }
            ratios.push(&TokenWithRatio { token_id: ratio.token_id.into(), ratio: ratio.ratio });
        }
        if set_initial_fee.owner_fee > FEE_DENOMINATOR
            || set_initial_fee.platform_fee > FEE_DENOMINATOR
        {
            panic!("Expected the fees to be less than the fee denominator of {}", FEE_DENOMINATOR);
        }
        if set_initial_fee.owner_fee + set_initial_fee.platform_fee > FEE_DENOMINATOR {
            panic!(
                "Expected the sum of fees to be less than the fee denominator of {}",
                FEE_DENOMINATOR
            );
        }
        Self { ratios, fee: set_initial_fee }
    }
}

impl Contract {
    pub(crate) fn on_burn(&mut self, account_id: AccountId, amount: Balance) {
        for i in 0..self.set_info.ratios.len() {
            let ratio = &self.set_info.ratios.get(i).unwrap();
            self.increase_balance(&account_id, &ratio.token_id, ratio.ratio as u128 * amount);
        }
    }

    pub(crate) fn unwrap_token(&mut self, amount: u128) {
        let account_id = env::predecessor_account_id();
        self.token.internal_withdraw(&account_id, amount);
        self.on_burn(account_id, amount);
    }

    pub(crate) fn change_owner_fee(&mut self, new_fee: u128) {
        if !self.set_info.fee.updatable {
            panic!("Cannot update a token set fee unless the fee property is marked initially updatable")
        }
        self.set_info.fee.owner_fee = new_fee;
    }

    /// Decrease the balances of the underlying tokens and wrap the tokens.
    /// Also, send the apportioned fee amount
    ///
    /// return the amount wrapped and given to the wrapper
    pub(crate) fn wrap_internal(&mut self, owner: &AccountId, amount: Option<Balance>) -> Balance {
        // TODO: hmmmmm... should this be the predecessor or the signer???
        let caller = env::predecessor_account_id();
        let max_amount_wrapped = self.get_max_amount(&caller);
        let amount_wrap = amount.unwrap_or(max_amount_wrapped);
        // TODO: add test for this
        if amount_wrap > max_amount_wrapped {
            panic!(
                "Maximum amount that can be wrapped is {}, tried wrapping {}",
                max_amount_wrapped, amount_wrap
            );
        }
        let owner_inrcr = (U256::from(amount_wrap) * U256::from(self.set_info.fee.owner_fee)
            / U256::from(FEE_DENOMINATOR))
        .as_u128();
        let platform_incr = (U256::from(amount_wrap) * U256::from(self.set_info.fee.platform_fee)
            / U256::from(FEE_DENOMINATOR))
        .as_u128();

        let amount_wrap_caller = amount_wrap - owner_inrcr - platform_incr;

        // Do the internal deposits
        self.token.internal_deposit(&caller, amount_wrap_caller);
        self.token.internal_deposit(&owner, owner_inrcr);
        self.token.internal_deposit(&self.set_info.fee.platform_id, platform_incr);

        self.decrease_potentials(amount_wrap, &caller);

        amount_wrap
    }

    fn decrease_potentials(&mut self, amount_out: Balance, account_id: &AccountId) {
        for i in 0..self.set_info.ratios.len() {
            let ratio = &self.set_info.ratios.get(i).unwrap();
            self.subtract_balance(&account_id, &ratio.token_id, (ratio.ratio as u128) * amount_out)
        }
    }

    fn get_max_amount(&self, account_id: &AccountId) -> Balance {
        let mut min = u128::MAX;
        for i in 0..self.set_info.ratios.len() {
            let ratio = &self.set_info.ratios.get(i).unwrap();
            let bal = self.get_ft_balance_internal(account_id, &ratio.token_id);

            let amount_out = bal / (ratio.ratio as u128);
            if amount_out < min {
                min = amount_out;
            }
        }
        min
    }
}
