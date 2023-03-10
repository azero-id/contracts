#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod azns_router {
    use ink::storage::Mapping;

    pub type Result<T> = core::result::Result<T, Error>;

    #[ink(storage)]
    pub struct Router {
        /// Account allowed to update the state
        admin: AccountId,
        /// Maps TLDs to their registry contract address
        routes: Mapping<String, AccountId>,
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
    }
}
