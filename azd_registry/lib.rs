#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use ink::env;

#[ink::contract]
mod azd_registry {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::result::*;

    use ink::env;
    use ink::storage::Mapping;

    use crate::azd_registry::Error::{
        CallerIsNotController, CallerIsNotOwner, NoRecordsForAddress, RecordNotFound,
        WithdrawFailed,
    };

    pub type Result<T> = core::result::Result<T, Error>;

    /// Emitted whenever a new name is registered.
    #[ink(event)]
    pub struct Register {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: ink::primitives::AccountId,
    }

    /// Emitted whenever a name is released
    #[ink(event)]
    pub struct Release {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: ink::primitives::AccountId,
    }

    /// Emitted whenever an address changes.
    #[ink(event)]
    pub struct SetAddress {
        #[ink(topic)]
        name: String,
        from: ink::primitives::AccountId,
        #[ink(topic)]
        old_address: Option<ink::primitives::AccountId>,
        #[ink(topic)]
        new_address: ink::primitives::AccountId,
    }

    /// Emitted whenever a name is transferred.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        name: String,
        from: ink::primitives::AccountId,
        #[ink(topic)]
        old_owner: Option<ink::primitives::AccountId>,
        #[ink(topic)]
        new_owner: ink::primitives::AccountId,
    }

    #[ink(storage)]
    pub struct DomainNameService {
        /// A mapping to set a controller for each address
        name_to_controller: Mapping<String, ink::primitives::AccountId>,
        /// A Stringmap to store all name to addresses mapping.
        name_to_address: Mapping<String, ink::primitives::AccountId>,
        /// A Stringmap to store all name to owners mapping.
        name_to_owner: Mapping<String, ink::primitives::AccountId>,
        /// The default address.
        default_address: ink::primitives::AccountId,
        /// Fee to pay for domain registration
        fee: Balance,
        /// Owner of the contract
        /// can withdraw funds
        owner: ink::primitives::AccountId,
        /// All names of an address
        owner_to_names: Mapping<ink::primitives::AccountId, Vec<String>>,
        additional_info: Mapping<String, Vec<(String, String)>>,
        // TODO: replace Vector with Mapping
        metadata: Mapping<String, Mapping<String, String>>,
    }

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    pub enum Error {
        /// Returned if the name already exists upon registration.
        NameAlreadyExists,
        /// Returned if caller is not owner while required to.
        CallerIsNotOwner,
        /// This call requires the caller to be a controller of the domain
        CallerIsNotController,
        /// Returned if caller did not send a required fee
        FeeNotPaid,
        /// Returned if name is empty
        NameEmpty,
        /// Record with the key doesn't exist
        RecordNotFound,
        /// Address has no records
        NoRecordsForAddress,
        /// Withdraw failed
        WithdrawFailed,
    }

    impl DomainNameService {
        /// Creates a new AZNS contract.
        #[ink(constructor)]
        pub fn new(fee: Option<Balance>) -> Self {
            let caller = Self::env().caller();

            Self {
                name_to_controller: Mapping::default(),
                name_to_address: Mapping::default(),
                name_to_owner: Mapping::default(),
                default_address: Default::default(),
                fee: match fee {
                    Some(fee) => fee,
                    None => Default::default(),
                },
                owner: caller,
                owner_to_names: Default::default(),
                additional_info: Default::default(),
            }
        }

        /// Transfers `value` amount of tokens to the caller.
        ///
        /// # Errors
        ///
        /// - Panics in case the requested transfer exceeds the contract balance.
        /// - Panics in case the requested transfer would have brought this
        ///   contract's balance below the minimum balance (i.e. the chain's
        ///   existential deposit).
        /// - Panics in case the transfer failed for another reason.
        #[ink(message)]
        pub fn withdraw(&mut self, value: Balance) -> Result<()> {
            if self.owner != Self::env().caller() {
                return Err(CallerIsNotOwner);
            }

            assert!(value <= Self::env().balance(), "insufficient funds!");

            if Self::env().transfer(Self::env().caller(), value).is_err() {
                return Err(WithdrawFailed);
            }

            Ok(())
        }

        /// Register specific name with caller as owner.
        #[ink(message, payable)]
        pub fn register(&mut self, name: String) -> Result<()> {
            /* Name cannot be empty */
            if name.is_empty() {
                return Err(Error::NameEmpty);
            }

            /* Make sure the register is paid for */
            let _transferred = Self::env().transferred_value();
            if _transferred < self.fee {
                return Err(Error::FeeNotPaid);
            }

            /* Ensure domain is not already registered */
            let caller = Self::env().caller();
            if self.name_to_owner.contains(&name) {
                return Err(Error::NameAlreadyExists);
            }

            /* Set domain owner */
            self.name_to_owner.insert(&name, &caller);

            /* Set domain controller */
            self.name_to_controller.insert(&name, &caller);

            /* Set resolved domain */
            self.name_to_address.insert(&name, &caller);

            /* Update convenience mapping */
            let previous_names = self.owner_to_names.get(caller);
            if let Some(names) = previous_names {
                let mut new_names = names.clone();
                new_names.push(name.clone());
                self.owner_to_names.insert(caller, &new_names);
            } else {
                self.owner_to_names
                    .insert(caller, &Vec::from([name.clone()]));
            }

            /* Emit register event */
            Self::env().emit_event(Register {
                name: name.clone(),
                from: caller,
            });

            Ok(())
        }

        /// Release domain from registration.
        #[ink(message)]
        pub fn release(&mut self, name: String) -> Result<()> {
            let caller = Self::env().caller();
            let owner = self.get_owner_or_default(&name);
            if caller != owner {
                return Err(CallerIsNotOwner);
            }

            self.name_to_owner.remove(&name);
            self.name_to_address.remove(&name);
            self.remove_name_from_owner(caller, name.clone());
            self.name_to_controller.remove(&name);
            self.additional_info.remove(&name);

            Self::env().emit_event(Release {
                name: name.clone(),
                from: caller,
            });

            Ok(())
        }

        /// Set resolved address for specific name.
        #[ink(message)]
        pub fn set_address(
            &mut self,
            name: String,
            new_address: ink::primitives::AccountId,
        ) -> Result<()> {
            /* Ensure the caller is the controller */
            let caller = Self::env().caller();
            let controller = self.get_controller_or_default(&name);
            if caller != controller {
                return Err(CallerIsNotController);
            }

            let old_address = self.name_to_address.get(&name);
            self.name_to_address.insert(&name, &new_address);

            Self::env().emit_event(SetAddress {
                name,
                from: caller,
                old_address,
                new_address,
            });
            Ok(())
        }

        /// Transfer owner to another address.
        #[ink(message)]
        pub fn transfer(&mut self, name: String, to: ink::primitives::AccountId) -> Result<()> {
            /* Ensure the caller is the owner of the domain */
            let caller = Self::env().caller();
            let owner = self.get_owner_or_default(&name);
            if caller != owner {
                return Err(CallerIsNotOwner);
            }

            /* Change owner */
            let old_owner = self.name_to_owner.get(&name);
            self.name_to_owner.insert(&name, &to);

            /* Remove from reverse search and add again */
            self.remove_name_from_owner(caller, name.clone());
            let previous_names = self.owner_to_names.get(to);
            if let Some(names) = previous_names {
                let mut new_names = names.clone();
                new_names.push(name.clone());
                self.owner_to_names.insert(to, &new_names);
            } else {
                self.owner_to_names.insert(to, &Vec::from([name.clone()]));
            }

            Self::env().emit_event(Transfer {
                name,
                from: caller,
                old_owner,
                new_owner: to,
            });

            Ok(())
        }

        /// Get address for specific name.
        #[ink(message)]
        pub fn get_address(&self, name: String) -> ink::primitives::AccountId {
            self.get_address_or_default(name)
        }

        /// Get owner of specific name.
        #[ink(message)]
        pub fn get_owner(&self, name: String) -> ink::primitives::AccountId {
            self.get_owner_or_default(&name)
        }

        pub fn get_controller_or_default(&self, name: &String) -> ink::primitives::AccountId {
            self.name_to_controller
                .get(&name)
                .unwrap_or(self.default_address)
        }

        /// Returns the owner given the String or the default address.
        fn get_owner_or_default(&self, name: &String) -> ink::primitives::AccountId {
            self.name_to_owner
                .get(&name)
                .unwrap_or(self.default_address)
        }

        /// Returns the address given the String or the default address.
        fn get_address_or_default(&self, name: String) -> ink::primitives::AccountId {
            self.name_to_address
                .get(&name)
                .unwrap_or(self.default_address)
        }

        /// Returns all names the address owns
        #[ink(message)]
        pub fn get_names_of_address(
            &self,
            owner: ink::primitives::AccountId,
        ) -> Option<Vec<String>> {
            return self.owner_to_names.get(owner);
        }

        /// Deletes a name from owner
        fn remove_name_from_owner(&mut self, owner: ink::primitives::AccountId, name: String) {
            if let Some(old_names) = self.owner_to_names.get(owner) {
                let mut new_names: Vec<String> = old_names.clone();
                new_names.retain(|prevname| prevname.clone() != name);
                self.owner_to_names.insert(owner, &new_names);
            }
        }

        // /// Sets an arbitrary record
        // #[ink(message)]
        // pub fn set_record(&mut self, owner:ink::primitives::AccountId, record: (String, String)) {
        //     self.additional_info.insert(owner, )
        //
        //     // /* If info vec already exists, modify it */
        //     // if let Some(original_info) = self.additional_info.get(owner) {
        //     //     /* Filter out the record from the vector, if it is already there */
        //     //     let mut filtered_info: Vec<&(String, String)> = original_info.clone().iter().filter(|&&tuple| {
        //     //         return tuple.0 != record.0.clone();
        //     //     }).collect();
        //     //     filtered_info.push(&record);
        //     //     self.additional_info.insert(owner, &Vec::from(filtered_info));
        //     // } else {
        //     //     self.additional_info.insert(owner, &Vec::from([record]));
        //     // }
        // }

        /// Gets an arbitrary record by key
        #[ink(message)]
        pub fn get_record(&self, name: String, key: String) -> Result<String> {
            return if let Some(info) = self.additional_info.get(name) {
                if let Some(value) = info.iter().find(|tuple| {
                    return tuple.0 == key;
                }) {
                    Ok(value.clone().1)
                } else {
                    Err(RecordNotFound)
                }
            } else {
                Err(NoRecordsForAddress)
            };
        }

        /// Sets all records
        #[ink(message)]
        pub fn set_all_records(
            &mut self,
            name: String,
            records: Vec<(String, String)>,
        ) -> Result<()> {
            /* Ensure that the caller is a controller */
            let caller: ink::primitives::AccountId = Self::env().caller();
            let controller = self.get_controller_or_default(&name);
            if caller != controller {
                return Err(CallerIsNotController);
            }

            self.metadata

            self.additional_info.insert(name, &records);

            Ok(())
        }

        /// Gets all records
        #[ink(message)]
        pub fn get_all_records(&self, name: String) -> Result<Vec<(String, String)>> {
            return if let Some(info) = self.additional_info.get(name) {
                return Ok(info);
            } else {
                Err(NoRecordsForAddress)
            };
        }
    }

    #[cfg(test)]
    mod tests {
        use alloc::string::{String, ToString};
        use alloc::vec::Vec;

        use crate::env::test::DefaultAccounts;
        use ink;
        use ink::env::DefaultEnvironment;
        use ink::primitives::AccountId;

        use ink::env::test::*;

        use crate::azd_registry::DomainNameService;
        use crate::azd_registry::Error::CallerIsNotOwner;

        use super::*;

        fn default_accounts() -> DefaultAccounts<DefaultEnvironment> {
            ink::env::test::default_accounts::<DefaultEnvironment>()
        }

        fn set_next_caller(caller: AccountId) {
            ink::env::test::set_caller::<DefaultEnvironment>(caller);
        }

        #[ink::test]
        fn register_works() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(None);

            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(
                contract.get_names_of_address(default_accounts.alice),
                Some(Vec::from([name.clone()]))
            );
            assert_eq!(
                contract.register(name.clone()),
                Err(Error::NameAlreadyExists)
            );
        }

        #[ink::test]
        fn withdraw_works() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(Some(50));

            let acc_balance_before_transfer: Balance =
                ink::env::test::get_account_balance::<DefaultEnvironment>(default_accounts.alice)
                    .unwrap();
            ink::env::test::set_value_transferred::<DefaultEnvironment>(50 ^ 12);
            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(contract.withdraw(50 ^ 12), Ok(()));
            let acc_balance_after_withdraw: Balance =
                ink::env::test::get_account_balance::<DefaultEnvironment>(default_accounts.alice)
                    .unwrap();
            assert_eq!(
                acc_balance_before_transfer + 50 ^ 12,
                acc_balance_after_withdraw
            );
        }

        #[ink::test]
        fn withdraw_only_owner() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(Some(50));

            let acc_balance_before_transfer: Balance =
                ink::env::test::get_account_balance::<DefaultEnvironment>(default_accounts.alice)
                    .unwrap();
            ink::env::test::set_value_transferred::<DefaultEnvironment>(50 ^ 12);
            assert_eq!(contract.register(name.clone()), Ok(()));

            set_next_caller(default_accounts.bob);
            assert_eq!(contract.withdraw(50 ^ 12), Err(CallerIsNotOwner));
        }

        #[ink::test]
        fn reverse_search_works() {
            let default_accounts = default_accounts();
            let name = String::from("test");
            let name2 = String::from("test2");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(None);

            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(contract.register(name2.clone()), Ok(()));
            assert!(contract
                .get_names_of_address(default_accounts.alice)
                .unwrap()
                .contains(&String::from("test")));
            assert!(contract
                .get_names_of_address(default_accounts.alice)
                .unwrap()
                .contains(&String::from("test2")));
        }

        #[ink::test]
        fn register_empty_reverts() {
            let default_accounts = default_accounts();
            let name = String::from("");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(None);

            assert_eq!(contract.register(name.clone()), Err(Error::NameEmpty));
        }

        #[ink::test]
        fn register_with_fee_works() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(Some(50 ^ 12));

            set_value_transferred::<DefaultEnvironment>(50 ^ 12);
            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(contract.register(name), Err(Error::NameAlreadyExists));
        }

        #[ink::test]
        fn register_without_fee_reverts() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(Some(50 ^ 12));

            assert_eq!(contract.register(name), Err(Error::FeeNotPaid));
        }

        #[ink::test]
        fn release_works() {
            let default_accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(default_accounts.alice);
            let mut contract = DomainNameService::new(None);

            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(
                contract.set_address(name.clone(), default_accounts.alice),
                Ok(())
            );
            assert_eq!(contract.get_owner(name.clone()), default_accounts.alice);
            assert_eq!(contract.get_address(name.clone()), default_accounts.alice);
            assert_eq!(
                contract.get_names_of_address(default_accounts.alice),
                Some(Vec::from([name.clone()]))
            );

            assert_eq!(contract.release(name.clone()), Ok(()));
            assert_eq!(contract.get_owner(name.clone()), Default::default());
            assert_eq!(contract.get_address(name.clone()), Default::default());
            assert_eq!(
                contract.get_names_of_address(default_accounts.alice),
                Some(Vec::from([]))
            );

            /* Another account can register again*/
            set_next_caller(default_accounts.bob);
            assert_eq!(contract.register(name.clone()), Ok(()));
            assert_eq!(
                contract.set_address(name.clone(), default_accounts.bob),
                Ok(())
            );
            assert_eq!(contract.get_owner(name.clone()), default_accounts.bob);
            assert_eq!(contract.get_address(name.clone()), default_accounts.bob);
            assert_eq!(contract.release(name.clone()), Ok(()));
            assert_eq!(contract.get_owner(name.clone()), Default::default());
            assert_eq!(contract.get_address(name.clone()), Default::default());
        }

        #[ink::test]
        fn set_address_works() {
            let accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(accounts.alice);

            let mut contract = DomainNameService::new(None);
            assert_eq!(contract.register(name.clone()), Ok(()));

            // Caller is not owner, `set_address` should fail.
            set_next_caller(accounts.bob);
            assert_eq!(
                contract.set_address(name.clone(), accounts.bob),
                Err(CallerIsNotOwner)
            );

            // Caller is owner, set_address will be successful
            set_next_caller(accounts.alice);
            assert_eq!(contract.set_address(name.clone(), accounts.bob), Ok(()));
            assert_eq!(contract.get_address(name.clone()), accounts.bob);
        }

        #[ink::test]
        fn transfer_works() {
            let accounts = default_accounts();
            let name = String::from("test");

            set_next_caller(accounts.alice);

            let mut contract = DomainNameService::new(None);
            assert_eq!(contract.register(name.clone()), Ok(()));

            // Test transfer of owner.
            assert_eq!(contract.transfer(name.clone(), accounts.bob), Ok(()));

            assert_eq!(
                contract.get_names_of_address(accounts.alice),
                Some(Vec::from([]))
            );
            assert_eq!(
                contract.get_names_of_address(accounts.bob),
                Some(Vec::from([name.clone()]))
            );

            // Owner is bob, alice `set_address` should fail.
            assert_eq!(
                contract.set_address(name.clone(), accounts.bob),
                Err(CallerIsNotOwner)
            );

            set_next_caller(accounts.bob);
            // Now owner is bob, `set_address` should be successful.
            assert_eq!(contract.set_address(name.clone(), accounts.bob), Ok(()));
            assert_eq!(contract.get_address(name.clone()), accounts.bob);
        }

        #[ink::test]
        fn additional_data_works() {
            let accounts = default_accounts();
            let key = String::from("twitter");
            let value = String::from("@test");
            let records = Vec::from([(key.clone(), value.clone())]);

            let domain_name = "test".to_string();

            set_next_caller(accounts.alice);
            let mut contract = DomainNameService::new(None);
            assert_eq!(contract.register(domain_name.clone()), Ok(()));

            assert_eq!(
                contract.set_all_records(domain_name.clone(), records.clone()),
                Ok(())
            );
            assert_eq!(
                contract
                    .get_record(domain_name.clone(), key.clone())
                    .unwrap(),
                value.clone()
            );

            /* Confirm idempotency */
            assert_eq!(
                contract.set_all_records(domain_name.clone(), records.clone()),
                Ok(())
            );
            assert_eq!(
                contract
                    .get_record(domain_name.clone(), key.clone())
                    .unwrap(),
                value.clone()
            );

            /* Confirm overwriting */
            assert_eq!(
                contract.set_all_records(
                    domain_name.clone(),
                    Vec::from([("twitter".to_string(), "@newtest".to_string())]),
                ),
                Ok(())
            );
            assert_eq!(
                contract.get_all_records(domain_name.clone()).unwrap(),
                Vec::from([("twitter".to_string(), "@newtest".to_string())])
            );
        }
    }
}
