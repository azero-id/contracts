#![cfg_attr(not(feature = "std"), no_std, no_main)]

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
    /// Zero price not allowed
    ZeroPrice,
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

pub use self::azns_fee_calculator::{FeeCalculator, FeeCalculatorRef};

#[zink::coating(Ownable2Step[
    Error = Error::NotAdmin
])]
#[zink::coating(Upgradable)]
#[ink::contract]
mod azns_fee_calculator {
    use super::*;
    use ink::storage::traits::ManualKey;
    use ink::storage::Mapping;

    // Length of name
    pub type Length = u8;

    #[ink(storage)]
    pub struct FeeCalculator {
        /// Account allowed to modify the variables
        admin: AccountId,
        /// Two-step ownership transfer AccountId
        pending_admin: Option<AccountId>,
        /// Maximum registration duration allowed (in years)
        max_registration_duration: u8,
        /// If no specific price found for a given length then this will be used
        common_price: Balance,
        /// Set price for specific name length
        price_by_length: Mapping<Length, Balance, ManualKey<100>>,
    }

    impl FeeCalculator {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            max_registration_duration: u8,
            common_price: Balance,
            price_points: Vec<(Length, Balance)>,
        ) -> Self {
            assert!(common_price > 0, "Zero price");

            let mut contract = Self {
                admin,
                pending_admin: None,
                max_registration_duration,
                common_price,
                price_by_length: Default::default(),
            };

            price_points.iter().for_each(|(length, price)| {
                assert!(price > &0, "Zero price");
                contract.price_by_length.insert(length, price);
            });

            contract
        }

        #[ink(message)]
        pub fn get_max_registration_duration(&self) -> u8 {
            self.max_registration_duration
        }

        // (base_price, premium): (Balance, Balance)
        #[ink(message)]
        pub fn get_name_price(&self, name: String, duration: u8) -> Result<(Balance, Balance)> {
            ensure!(
                1 <= duration && duration <= self.max_registration_duration,
                Error::InvalidDuration
            );
            ensure!(name.len() != 0, Error::ZeroLength);

            let base_price = self
                .price_by_length
                .get(name.len() as Length)
                .unwrap_or(self.common_price);

            let premium = (duration as u128 - 1) * base_price;

            Ok((base_price, premium))
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

            if common_price == 0 {
                return Err(Error::ZeroPrice);
            }
            self.common_price = common_price;

            Ok(())
        }

        #[ink(message)]
        pub fn set_prices_by_length(
            &mut self,
            price_points: Vec<(Length, Option<Balance>)>,
        ) -> Result<()> {
            self.ensure_admin()?;

            for (length, price) in &price_points {
                if let Some(price) = price {
                    if price == &0 {
                        return Err(Error::ZeroPrice);
                    }
                    self.price_by_length.insert(length, price);
                } else {
                    self.price_by_length.remove(length);
                }
            }

            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::test::*;
        use ink::env::DefaultEnvironment;

        fn default_accounts() -> DefaultAccounts<DefaultEnvironment> {
            ink::env::test::default_accounts::<DefaultEnvironment>()
        }

        fn get_test_fee_calculator() -> FeeCalculator {
            let contract_addr: AccountId = AccountId::from([0xFF as u8; 32]);
            set_callee::<DefaultEnvironment>(contract_addr);
            FeeCalculator::new(
                default_accounts().alice,
                3,
                6_u128 * 10_u128.pow(12),
                vec![
                    (3, 640_u128 * 10_u128.pow(12)),
                    (4, 160_u128 * 10_u128.pow(12)),
                ],
            )
        }

        #[ink::test]
        fn new_works() {
            let contract = get_test_fee_calculator();

            assert_eq!(contract.get_common_price(), 6_u128 * 10_u128.pow(12));
            assert_eq!(
                contract.get_price_by_length(3),
                Some(640_u128 * 10_u128.pow(12))
            );
            assert_eq!(
                contract.get_price_by_length(4),
                Some(160_u128 * 10_u128.pow(12))
            );
        }

        #[ink::test]
        fn get_name_price_works() {
            let contract = get_test_fee_calculator();

            assert_eq!(
                contract.get_name_price("".to_string(), 1),
                Err(Error::ZeroLength)
            );

            let name = "alice".to_string();

            // Duration: 0
            assert_eq!(
                contract.get_name_price(name.clone(), 0),
                Err(Error::InvalidDuration)
            );

            // Duration: 1
            assert_eq!(
                contract.get_name_price(name.clone(), 1),
                Ok((6_u128 * 10_u128.pow(12), 0))
            );

            // Duration: 2
            assert_eq!(
                contract.get_name_price(name.clone(), 2),
                Ok((6_u128 * 10_u128.pow(12), 6_u128 * 10_u128.pow(12)))
            );

            // Duration: 3
            assert_eq!(
                contract.get_name_price(name.clone(), 3),
                Ok((6_u128 * 10_u128.pow(12), 12_u128 * 10_u128.pow(12)))
            );

            // Duration: 4
            assert_eq!(
                contract.get_name_price(name.clone(), 4),
                Err(Error::InvalidDuration)
            );
        }

        #[ink::test]
        fn set_max_registration_duration_works() {
            let mut contract = get_test_fee_calculator();

            assert_eq!(contract.get_max_registration_duration(), 3);
            contract.set_max_registration_duration(5).unwrap();
            assert_eq!(contract.get_max_registration_duration(), 5);
        }

        #[ink::test]
        fn set_common_price_works() {
            let mut contract = get_test_fee_calculator();

            assert_eq!(contract.get_common_price(), 6_u128 * 10_u128.pow(12));
            contract.set_common_price(100).unwrap();
            assert_eq!(contract.get_common_price(), 100);
        }

        #[ink::test]
        fn set_price_by_length_works() {
            let mut contract = get_test_fee_calculator();

            contract
                .set_prices_by_length(vec![(2, Some(100)), (3, None)])
                .unwrap();
            assert_eq!(contract.get_price_by_length(2), Some(100));
            assert_eq!(contract.get_price_by_length(3), None);
        }

        #[ink::test]
        fn zero_price_check_works() {
            let mut contract = get_test_fee_calculator();

            assert_eq!(contract.set_common_price(0), Err(Error::ZeroPrice));
            assert_eq!(
                contract.set_prices_by_length(vec![(2, Some(0))]),
                Err(Error::ZeroPrice)
            );
        }

        #[ink::test]
        fn ownable_2_step_works() {
            let accounts = default_accounts();
            let mut contract = get_test_fee_calculator();

            assert_eq!(contract.get_admin(), accounts.alice);
            contract.transfer_ownership(Some(accounts.bob)).unwrap();

            assert_eq!(contract.get_admin(), accounts.alice);

            set_caller::<DefaultEnvironment>(accounts.bob);
            contract.accept_ownership().unwrap();
            assert_eq!(contract.get_admin(), accounts.bob);
        }

        #[ink::test]
        fn admin_op_checked() {
            let mut contract = get_test_fee_calculator();
            set_caller::<DefaultEnvironment>(default_accounts().bob);

            assert_eq!(
                contract.set_max_registration_duration(5),
                Err(Error::NotAdmin)
            );
            assert_eq!(contract.set_common_price(100), Err(Error::NotAdmin));
            assert_eq!(
                contract.set_prices_by_length(vec![(3, None)]),
                Err(Error::NotAdmin)
            );
            assert_eq!(
                contract.transfer_ownership(Some(default_accounts().bob)),
                Err(Error::NotAdmin)
            );
        }
    }
}
