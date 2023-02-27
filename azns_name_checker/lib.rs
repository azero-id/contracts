#![cfg_attr(not(feature = "std"), no_std)]

pub use self::azns_name_checker::{NameChecker, NameCheckerRef};

extern crate alloc;
extern crate unicode_segmentation;

/// Contains the bounds of a Unicode range, with each bound representing a Unicode character
/// Used to check whether a certain character is allowed by specifying allowed ranges, such as a-z etc.
#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct UnicodeRange {
    pub lower: u32,
    pub upper: u32,
}

#[ink::contract]
mod azns_name_checker {
    use crate::UnicodeRange;
    use ink::prelude::string::String;
    use ink::prelude::vec;
    use ink::prelude::vec::Vec;

    type Min = u8;
    type Max = u8;

    #[ink(storage)]
    pub struct NameChecker {
        admin: AccountId,
        allowed_unicode_ranges: Vec<UnicodeRange>,
        disallowed_unicode_ranges_for_edges: Vec<UnicodeRange>,
        allowed_length: (Min, Max),
    }

    pub type Result<T> = core::result::Result<T, Error>;

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Caller not allowed to call privileged calls.
        NotAdmin,
        TooShort,
        TooLong,
        ContainsDisallowedCharacters,
    }

    impl NameChecker {
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            allowed_length: (u8, u8),
            allowed_unicode_ranges: Vec<UnicodeRange>,
            disallowed_unicode_ranges_for_edges: Vec<UnicodeRange>,
        ) -> Self {
            Self {
                admin,
                allowed_unicode_ranges,
                allowed_length,
                disallowed_unicode_ranges_for_edges,
            }
        }

        #[ink(message)]
        pub fn is_name_allowed(&self, domain: String) -> Result<()> {
            /* Check length */
            let (min, max) = self.allowed_length;
            let len = domain.chars().count() as u64;

            match len {
                l if l > max as u64 => return Err(Error::TooLong),
                l if l < min as u64 => return Err(Error::TooShort),
                _ => (),
            }

            /* Check edges */
            let edges = vec![
                domain.chars().next().unwrap(),
                domain.chars().rev().next().unwrap(),
            ];

            let illegal_edges = edges.iter().any(|char| {
                self.disallowed_unicode_ranges_for_edges
                    .iter()
                    .any(|range| {
                        let lower = range.lower;
                        let upper = range.upper;

                        lower <= *char as u32 && *char as u32 <= upper
                    })
            });

            if illegal_edges {
                return Err(Error::ContainsDisallowedCharacters);
            }

            /* Check whole name */
            let allowed = domain.chars().all(|char| {
                self.allowed_unicode_ranges.iter().any(|range| {
                    let lower = range.lower;
                    let upper = range.upper;

                    lower <= char as u32 && char as u32 <= upper
                })
            });

            match allowed {
                true => Ok(()),
                false => Err(Error::ContainsDisallowedCharacters),
            }
        }

        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
        }

        #[ink(message)]
        pub fn get_allowed_unicode_ranges(&self) -> Vec<UnicodeRange> {
            self.allowed_unicode_ranges.clone()
        }

        #[ink(message)]
        pub fn get_disallowed_unicode_ranges_for_edges(&self) -> Vec<UnicodeRange> {
            self.disallowed_unicode_ranges_for_edges.clone()
        }

        #[ink(message)]
        pub fn get_allowed_length(&self) -> (Min, Max) {
            self.allowed_length
        }

        #[ink(message)]
        pub fn set_allowed_unicode_ranges(&mut self, new_ranges: Vec<UnicodeRange>) -> Result<()> {
            self.ensure_admin()?;
            self.allowed_unicode_ranges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_disallowed_unicode_ranges_for_edges(
            &mut self,
            new_ranges: Vec<UnicodeRange>,
        ) -> Result<()> {
            self.ensure_admin()?;
            self.disallowed_unicode_ranges_for_edges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_allowed_length(&mut self, new_length: (Min, Max)) -> Result<()> {
            self.ensure_admin()?;
            self.allowed_length = new_length;
            Ok(())
        }

        #[ink(message)]
        pub fn set_admin(&mut self, account: AccountId) -> Result<()> {
            self.ensure_admin()?;
            self.admin = account;
            Ok(())
        }

        fn ensure_admin(&self) -> Result<()> {
            match self.env().caller() == self.admin {
                true => Ok(()),
                false => Err(Error::NotAdmin),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::azns_name_checker::*;
    use crate::azns_name_checker::Error;
    use crate::UnicodeRange;
    use ink::env::test::default_accounts;
    use ink::env::DefaultEnvironment;
    use ink::prelude::string::String;

    #[ink::test]
    fn checks_length() {
        let alice = default_accounts::<DefaultEnvironment>().alice;
        let checker = NameChecker::new(
            alice,
            (2, 5),
            vec![
                UnicodeRange {
                    lower: 'a' as u32,
                    upper: 'z' as u32,
                },
                UnicodeRange {
                    lower: '0' as u32,
                    upper: '9' as u32,
                },
            ],
            vec![],
        );

        let short_name = String::from("a");
        assert_eq!(checker.is_name_allowed(short_name), Err(Error::TooShort));

        let long_name = String::from("abcdef");
        assert_eq!(checker.is_name_allowed(long_name), Err(Error::TooLong));

        let ok_name = String::from("abcd");
        assert_eq!(checker.is_name_allowed(ok_name), Ok(()));
    }

    #[ink::test]
    fn checks_unicode_ranges() {
        let alice = default_accounts::<DefaultEnvironment>().alice;
        let checker = NameChecker::new(
            alice,
            (2, 5),
            vec![
                UnicodeRange {
                    lower: 'a' as u32,
                    upper: 'z' as u32,
                },
                UnicodeRange {
                    lower: '0' as u32,
                    upper: '9' as u32,
                },
                UnicodeRange {
                    lower: '-' as u32,
                    upper: '-' as u32,
                },
            ],
            vec![],
        );

        let allowed_name = String::from("abcd");
        assert_eq!(checker.is_name_allowed(allowed_name), Ok(()));

        let allowed_name_2 = String::from("abc-d");
        assert_eq!(checker.is_name_allowed(allowed_name_2), Ok(()));

        let bad_chars = String::from("***");
        assert_eq!(
            checker.is_name_allowed(bad_chars),
            Err(Error::ContainsDisallowedCharacters)
        );
    }

    #[ink::test]
    fn works_with_emojis() {
        let alice = default_accounts::<DefaultEnvironment>().alice;
        let checker = NameChecker::new(
            alice,
            (0, 99),
            vec![
                UnicodeRange {
                    lower: 'a' as u32,
                    upper: 'z' as u32,
                },
                UnicodeRange {
                    lower: '\u{1F600}' as u32, // 😀
                    upper: '\u{1F603}' as u32, // 😃
                },
                UnicodeRange {
                    lower: '🚀' as u32,
                    upper: '🚂' as u32,
                },
                /* Skin tones */
                UnicodeRange {
                    lower: '\u{1F44A}' as u32, // 👊
                    upper: '\u{1F44D}' as u32, // 👍
                },
            ],
            vec![],
        );

        let allowed_name = String::from("😁");
        assert_eq!(checker.is_name_allowed(allowed_name), Ok(()));

        let allowed_name_2 = String::from("🚁");
        assert_eq!(checker.is_name_allowed(allowed_name_2), Ok(()));

        let bad_chars = String::from("😅");
        assert_eq!(
            checker.is_name_allowed(bad_chars),
            Err(Error::ContainsDisallowedCharacters)
        );
    }

    #[ink::test]
    fn checks_edges() {
        let alice = default_accounts::<DefaultEnvironment>().alice;
        let checker = NameChecker::new(
            alice,
            (2, 5),
            vec![
                UnicodeRange {
                    lower: 'a' as u32,
                    upper: 'z' as u32,
                },
                UnicodeRange {
                    lower: '0' as u32,
                    upper: '9' as u32,
                },
                UnicodeRange {
                    lower: '-' as u32,
                    upper: '-' as u32,
                },
            ],
            vec![UnicodeRange {
                lower: '-' as u32,
                upper: '-' as u32,
            }],
        );

        let ok_edge = String::from("a-bcd");
        assert_eq!(checker.is_name_allowed(ok_edge), Ok(()));

        let disallowed_edge = String::from("-abcd");
        assert_eq!(
            checker.is_name_allowed(disallowed_edge),
            Err(Error::ContainsDisallowedCharacters)
        );

        let disallowed_edge_2 = String::from("abcd-");
        assert_eq!(
            checker.is_name_allowed(disallowed_edge_2),
            Err(Error::ContainsDisallowedCharacters)
        );
    }

    #[ink::test]
    fn set_admin_works() {
        let accounts = default_accounts::<DefaultEnvironment>();
        let mut contract = NameChecker::new(accounts.alice, (2, 5), vec![], vec![]);

        assert_eq!(contract.get_admin(), accounts.alice);
        assert_eq!(contract.set_admin(accounts.bob), Ok(()));
        assert_eq!(contract.get_admin(), accounts.bob);

        // Now alice (not admin anymore) cannot update admin
        assert_eq!(contract.set_admin(accounts.alice), Err(Error::NotAdmin));
    }
}
