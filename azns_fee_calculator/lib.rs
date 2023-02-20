#![cfg_attr(not(feature = "std"), no_std)]

use ink::prelude::string::String;
use ink::prelude::vec::Vec;

#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum Error {
    /// Caller not allowed to call privileged calls.
    NotAdmin,
    /// Given registration duration is not allowed.
    InvalidDuration,
    /// Zero length name not allowed
    ZeroLength,
}

pub type Result<T> = core::result::Result<T, Error>;

/// Evaluate `$x:expr` and if not true return `Err($y:expr)`.
///
/// Used as `ensure!(expression_to_ensure, expression_to_return_on_false)`.
macro_rules! ensure {
    ( $condition:expr, $error:expr $(,)? ) => {{
        if !$condition {
            return ::core::result::Result::Err(::core::convert::Into::into($error));
        }
    }};
}

pub use self::azns_fee_calculator::{AznsFeeCalculator, AznsFeeCalculatorRef};

#[ink::contract]
mod azns_fee_calculator {
    use super::*;
    use ink::storage::traits::ManualKey;
    use ink::storage::Mapping;

    // Length of name
    pub type Length = u8;

    #[ink(storage)]
    pub struct AznsFeeCalculator {
        /// Account allowed to modify the variables
        admin: AccountId,
        /// Maximum registration duration allowed (in years)
        max_registration_duration: u8,
        /// If no specific price found for a given length then this will be used
        common_price: Balance,
        /// Set price for specific name length
        price_by_length: Mapping<Length, Balance, ManualKey<100>>,
    }

    impl AznsFeeCalculator {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            max_registration_duration: u8,
            common_price: Balance,
            price_points: Vec<(Length, Balance)>,
        ) -> Self {
            let mut contract = Self {
                admin,
                max_registration_duration,
                common_price,
                price_by_length: Default::default(),
            };

            price_points.iter().for_each(|(length, price)| {
                contract.price_by_length.insert(length, price);
            });

            contract
        }

        #[ink(message)]
        pub fn get_name_price(&self, name: String, duration: u8) -> Result<Balance> {
            ensure!(
                1 <= duration && duration <= self.max_registration_duration,
                Error::InvalidDuration
            );
            ensure!(name.len() != 0, Error::ZeroLength);

            let base_price = self
                .price_by_length
                .get(name.len() as Length)
                .unwrap_or(self.common_price);

            let mut price = 0;
            for year in 1..=duration {
                price += (year as u128) * base_price;
            }

            Ok(price)
        }

        #[ink(message)]
        pub fn get_common_price(&self) -> Balance {
            self.common_price
        }

        #[ink(message)]
        pub fn get_price_by_length(&self, len: Length) -> Option<Balance> {
            self.price_by_length.get(&len)
        }

        #[ink(message)]
        pub fn set_max_registration_duration(&mut self, duration: u8) -> Result<()> {
            self.ensure_admin()?;
            self.max_registration_duration = duration;
            Ok(())
        }

        #[ink(message)]
        pub fn set_common_price(&mut self, common_price: Balance) -> Result<()> {
            self.ensure_admin()?;
            self.common_price = common_price;
            Ok(())
        }

        #[ink(message)]
        pub fn set_prices_by_length(
            &mut self,
            price_points: Vec<(Length, Option<Balance>)>,
        ) -> Result<()> {
            self.ensure_admin()?;

            price_points.iter().for_each(|(length, price)| {
                if let Some(price) = price {
                    self.price_by_length.insert(length, price);
                } else {
                    self.price_by_length.remove(length);
                }
            });

            Ok(())
        }

        #[ink(message)]
        pub fn set_admin(&mut self, account: AccountId) -> Result<()> {
            self.ensure_admin()?;
            self.admin = account;
            Ok(())
        }

        #[ink(message)]
        pub fn upgrade_contract(&mut self, code_hash: [u8; 32]) -> Result<()> {
            self.ensure_admin()?;

            ink::env::set_code_hash(&code_hash).unwrap_or_else(|err| {
                panic!(
                    "Failed to `set_code_hash` to {:?} due to {:?}",
                    code_hash, err
                )
            });
            ink::env::debug_println!("Switched code hash to {:?}.", code_hash);

            Ok(())
        }

        fn ensure_admin(&self) -> Result<()> {
            ensure!(self.env().caller() == self.admin, Error::NotAdmin);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
    }
}
