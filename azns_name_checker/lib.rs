#![cfg_attr(not(feature = "std"), no_std)]

pub use self::azns_name_checker::{NameChecker, NameCheckerRef};

extern crate alloc;

#[ink::contract]
mod azns_name_checker {
    use crate::azns_name_checker::Error::{
        NameContainsDisallowedCharacters, NameTooLong, NameTooShort,
    };
    // use alloc::string::String;
    use ink::prelude::string::String;
    use ink::prelude::vec::Vec;
    use ink::storage::Mapping;

    type Min = u64;
    type Max = u64;
    type LowerBound = String;
    type UpperBound = String;

    #[ink(storage)]
    pub struct NameChecker {
        index: u64,
        allowed_unicode_ranges: Mapping<String, Vec<(LowerBound, UpperBound)>>,
        allowed_length: (Min, Max),
    }

    pub type Result<T> = core::result::Result<T, Error>;

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        NameTooShort,
        NameTooLong,
        NameContainsDisallowedCharacters,
    }

    impl NameChecker {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                index: 0,
                allowed_unicode_ranges: Mapping::default(),
                allowed_length: (0, 0),
            }
        }

        #[ink(message)]
        pub fn is_name_allowed(&self, domain: String) -> Result<bool> {
            /* Check length */
            let min = self.allowed_length.0;
            let max = self.allowed_length.1;
            // let length_ok = match domain.len() as u64 {
            //     min..=max => Ok(true),
            //     0..=min => Err(NameTooShort),
            //     _ => Err(NameTooLong),
            // };

            /* Allowed Unicode Ranges only */
            // TODO Replace with iterator
            // let allowed = domain.chars().all(|char| {
            // self.allowed_unicode_ranges
            //     .iter()
            //     .map(|range| {
            //         let lower = range.0;
            //         let upper = range.1;
            //         match char {
            //             lower..=upper => true,
            //             _ => false,
            //         }
            //     })
            //     .collect()
            // });

            let allowed = true;

            match allowed {
                true => Ok(true),
                false => Err(NameContainsDisallowedCharacters),
            }
        }
    }
}

pub fn get_domain_price(domain: &str) -> u128 {
    match domain.len() {
        4 => 160 ^ 12,
        _ => 5 ^ 12,
    }
}
