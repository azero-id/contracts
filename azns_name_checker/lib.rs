#![cfg_attr(not(feature = "std"), no_std)]

pub use self::azns_name_checker::{NameChecker, NameCheckerRef};

extern crate alloc;

#[ink::contract]
mod azns_name_checker {
    use crate::azns_name_checker::Error::{
        NameContainsDisallowedCharacters, NameTooLong, NameTooShort,
    };
    use alloc::vec::Vec;

    type Min = usize;
    type Max = usize;
    type LowerBound = char;
    type UpperBound = char;

    #[ink(storage)]
    pub struct NameChecker {
        allowed_unicode_ranges: Vec<(LowerBound, UpperBound)>,
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
        pub fn new() -> Self {}

        #[ink(message)]
        pub fn is_name_allowed(&self, domain: &str) -> Result<bool> {
            /* Check length */
            let min = self.allowed_length.0;
            let max = self.allowed_length.1;
            match domain.len() {
                min..=max => true,
                0..min => Err(NameTooShort),
                _ => Err(NameTooLong),
            }

            /* Allowed Unicode Ranges only */
            let allowed = domain.chars().all(|char| {
                self.allowed_unicode_ranges
                    .iter()
                    .map(|range| {
                        let lower = range.0;
                        let upper = range.1;
                        match char {
                            lower..=upper => true,
                            _ => false,
                        }
                    })
                    .collect()
            });

            match allowed {
                true => Ok(true),
                false => Err(NameContainsDisallowedCharacters),
            }
        }
    }
}
