#![cfg_attr(not(feature = "std"), no_std, no_main)]

pub use self::azns_name_checker::{NameChecker, NameCheckerRef};

/// Contains the bounds of a Unicode range, with each bound representing a Unicode character
/// Used to check whether a certain character is allowed by specifying allowed ranges, such as a-z etc.
#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, Clone)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct UnicodeRange {
    pub lower: u32,
    pub upper: u32,
}

const BANNED_CHARS: &[char] = &[
    /* Unicode whitespace/invisible characters starts from here */
    '\u{0009}', '\u{0020}', '\u{00A0}', '\u{00AD}', '\u{034F}', '\u{061C}', '\u{070F}', '\u{115F}',
    '\u{1160}', '\u{1680}', '\u{17B4}', '\u{17B5}', '\u{180E}', '\u{2000}', '\u{2001}', '\u{2002}',
    '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200A}',
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{202F}', '\u{205F}', '\u{2060}',
    '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{206A}', '\u{206B}', '\u{206C}', '\u{206D}',
    '\u{206E}', '\u{206F}', '\u{2800}', '\u{3000}', '\u{3164}', '\u{FEFF}', '\u{FFA0}',
    /* Unicode Dash characters starts from here */
    '\u{058A}', '\u{05BE}', '\u{1806}', '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{207B}',
    '\u{208B}', '\u{2212}', '\u{2E1A}', '\u{2E40}', '\u{2E5D}', '\u{FE58}', '\u{FE63}', '\u{FF0D}',
];

impl UnicodeRange {
    fn is_valid(&self) -> bool {
        if self.lower > self.upper {
            return false;
        }

        for &char in BANNED_CHARS.iter() {
            let char = char as u32;
            if self.lower <= char && char <= self.upper {
                return false;
            }
        }
        true
    }
}

#[zink::coating(Ownable2Step[
    Error = Error::NotAdmin
])]
#[zink::coating(Upgradable)]
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
        pending_admin: Option<AccountId>,
        allowed_length: (Min, Max),
        allowed_unicode_ranges: Vec<UnicodeRange>,
        disallowed_unicode_ranges_for_edges: Vec<UnicodeRange>,
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
        InvalidRange,
    }

    impl NameChecker {
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            allowed_length: (u8, u8),
            allowed_unicode_ranges: Vec<UnicodeRange>,
            disallowed_unicode_ranges_for_edges: Vec<UnicodeRange>,
        ) -> Self {
            let mut contract = Self {
                admin,
                pending_admin: None,
                allowed_unicode_ranges: Default::default(),
                allowed_length: Default::default(),
                disallowed_unicode_ranges_for_edges: Default::default(),
            };

            contract
                .set_allowed_length(allowed_length)
                .expect("invalid length(s)");
            contract
                .set_allowed_unicode_ranges(allowed_unicode_ranges)
                .expect("invalid allowed-unicode-range(s)");
            contract
                .set_disallowed_unicode_ranges_for_edges(disallowed_unicode_ranges_for_edges)
                .expect("invalid disallowed-unicodes-for-edges");

            contract
        }

        #[ink(message)]
        pub fn is_name_allowed(&self, name: String) -> Result<()> {
            /* Check length */
            let (min, max) = self.allowed_length;
            let len = name.chars().count() as u64;

            match len {
                l if l > max as u64 => return Err(Error::TooLong),
                l if l < min as u64 => return Err(Error::TooShort),
                _ => (),
            }

            /* Check edges */
            let edges = vec![
                name.chars().next().unwrap(),
                name.chars().rev().next().unwrap(),
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
            let allowed = name.chars().all(|char| {
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

            if !new_ranges.iter().all(UnicodeRange::is_valid) {
                return Err(Error::InvalidRange);
            }
            self.allowed_unicode_ranges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_disallowed_unicode_ranges_for_edges(
            &mut self,
            new_ranges: Vec<UnicodeRange>,
        ) -> Result<()> {
            self.ensure_admin()?;

            if new_ranges.iter().any(|rng| rng.lower > rng.upper) {
                return Err(Error::InvalidRange);
            }
            self.disallowed_unicode_ranges_for_edges = new_ranges;
            Ok(())
        }

        #[ink(message)]
        pub fn set_allowed_length(&mut self, new_length: (Min, Max)) -> Result<()> {
            self.ensure_admin()?;

            if new_length.0 == 0 || new_length.0 > new_length.1 {
                return Err(Error::InvalidRange);
            }
            self.allowed_length = new_length;
            Ok(())
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
            (1, 99),
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
    fn ownable_2_step_works() {
        let accounts = default_accounts::<DefaultEnvironment>();
        let mut contract = NameChecker::new(accounts.alice, (2, 5), vec![], vec![]);

        assert_eq!(contract.get_admin(), accounts.alice);
        contract.transfer_ownership(Some(accounts.bob)).unwrap();

        assert_eq!(contract.get_admin(), accounts.alice);

        ink::env::test::set_caller::<DefaultEnvironment>(accounts.bob);
        contract.accept_ownership().unwrap();
        assert_eq!(contract.get_admin(), accounts.bob);
    }

    #[ink::test]
    #[should_panic(expected = "invalid allowed-unicode-range(s)")]
    fn banned_characters_disallowed() {
        let alice = default_accounts::<DefaultEnvironment>().alice;
        let _checker = NameChecker::new(
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
                    // Blank space
                    lower: ' ' as u32,
                    upper: ' ' as u32,
                },
            ],
            vec![],
        );
    }
}
