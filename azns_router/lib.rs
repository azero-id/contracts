#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod azns_router {
    use ink::prelude::string::String;
    use ink::storage::traits::ManualKey;
    use ink::storage::Mapping;

    pub type Result<T> = core::result::Result<T, Error>;

    #[ink(storage)]
    pub struct Router {
        /// Account allowed to update the state
        admin: AccountId,
        /// Maps TLDs to their registry contract address
        routes: Mapping<String, AccountId, ManualKey<100>>,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Caller not allowed to call privileged calls.
        NotAdmin,
        /// Not a contract address
        InvalidRegistryAddress,
        /// given TLD already points to a registry
        TldAlreadyInUse,
    }

    impl Router {
        #[ink(constructor)]
        pub fn new(admin: AccountId) -> Self {
            Self {
                admin,
                routes: Default::default(),
            }
        }

        #[ink(message)]
        pub fn add_registry(&mut self, tld: String, registry_addr: AccountId) -> Result<()> {
            self.ensure_admin()?;

            // this is disabled during tests as it is not being supported (tests end up panicking).
            #[cfg(not(test))]
            if !self.env().is_contract(&registry_addr) {
                return Err(Error::InvalidRegistryAddress);
            }
            if self.routes.contains(&tld) {
                return Err(Error::TldAlreadyInUse);
            }

            self.routes.insert(&tld, &registry_addr);
            Ok(())
        }

        #[ink(message)]
        pub fn get_registry(&self, tld: String) -> Option<AccountId> {
            self.routes.get(tld)
        }

        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
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
            match self.env().caller() == self.admin {
                true => Ok(()),
                false => Err(Error::NotAdmin),
            }
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

        fn get_test_router() -> Router {
            let contract_addr: AccountId = AccountId::from([0xFF as u8; 32]);
            set_callee::<DefaultEnvironment>(contract_addr);
            Router::new(default_accounts().alice)
        }

        #[ink::test]
        fn add_registry_works() {
            let mut contract = get_test_router();

            let tld = "azero".to_string();
            let registry_addr = default_accounts().bob;

            assert_eq!(contract.add_registry(tld.clone(), registry_addr), Ok(()));
            assert_eq!(contract.get_registry(tld.clone()), Some(registry_addr));

            // Adding same tld again fails
            assert_eq!(
                contract.add_registry(tld.clone(), registry_addr),
                Err(Error::TldAlreadyInUse)
            );
        }

        #[ink::test]
        fn set_admin_works() {
            let mut contract = get_test_router();

            assert_eq!(contract.get_admin(), default_accounts().alice);
            contract.set_admin(default_accounts().bob).unwrap();
            assert_eq!(contract.get_admin(), default_accounts().bob);
        }
    }
}
