#![cfg_attr(not(feature = "std"), no_std)]

pub use self::azns_name_checker::{NameChecker, NameCheckerRef};

extern crate alloc;
extern crate unicode_segmentation;

#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct UnicodeRange {
    pub lower: u32,
    pub upper: u32,
}

#[ink::contract]
mod azns_name_checker {
    use crate::azns_name_checker::Error::{ContainsDisallowedCharacters, TooLong, TooShort};

    use crate::alloc::string::ToString;
    use crate::UnicodeRange;
    use alloc::vec;
    use ink::prelude::string::String;
    use ink::prelude::vec::Vec;
    use unicode_segmentation::UnicodeSegmentation;

    type Min = u64;
    type Max = u64;

    #[ink(storage)]
    pub struct NameChecker {
        allowed_unicode_ranges: Vec<UnicodeRange>,
        disallowed_for_edges: Vec<UnicodeRange>,
        allowed_length: (Min, Max),
        admin: AccountId,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        TooShort,
        TooLong,
        ContainsDisallowedCharacters,
        CallerIsNotAdmin,
    }

    impl NameChecker {
        #[ink(constructor)]
        pub fn new(
            allowed_length: (u64, u64),
            allowed_unicode_ranges: Vec<UnicodeRange>,
            disallowed_for_edges: Vec<UnicodeRange>,
        ) -> Self {
            Self {
                allowed_unicode_ranges,
                allowed_length,
                disallowed_for_edges,
                admin: Self::env().caller(),
            }
        }

        #[ink(message)]
        pub fn is_name_allowed(&self, domain: String) -> Result<bool> {
            let stripped_domain = self.strip_skin_tones(&domain);
            /* Check length */
            let (min, max) = self.allowed_length;
            let len = stripped_domain.len() as u64;

            match len {
                l if l > max => return Err(TooLong),
                l if l < min => return Err(TooShort),
                _ => (),
            }

            /* Check edges */
            let edges = vec![
                stripped_domain.chars().next().unwrap(),
                stripped_domain.chars().rev().next().unwrap(),
            ];

            let illegal_edges = edges.iter().any(|char| {
                self.disallowed_for_edges.iter().any(|range| {
                    let lower = range.lower;
                    let upper = range.upper;

                    let check_char = move |c| lower <= c && c <= upper;
                    check_char(*char as u32)
                })
            });

            if illegal_edges {
                return Err(ContainsDisallowedCharacters);
            }

            /* Check whole name */
            let allowed = stripped_domain.chars().all(|char| {
                self.allowed_unicode_ranges.iter().any(|range| {
                    let lower = range.lower;
                    let upper = range.upper;

                    let check_char = move |c| lower <= c && c <= upper;
                    check_char(char as u32)
                })
            });

            match allowed {
                true => Ok(true),
                false => Err(ContainsDisallowedCharacters),
            }
        }

        fn ensure_admin(&self) -> Result<()> {
            if self.admin != self.env().caller() {
                Err(Error::CallerIsNotAdmin)
            } else {
                Ok(())
            }
        }

        #[ink(message)]
        pub fn set_allowed_unicode_ranges(&mut self, new_ranges: Vec<UnicodeRange>) -> Result<()> {
            self.ensure_admin()?;
            self.allowed_unicode_ranges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_disallowed_for_edges(&mut self, new_ranges: Vec<UnicodeRange>) -> Result<()> {
            self.ensure_admin()?;
            self.disallowed_for_edges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_allowed_length(&mut self, new_length: (Min, Max)) -> Result<()> {
            self.ensure_admin()?;
            self.allowed_length = new_length;
            Ok(())
        }

        fn strip_skin_tones(&self, s: &str) -> String {
            fn is_emoji_modifier(c: char) -> bool {
                match c {
                    '\u{1f3fb}'...'\u{1f3ff}' => true,
                    _ => false,
                }
            }

            s.graphemes(true)
                .map(|g| match g.len() {
                    4 => g.to_string(),
                    8 => {
                        if is_emoji_modifier(g.chars().nth(1).unwrap()) {
                            let base_emoji = g.chars().next().unwrap().to_string();
                            base_emoji
                        } else {
                            g.to_string()
                        }
                    }
                    _ => g.to_string(),
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::azns_name_checker::*;
    use crate::azns_name_checker::Error::{ContainsDisallowedCharacters, TooLong, TooShort};
    use crate::UnicodeRange;
    use ink::prelude::string::String;

    #[ink::test]
    fn checks_length() {
        let checker = NameChecker::new(
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
        assert_eq!(checker.is_name_allowed(short_name), Err(TooShort));

        let long_name = String::from("abcdef");
        assert_eq!(checker.is_name_allowed(long_name), Err(TooLong));

        let ok_name = String::from("abcd");
        assert_eq!(checker.is_name_allowed(ok_name), Ok(true));
    }

    #[ink::test]
    fn checks_unicode_ranges() {
        let checker = NameChecker::new(
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
        assert_eq!(checker.is_name_allowed(allowed_name), Ok(true));

        let allowed_name_2 = String::from("abc-d");
        assert_eq!(checker.is_name_allowed(allowed_name_2), Ok(true));

        let bad_chars = String::from("***");
        assert_eq!(
            checker.is_name_allowed(bad_chars),
            Err(ContainsDisallowedCharacters)
        );
    }

    #[ink::test]
    fn works_with_emojis() {
        let checker = NameChecker::new(
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
        assert_eq!(checker.is_name_allowed(allowed_name), Ok(true));

        let allowed_name_2 = String::from("🚁");
        assert_eq!(checker.is_name_allowed(allowed_name_2), Ok(true));

        let allowed_name_3 = String::from("👋🏽");
        assert_eq!(checker.is_name_allowed(allowed_name_3), Ok(true));

        let bad_chars = String::from("😅");
        assert_eq!(
            checker.is_name_allowed(bad_chars),
            Err(ContainsDisallowedCharacters)
        );
    }

    #[ink::test]
    fn checks_edges() {
        let checker = NameChecker::new(
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
        assert_eq!(checker.is_name_allowed(ok_edge), Ok(true));

        let disallowed_edge = String::from("-abcd");
        assert_eq!(
            checker.is_name_allowed(disallowed_edge),
            Err(ContainsDisallowedCharacters)
        );

        let disallowed_edge_2 = String::from("abcd-");
        assert_eq!(
            checker.is_name_allowed(disallowed_edge_2),
            Err(ContainsDisallowedCharacters)
        );
    }
}
