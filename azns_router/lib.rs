#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod azns_router {
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec;
    use ink::prelude::vec::Vec;
    use ink::storage::traits::ManualKey;
    use ink::storage::Mapping;

    pub type Result<T> = core::result::Result<T, Error>;

    #[ink(storage)]
    pub struct Router {
        /// Account allowed to update the state
        admin: AccountId,
        /// List of registeries registered with the router
        registry: Vec<AccountId>,
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
        /// Given TLD already points to a registry
        TldAlreadyInUse(String),
        /// Given Tld not found
        TldNotFound(String),
        /// Cannot find the resolved address
        CouldNotResolveDomain,
        /// Domain does not contain valid name and/or tld
        InvalidDomainName,
    }

    impl Router {
        #[ink(constructor)]
        pub fn new(admin: AccountId) -> Self {
            Self {
                admin,
                routes: Default::default(),
                registry: Default::default(),
            }
        }

        #[ink(message)]
        pub fn add_registry(&mut self, tld: Vec<String>, registry_addr: AccountId) -> Result<()> {
            self.ensure_admin()?;

            // this is disabled during tests as it is not being supported (tests end up panicking).
            #[cfg(not(test))]
            if !self.env().is_contract(&registry_addr) {
                return Err(Error::InvalidRegistryAddress);
            }

            if self.registry.iter().all(|&x| x != registry_addr) {
                self.registry.push(registry_addr);
            }

            for i in 0..tld.len() {
                if self.routes.contains(&tld[i]) {
                    return Err(Error::TldAlreadyInUse(tld[i].clone()));
                }
                self.routes.insert(&tld[i], &registry_addr);
            }

            Ok(())
        }

        #[ink(message)]
        pub fn update_registry(
            &mut self,
            tld: Vec<String>,
            registry_addr: AccountId,
        ) -> Result<()> {
            self.ensure_admin()?;

            // this is disabled during tests as it is not being supported (tests end up panicking).
            #[cfg(not(test))]
            if !self.env().is_contract(&registry_addr) {
                return Err(Error::InvalidRegistryAddress);
            }

            for i in 0..tld.len() {
                if !self.routes.contains(&tld[i]) {
                    return Err(Error::TldNotFound(tld[i].clone()));
                }
                self.routes.insert(&tld[i], &registry_addr);
            }

            Ok(())
        }

        #[ink(message)]
        pub fn get_all_registry(&self) -> Vec<AccountId> {
            self.registry.clone()
        }

        #[ink(message)]
        pub fn get_registry(&self, tld: String) -> Option<AccountId> {
            self.routes.get(tld)
        }

        #[ink(message, selector = 0xd259f7ba)]
        pub fn get_address(&self, domain: String) -> Result<AccountId> {
            let (name, tld) = Self::extract_domain(&domain)?;

            let registry_addr = self
                .get_registry(tld.clone())
                .ok_or(Error::TldNotFound(tld))?;

            match cfg!(test) {
                true => unimplemented!(
                    "`invoke_contract()` not being supported (tests end up panicking)"
                ),
                false => {
                    use ink::env::call::{build_call, ExecutionInput, Selector};

                    const GET_ADDRESS_SELECTOR: [u8; 4] = [0xD2, 0x59, 0xF7, 0xBA];
                    let result = build_call::<Environment>()
                        .call(registry_addr)
                        .exec_input(
                            ExecutionInput::new(Selector::new(GET_ADDRESS_SELECTOR)).push_arg(name),
                        )
                        .returns::<core::result::Result<AccountId, u8>>()
                        .params()
                        .invoke();

                    result.map_err(|_| Error::CouldNotResolveDomain)
                }
            }
        }

        /// @returns list of (registry-address, primary-domain) for given account
        #[ink(message)]
        pub fn get_primary_domains(
            &self,
            account: AccountId,
            tld: Option<String>,
        ) -> Vec<(AccountId, String)> {
            let list = match tld {
                None => self.registry.clone(),
                Some(tld) => self.get_registry(tld).map_or(vec![], |a| vec![a]),
            };

            list.iter()
                .filter_map(|&addr| {
                    self.get_primary_domain_for(account, addr)
                        .map(|domain| (addr, domain))
                })
                .collect()
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

        fn extract_domain(domain: &str) -> Result<(String, String)> {
            let pos = domain.rfind('.').ok_or(Error::InvalidDomainName)?;

            let name = domain
                .get(0..pos)
                .ok_or(Error::InvalidDomainName)?
                .to_string();

            let tld = domain
                .get(pos + 1..)
                .ok_or(Error::InvalidDomainName)?
                .to_string();

            if name.is_empty() || tld.is_empty() {
                return Err(Error::InvalidDomainName);
            }
            Ok((name, tld))
        }

        fn get_primary_domain_for(
            &self,
            account: AccountId,
            registry_addr: AccountId,
        ) -> Option<String> {
            match cfg!(test) {
                true => unimplemented!(
                    "`invoke_contract()` not being supported (tests end up panicking)"
                ),
                false => {
                    use ink::env::call::{build_call, ExecutionInput, Selector};

                    const GET_PRIMARY_DOMAIN_SELECTOR: [u8; 4] = [0xBF, 0x5B, 0x56, 0x77];
                    let result = build_call::<Environment>()
                        .call(registry_addr)
                        .exec_input(
                            ExecutionInput::new(Selector::new(GET_PRIMARY_DOMAIN_SELECTOR))
                                .push_arg(account),
                        )
                        .returns::<Option<String>>()
                        .params()
                        .invoke();

                    result
                }
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

            let tld1 = "azero".to_string();
            let tld2 = "a0".to_string();
            let tld = vec![tld1.clone(), tld2.clone()];
            let registry_addr = default_accounts().bob;

            assert_eq!(contract.add_registry(tld.clone(), registry_addr), Ok(()));
            assert_eq!(contract.get_registry(tld1.clone()), Some(registry_addr));
            assert_eq!(contract.get_registry(tld2), Some(registry_addr));

            // Adding same tld again fails
            assert_eq!(
                contract.add_registry(tld, registry_addr),
                Err(Error::TldAlreadyInUse(tld1.clone()))
            );
        }

        #[ink::test]
        fn update_registry_works() {
            let mut contract = get_test_router();

            let tld1 = "azero".to_string();
            let tld2 = "a0".to_string();
            let tld = vec![tld1.clone(), tld2.clone()];
            let registry_addr = default_accounts().bob;

            assert_eq!(
                contract.update_registry(tld.clone(), registry_addr),
                Err(Error::TldNotFound(tld1.clone()))
            );
            assert_eq!(contract.add_registry(tld.clone(), registry_addr), Ok(()));

            let new_registry_addr = default_accounts().django;
            assert_eq!(
                contract.update_registry(vec![tld2.clone()], new_registry_addr),
                Ok(())
            );
            assert_eq!(contract.get_registry(tld2), Some(new_registry_addr));
        }

        #[ink::test]
        fn set_admin_works() {
            let mut contract = get_test_router();

            assert_eq!(contract.get_admin(), default_accounts().alice);
            contract.set_admin(default_accounts().bob).unwrap();
            assert_eq!(contract.get_admin(), default_accounts().bob);
        }

        #[test]
        fn extract_domain_works() {
            assert_eq!(
                Router::extract_domain("alice"),
                Err(Error::InvalidDomainName)
            );

            assert_eq!(
                Router::extract_domain("alice."),
                Err(Error::InvalidDomainName)
            );

            assert_eq!(
                Router::extract_domain(".azero"),
                Err(Error::InvalidDomainName)
            );

            assert_eq!(
                Router::extract_domain("alice.azero"),
                Ok(("alice".to_string(), "azero".to_string()))
            );

            assert_eq!(
                Router::extract_domain("sub.alice.azero"),
                Ok(("sub.alice".to_string(), "azero".to_string()))
            );
        }
    }
}
