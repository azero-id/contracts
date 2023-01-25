#![cfg_attr(not(feature = "std"), no_std)]

mod address_dict;

#[ink::contract]
mod azns_registry {
    use crate::address_dict::AddressDict;
    use azns_name_checker::get_domain_price;
    use ink::env::hash::CryptoHash;
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec::Vec;
    use ink::storage::Mapping;

    use azns_name_checker::NameCheckerRef;
    use merkle_verifier::MerkleVerifierRef;

    pub type Result<T> = core::result::Result<T, Error>;

    /// Different states of a domain
    #[derive(scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, Debug, PartialEq))]
    pub enum DomainStatus {
        /// Domain is registered by the given address
        Registered(AccountId),
        /// Domain is reserved for the given address
        Reserved(AccountId),
        /// Domain is available for purchase
        Available,
        /// Domain has invalid characters/length
        Unavailable,
    }

    /// Emitted whenever a new name is registered.
    #[ink(event)]
    pub struct Register {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: AccountId,
    }

    /// Emitted whenever a name is released
    #[ink(event)]
    pub struct Release {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: AccountId,
    }

    /// Emitted whenever an address changes.
    #[ink(event)]
    pub struct SetAddress {
        #[ink(topic)]
        name: String,
        from: AccountId,
        #[ink(topic)]
        old_address: Option<AccountId>,
        #[ink(topic)]
        new_address: AccountId,
    }

    /// Emitted whenever a name is transferred.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        name: String,
        from: AccountId,
        #[ink(topic)]
        old_owner: Option<AccountId>,
        #[ink(topic)]
        new_owner: AccountId,
    }

    /// Emitted when switching from whitelist-phase to public-phase
    #[ink(event)]
    pub struct PublicPhaseActivated;

    #[ink(storage)]
    pub struct DomainNameService {
        /// Owner of the contract can withdraw funds
        owner: AccountId,
        /// The default address.
        default_address: AccountId,
        /// Names which can be claimed only by the specified user
        reserved_names: Mapping<String, AccountId>,

        /// Mapping from name to addresses associated with it
        name_to_address_dict: Mapping<String, AddressDict>,
        /// Metadata
        metadata: Mapping<String, Vec<(String, String)>>,

        /// All names an address owns
        owner_to_names: Mapping<AccountId, Vec<String>>,
        /// All names an address controls
        controller_to_names: Mapping<AccountId, Vec<String>>,
        /// All names that resolve to the given address
        resolving_to_address: Mapping<AccountId, Vec<String>>,
        /// Primary domain record
        /// IMPORTANT NOTE: This mapping may be out-of-date, since we don't update it when a resolved address changes, or when a domain is withdrawn.
        /// Only use the get_primary_domain
        address_to_primary_domain: Mapping<AccountId, String>,

        name_checker: Option<NameCheckerRef>,
        /// Merkle Verifier used to identifiy whitelisted addresses
        whitelisted_address_verifier: Option<MerkleVerifierRef>,
    }

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Returned if the name already exists upon registration.
        NameAlreadyExists,
        /// Name is (currently) now allowed
        NameNotAllowed,
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
        /// No resolved address found
        NoResolvedAddress,
        /// A user can claim only one domain during the whitelist-phase
        AlreadyClaimed,
        /// The merkle proof is invalid
        InvalidMerkleProof,
        /// Given operation can only be performed during the whitelist-phase
        OnlyDuringWhitelistPhase,
        /// Given operation cannot be performed during the whitelist-phase
        RestrictedDuringWhitelistPhase,
        /// The given domain is reserved and cannot to be bought
        CannotBuyReservedDomain,
        /// Cannot claim a non-reserved domain. Consider buying it.
        NotReservedDomain,
        /// User is not authorised to claim the given domain
        NotAuthorised,
    }

    impl DomainNameService {
        /// Creates a new AZNS contract.
        #[ink(constructor)]
        pub fn new(
            name_checker_hash: Option<Hash>,
            merkle_verifier_hash: Option<Hash>,
            merkle_root: [u8; 32],
            reserved_domains: Option<Vec<(String, Option<AccountId>)>>,
            version: Option<u32>,
        ) -> Self {
            let caller = Self::env().caller();

            // Initializing NameChecker
            let total_balance = Self::env().balance();
            let salt = version.unwrap_or(1u32).to_le_bytes();

            let name_checker = name_checker_hash.map(|hash| {
                NameCheckerRef::new()
                    .endowment(total_balance / 4) // TODO why /4?
                    .code_hash(hash)
                    .salt_bytes(salt)
                    .instantiate()
                    // .expect("failed at instantiating the `NameCheckerRef` contract")
            });

            // Initializing MerkleVerifier
            let whitelisted_address_verifier = merkle_verifier_hash.map(|ch| {
                MerkleVerifierRef::new(merkle_root)
                    .endowment(total_balance / 4) // TODO why /4?
                    .code_hash(ch)
                    .salt_bytes(salt)
                    .instantiate()
                    // .expect("failed at instantiating the `MerkleVerifierRef` contract")
            });

            let mut contract = Self {
                owner: caller,
                name_checker,
                name_to_address_dict: Mapping::default(),
                default_address: Default::default(),
                owner_to_names: Default::default(),
                metadata: Default::default(),
                address_to_primary_domain: Default::default(),
                controller_to_names: Default::default(),
                resolving_to_address: Default::default(),
                whitelisted_address_verifier,
                reserved_names: Default::default(),
            };

            // Initializing reserved domains
            if let Some(set) = reserved_domains {
                contract.add_reserved_domains(set).expect("Infallible");
            }
            contract
        }

        /// Register specific name on behalf of some other address.
        /// Pay the fee, but forward the ownership of the domain to the provided recipient
        ///
        /// NOTE: During the whitelist phase, use `register()` method instead.
        #[ink(message, payable)]
        pub fn register_on_behalf_of(
            &mut self,
            name: String,
            recipient: AccountId,
            merkle_proof: Option<Vec<[u8; 32]>>,
        ) -> Result<()> {
            if !self.is_name_allowed(&name) {
                return Err(Error::NameNotAllowed);
            }

            // The name must not be a reserved domain
            if self.reserved_names.contains(&name) {
                return Err(Error::CannotBuyReservedDomain);
            }

            // If in whitelist-phase; Verify that the caller is whitelisted
            if self.is_whitelist_phase() {
                let caller = self.env().caller();

                // Recipient must be the same as caller incase of whitelist-phase
                if recipient != caller {
                    return Err(Error::RestrictedDuringWhitelistPhase);
                }

                // Verify this is the first claim of the user
                if self.owner_to_names.contains(caller) {
                    return Err(Error::AlreadyClaimed);
                }

                // Verify the proof
                if !self.verify_proof(caller, merkle_proof) {
                    return Err(Error::InvalidMerkleProof);
                }
            }

            /* Make sure the register is paid for */
            let _transferred = Self::env().transferred_value();
            if _transferred < get_domain_price(&name) {
                return Err(Error::FeeNotPaid);
            }

            self.register_domain(&name, &recipient)
        }

        /// Register specific name with caller as owner.
        ///
        /// NOTE: Whitelisted addresses can buy one domain during the whitelist phase by submitting its proof
        #[ink(message, payable)]
        pub fn register(
            &mut self,
            name: String,
            merkle_proof: Option<Vec<[u8; 32]>>,
            set_as_primary_domain: bool,
        ) -> Result<()> {
            self.register_on_behalf_of(name.clone(), self.env().caller(), merkle_proof)?;
            if set_as_primary_domain {
                self.set_primary_domain(self.env().caller(), name)?;
            }
            Ok(())
        }

        /// Allows users to claim their reserved domain at zero cost
        #[ink(message)]
        pub fn claim_reserved_domain(&mut self, name: String) -> Result<()> {
            let caller = self.env().caller();

            let Some(user) = self.reserved_names.get(&name) else {
                return Err(Error::NotReservedDomain);
            };

            if caller != user {
                return Err(Error::NotAuthorised);
            }

            self.register_domain(&name, &caller).and_then(|_| {
                // Remove the domain from the list once claimed
                self.reserved_names.remove(name);
                Ok(())
            })
        }

        /// Release domain from registration.
        #[ink(message)]
        pub fn release(&mut self, name: String) -> Result<()> {
            // Disabled during whitelist-phase
            if self.is_whitelist_phase() {
                return Err(Error::RestrictedDuringWhitelistPhase);
            }

            let caller = Self::env().caller();
            let owner = self.get_owner_or_default(&name);
            if caller != owner {
                return Err(Error::CallerIsNotOwner);
            }

            self.name_to_address_dict.remove(&name);
            self.metadata.remove(&name);
            self.remove_name_from_owner(&caller, &name);

            Self::env().emit_event(Release { name, from: caller });

            Ok(())
        }

        /// Transfer owner to another address.
        #[ink(message)]
        pub fn transfer(&mut self, name: String, to: AccountId) -> Result<()> {
            // Transfer is disabled during the whitelist-phase
            if self.is_whitelist_phase() {
                return Err(Error::RestrictedDuringWhitelistPhase);
            }

            /* Ensure the caller is the owner of the domain */
            let caller = Self::env().caller();
            let owner = self.get_owner_or_default(&name);
            if caller != owner {
                return Err(Error::CallerIsNotOwner);
            }

            /* Transfer control to new owner `to` */
            let address_dict = AddressDict::new(to);
            self.name_to_address_dict.insert(&name, &address_dict);

            /* Remove from reverse search */
            self.remove_name_from_owner(&caller, &name);
            self.remove_name_from_controller(&caller, &name);
            self.remove_name_from_resolving(&caller, &name);

            /* Add to reverse search of owner */
            self.add_name_to_owner(&to, &name);
            self.add_name_to_controller(&to, &name);
            self.add_name_to_resolving(&to, &name);

            Self::env().emit_event(Transfer {
                name,
                from: caller,
                old_owner: Some(owner),
                new_owner: to,
            });

            Ok(())
        }

        /// Set primary domain of an address (reverse record)
        #[ink(message, payable)]
        pub fn set_primary_domain(&mut self, address: AccountId, name: String) -> Result<()> {
            /* Ensure the caller controls the target name */
            self.ensure_controller(&Self::env().caller(), &name)?;

            let resolved = self.get_resolved_address_or_default(&name);

            /* Ensure the target name resolves to the address */
            if resolved != address {
                return Err(Error::NoResolvedAddress);
            }

            self.address_to_primary_domain.insert(address, &name);

            Ok(())
        }

        /// Set resolved address for specific name.
        #[ink(message)]
        pub fn set_address(&mut self, name: String, new_address: AccountId) -> Result<()> {
            /* Ensure the caller is the controller */
            let caller = Self::env().caller();
            self.ensure_controller(&caller, &name)?;

            let mut address_dict = self.get_address_dict_or_default(&name);
            let old_address = address_dict.resolved;
            address_dict.set_resolved(new_address);
            self.name_to_address_dict.insert(&name, &address_dict);

            /* Check if the old resolved address had this domain set as the primary domain */
            /* If yes -> clear it */
            if self.address_to_primary_domain.get(old_address) == Some(name.clone()) {
                self.address_to_primary_domain.remove(old_address);
            }

            /* Remove the name from the old resolved address */
            self.remove_name_from_resolving(&old_address, &name);

            /* Add the name to the new resolved address */
            self.add_name_to_resolving(&new_address, &name);

            Self::env().emit_event(SetAddress {
                name,
                from: caller,
                old_address: Some(old_address),
                new_address,
            });
            Ok(())
        }

        #[ink(message)]
        pub fn set_controller(&mut self, name: String, new_controller: AccountId) -> Result<()> {
            /* Ensure caller is either controller or owner */
            let caller = Self::env().caller();
            let mut address_dict = self.get_address_dict_or_default(&name);
            let owner = address_dict.owner;
            let controller = address_dict.controller;

            if caller != owner && caller != controller {
                return Err(Error::CallerIsNotOwner);
            }

            address_dict.set_controller(new_controller);
            self.name_to_address_dict.insert(&name, &address_dict);

            /* Remove the name from the old controller */
            self.remove_name_from_controller(&caller, &name);

            /* Add the name to the new controller */
            self.add_name_to_controller(&new_controller, &name);

            Ok(())
        }

        /// Sets all records
        #[ink(message)]
        pub fn set_all_records(
            &mut self,
            name: String,
            records: Vec<(String, String)>,
        ) -> Result<()> {
            /* Ensure that the caller is a controller */
            let caller: AccountId = Self::env().caller();
            self.ensure_controller(&caller, &name)?;

            self.metadata.insert(name, &records);

            Ok(())
        }

        /// Returns the current status of the domain
        #[ink(message)]
        pub fn get_domain_status(&self, name: String) -> DomainStatus {
            if let Some(user) = self.name_to_address_dict.get(&name) {
                DomainStatus::Registered(user.owner)
            } else if let Some(user) = self.reserved_names.get(&name) {
                DomainStatus::Reserved(user)
            } else if self.is_name_allowed(&name) {
                DomainStatus::Available
            } else {
                DomainStatus::Unavailable
            }
        }

        /// Get owner of specific name.
        #[ink(message)]
        pub fn get_owner(&self, name: String) -> AccountId {
            self.get_owner_or_default(&name)
        }

        /// Get controller of specific name.
        #[ink(message)]
        pub fn get_controller(&self, name: String) -> AccountId {
            self.get_controller_or_default(&name)
        }

        /// Get address for specific name.
        #[ink(message)]
        pub fn get_address(&self, name: String) -> AccountId {
            self.get_resolved_address_or_default(&name)
        }

        /// Gets all records
        #[ink(message)]
        pub fn get_metadata(&self, name: String) -> Result<Vec<(String, String)>> {
            if let Some(info) = self.metadata.get(name) {
                Ok(info)
            } else {
                Err(Error::NoRecordsForAddress)
            }
        }

        /// Gets an arbitrary record by key
        #[ink(message)]
        pub fn get_metadata_by_key(&self, name: String, key: String) -> Result<String> {
            return if let Some(info) = self.metadata.get(name) {
                if let Some(value) = info.iter().find(|tuple| tuple.0 == key) {
                    Ok(value.clone().1)
                } else {
                    Err(Error::RecordNotFound)
                }
            } else {
                Err(Error::NoRecordsForAddress)
            };
        }

        /// Returns all names the address owns
        #[ink(message)]
        pub fn get_owned_names_of_address(&self, owner: AccountId) -> Option<Vec<String>> {
            self.owner_to_names.get(owner)
        }

        #[ink(message)]
        pub fn get_controlled_names_of_address(
            &self,
            controller: AccountId,
        ) -> Option<Vec<String>> {
            self.controller_to_names.get(controller)
        }

        #[ink(message)]
        pub fn get_resolving_names_of_address(&self, address: AccountId) -> Option<Vec<String>> {
            self.resolving_to_address.get(address)
        }

        #[ink(message)]
        pub fn get_primary_domain(&self, address: AccountId) -> Result<String> {
            /* Get the naive primary domain of the address */
            let Some(primary_domain) = self.address_to_primary_domain.get(address) else {
                /* No primary domain set */
                return Err(Error::NoResolvedAddress);
            };

            /* Check that the primary domain actually resolves to the claimed address */
            let resolved_address = self.get_address(primary_domain.clone());
            if resolved_address != address {
                /* Resolved address is no longer valid */
                return Err(Error::NoResolvedAddress);
            }

            Ok(primary_domain)
        }

        #[ink(message)]
        pub fn get_names_of_address(&self, address: AccountId) -> Vec<String> {
            let resolved_names = self.get_resolving_names_of_address(address);
            let controlled_names = self.get_controlled_names_of_address(address);
            let owned_names = self.get_owned_names_of_address(address);

            // Using BTreeSet to remove duplicates
            let set: ink::prelude::collections::BTreeSet<String> =
                [resolved_names, controlled_names, owned_names]
                    .into_iter()
                    .filter_map(|x| x)
                    .flatten()
                    .collect();

            set.into_iter().collect()
        }

        /// Returns `true` when contract is in whitelist-phase
        /// and `false` when it is in public-phase
        #[ink(message)]
        pub fn is_whitelist_phase(&self) -> bool {
            self.whitelisted_address_verifier.is_some()
        }

        #[ink(message)]
        pub fn verify_proof(
            &self,
            account: AccountId,
            merkle_proof: Option<Vec<[u8; 32]>>,
        ) -> bool {
            let Some(merkle_proof) = merkle_proof else {
                return false;
            };
            let mut leaf = [0u8; 32];
            ink::env::hash::Sha2x256::hash(account.as_ref(), &mut leaf);

            let Some(verifier) = &self.whitelisted_address_verifier else {
                return false;
            };
            verifier.verify_proof(leaf, merkle_proof)
        }

        /// (ADMIN-OPERATION)
        /// Transfers `value` amount of tokens to the caller.
        #[ink(message)]
        pub fn withdraw(&mut self, value: Balance) -> Result<()> {
            if self.owner != Self::env().caller() {
                return Err(Error::CallerIsNotOwner);
            }

            assert!(value <= Self::env().balance(), "insufficient funds!");

            if Self::env().transfer(Self::env().caller(), value).is_err() {
                return Err(Error::WithdrawFailed);
            }

            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Update the merkle root
        #[ink(message)]
        pub fn update_merkle_root(&mut self, new_root: [u8; 32]) -> Result<()> {
            self.ensure_admin()?;

            let Some(verifier) = self.whitelisted_address_verifier.as_mut() else {
                return Err(Error::OnlyDuringWhitelistPhase);
            };
            verifier.update_root(new_root);

            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Switch from whitelist-phase to public-phase
        #[ink(message)]
        pub fn switch_to_public_phase(&mut self) -> Result<()> {
            self.ensure_admin()?;

            if self.whitelisted_address_verifier.take().is_some() {
                self.env().emit_event(PublicPhaseActivated {});
            }
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Reserve domain name for specific addresses
        // @dev (name, None) denotes that the name is reserved but not tied to any address yet
        #[ink(message)]
        pub fn add_reserved_domains(
            &mut self,
            set: Vec<(String, Option<AccountId>)>,
        ) -> Result<()> {
            if self.owner != self.env().caller() {
                return Err(Error::CallerIsNotOwner);
            }

            set.iter().for_each(|(name, addr)| {
                let addr = addr.unwrap_or(self.default_address);
                self.reserved_names.insert(&name, &addr);
            });
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Remove given names from the list of reserved domains
        #[ink(message)]
        pub fn remove_reserved_domain(&mut self, set: Vec<String>) -> Result<()> {
            if self.owner != self.env().caller() {
                return Err(Error::CallerIsNotOwner);
            }

            set.iter().for_each(|name| self.reserved_names.remove(name));
            Ok(())
        }

        fn ensure_admin(&mut self) -> Result<()> {
            if self.owner != self.env().caller() {
                Err(Error::CallerIsNotOwner)
            } else {
                Ok(())
            }
        }

        fn ensure_controller(&self, address: &AccountId, name: &str) -> Result<()> {
            /* Ensure that the address is a controller of the target domain */
            let controller = self.get_controller_or_default(&name);
            if address != &controller {
                Err(Error::CallerIsNotController)
            } else {
                Ok(())
            }
        }

        fn register_domain(&mut self, name: &str, recipient: &AccountId) -> Result<()> {
            /* Ensure domain is not already registered */
            if self.name_to_address_dict.contains(name) {
                return Err(Error::NameAlreadyExists);
            }

            let address_dict = AddressDict::new(recipient.clone());
            self.name_to_address_dict.insert(name, &address_dict);

            /* Update convenience mapping for owned domains */
            self.add_name_to_owner(recipient, name);

            /* Update convenience mapping for controlled domains */
            self.add_name_to_controller(recipient, name);

            /* Update convenience mapping for resolved domains */
            self.add_name_to_resolving(recipient, name);

            /* Emit register event */
            Self::env().emit_event(Register {
                name: name.to_string(),
                from: *recipient,
            });

            Ok(())
        }

        /// Adds a name to owners' collection
        fn add_name_to_owner(&mut self, owner: &AccountId, name: &str) {
            let mut names = self.owner_to_names.get(owner).unwrap_or_default();
            names.push(name.to_string());
            self.owner_to_names.insert(&owner, &names);
        }

        /// Adds a name to controllers' collection
        fn add_name_to_controller(&mut self, controller: &AccountId, name: &str) {
            let mut names = self.controller_to_names.get(controller).unwrap_or_default();
            names.push(name.to_string());
            self.controller_to_names.insert(&controller, &names);
        }

        /// Adds a name to resolvings' collection
        fn add_name_to_resolving(&mut self, resolving: &AccountId, name: &str) {
            let mut names = self.resolving_to_address.get(resolving).unwrap_or_default();
            names.push(name.to_string());
            self.resolving_to_address.insert(&resolving, &names);
        }

        /// Deletes a name from owner
        fn remove_name_from_owner(&mut self, owner: &AccountId, name: &str) {
            if let Some(old_names) = self.owner_to_names.get(owner) {
                let mut new_names: Vec<String> = old_names;
                new_names.retain(|prevname| prevname != name);
                self.owner_to_names.insert(owner, &new_names);
            }
        }

        /// Deletes a name from controllers' collection
        fn remove_name_from_controller(&mut self, controller: &AccountId, name: &str) {
            self.controller_to_names.get(controller).map(|mut names| {
                names.retain(|ele| ele != name);
                self.controller_to_names.insert(&controller, &names);
            });
        }

        /// Deletes a name from resolvings' collection
        fn remove_name_from_resolving(&mut self, resolving: &AccountId, name: &str) {
            self.resolving_to_address.get(resolving).map(|mut names| {
                names.retain(|ele| ele != name);
                self.resolving_to_address.insert(&resolving, &names);
            });
        }

        fn is_name_allowed(&self, name: &str) -> bool {
            /* Name cannot be empty */
            if name.is_empty() {
                return false;
            }

            /* Name must be legal */
            if let Some(name_checker) = &self.name_checker {
                if name_checker.is_name_allowed(name.to_string()) != Ok(true) {
                    return false;
                }
            }
            true
        }

        fn get_address_dict_or_default(&self, name: &str) -> AddressDict {
            self.name_to_address_dict
                .get(name)
                .unwrap_or_else(|| AddressDict::new(self.default_address))
        }

        /// Returns the owner given the String or the default address.
        fn get_owner_or_default(&self, name: &str) -> AccountId {
            self.get_address_dict_or_default(name).owner
        }

        fn get_controller_or_default(&self, name: &str) -> AccountId {
            self.get_address_dict_or_default(name).controller
        }

        /// Returns the address given the String or the default address.
        fn get_resolved_address_or_default(&self, name: &str) -> AccountId {
            self.get_address_dict_or_default(name).resolved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::azns_registry::*;
    use ink::env::test::*;
    use ink::env::DefaultEnvironment;
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec::Vec;
    use ink::primitives::AccountId;
    type Balance = u128;

    fn default_accounts() -> DefaultAccounts<DefaultEnvironment> {
        ink::env::test::default_accounts::<DefaultEnvironment>()
    }

    fn set_next_caller(caller: AccountId) {
        set_caller::<DefaultEnvironment>(caller);
    }

    fn get_test_name_service() -> DomainNameService {
        DomainNameService::new(None, None, [0u8; 32], None, None)
    }

    #[ink::test]
    fn owner_to_names_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None, false), Ok(()));

        /* Now alice owns three domains */
        /* getting all owned domains should return all three */
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(vec![name, name2, name3])
        );
    }

    #[ink::test]
    fn controller_to_names_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None, false), Ok(()));

        /* Register bar under bob, but set controller to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None, false), Ok(()));
        assert_eq!(
            contract.set_controller(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* Now alice owns three domains */
        /* getting all owned domains should return all three */
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            Some(vec![name, name2, name3])
        );
    }

    #[ink::test]
    fn get_names_of_address_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        set_next_caller(default_accounts.charlie);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None, false), Ok(()));

        /* getting all domains should return first only */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name.clone()]
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None, false), Ok(()));
        assert_eq!(
            contract.set_address(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* getting all domains should return all three */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name3.clone(), name.clone()]
        );

        set_next_caller(default_accounts.charlie);
        assert_eq!(
            contract.set_controller(name2.clone(), default_accounts.alice),
            Ok(())
        );

        /* getting all domains should return all three */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name3, name2, name]
        );
    }

    #[ink::test]
    fn resolving_to_address_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None, false), Ok(()));

        /* getting all domains should return first two */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![name.clone(), name2.clone()])
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None, false), Ok(()));
        assert_eq!(
            contract.set_address(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* Now all three domains resolve to alice's address */
        /* getting all resolving domains should return all three names */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![name.clone(), name2.clone(), name3.clone()])
        );

        /* Remove the pointer to alice */
        assert_eq!(contract.set_address(name3, default_accounts.bob), Ok(()));

        /* getting all resolving domains should return first two names */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![name, name2])
        );
    }

    #[ink::test]
    fn set_primary_domain_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2, None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3, None, false), Ok(()));

        /* Now alice owns three domains */
        /* Set the primary domain for alice's address to domain 1 */
        contract
            .set_primary_domain(default_accounts.alice, name.clone())
            .unwrap();

        /* Now the primary domain should resolve to alice's address */
        assert_eq!(
            contract.get_primary_domain(default_accounts.alice),
            Ok(name.clone())
        );

        /* Change the resolved address of the first domain to bob, invalidating the primary domain claim */
        contract
            .set_address(name.clone(), default_accounts.bob)
            .unwrap();

        /* Now the primary domain should not resolve to anything */
        assert_eq!(
            contract.get_primary_domain(default_accounts.alice),
            Err(Error::NoResolvedAddress)
        );

        /* Set bob's primary domain */
        contract
            .set_primary_domain(default_accounts.bob, name.clone())
            .unwrap();

        /* Now the primary domain should not resolve to anything */
        assert_eq!(contract.get_primary_domain(default_accounts.bob), Ok(name));
    }

    #[ink::test]
    fn register_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(
            contract.register(name, None, false),
            Err(Error::NameAlreadyExists)
        );

        // Reserved names cannot be registered
        let reserved_name = String::from("AlephZero");
        let reserved_list = vec![(reserved_name.clone(), Some(default_accounts.alice))];
        contract
            .add_reserved_domains(reserved_list)
            .expect("Failed to reserve domain");

        assert_eq!(
            contract.register(reserved_name, None, false),
            Err(Error::CannotBuyReservedDomain)
        );
    }

    #[ink::test]
    fn register_with_set_primary_domain_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, true), Ok(()));

        assert_eq!(
            contract.get_primary_domain(default_accounts.alice),
            Ok(name)
        );
    }

    #[ink::test]
    fn withdraw_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        let acc_balance_before_transfer: Balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name, None, false), Ok(()));
        assert_eq!(contract.withdraw(160 ^ 12), Ok(()));
        let acc_balance_after_withdraw: Balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        assert_eq!(
            (acc_balance_before_transfer + 160) ^ 12,
            acc_balance_after_withdraw
        );
    }

    #[ink::test]
    fn withdraw_only_owner() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        let _acc_balance_before_transfer: Balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name, None, false), Ok(()));

        set_next_caller(default_accounts.bob);
        assert_eq!(contract.withdraw(160 ^ 12), Err(Error::CallerIsNotOwner));
    }

    #[ink::test]
    fn reverse_search_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("test2");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name, None, false), Ok(()));
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2, None, false), Ok(()));
        assert!(contract
            .get_owned_names_of_address(default_accounts.alice)
            .unwrap()
            .contains(&String::from("test")));
        assert!(contract
            .get_owned_names_of_address(default_accounts.alice)
            .unwrap()
            .contains(&String::from("test2")));
    }

    #[ink::test]
    fn register_empty_reverts() {
        let default_accounts = default_accounts();
        let name = String::from("");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(
            contract.register(name, None, false),
            Err(Error::NameNotAllowed)
        );
    }

    // TODO: enable this test once we get cross-contract testing working
    // #[ink::test]
    // fn register_disallowed_reverts() {
    //     let default_accounts = default_accounts();
    //     let name = String::from("ýáěšžčřýáěščžá");
    //
    //     set_next_caller(default_accounts.alice);
    //     let mut contract = get_test_name_service();
    //
    //     set_value_transferred::<DefaultEnvironment>(160 ^ 12);
    //     assert_eq!(contract.register(name, None), Err(NameNotAllowed, false));
    // }

    #[ink::test]
    fn register_with_fee_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));
        assert_eq!(
            contract.register(name, None, false),
            Err(Error::NameAlreadyExists)
        );
    }

    #[ink::test]
    fn register_without_fee_reverts() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        assert_eq!(contract.register(name, None, false), Err(Error::FeeNotPaid));
    }

    #[ink::test]
    fn release_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));
        assert_eq!(
            contract.set_address(name.clone(), default_accounts.alice),
            Ok(())
        );
        assert_eq!(contract.get_owner(name.clone()), default_accounts.alice);
        assert_eq!(contract.get_address(name.clone()), default_accounts.alice);
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );

        assert_eq!(contract.release(name.clone()), Ok(()));
        assert_eq!(contract.get_owner(name.clone()), Default::default());
        assert_eq!(contract.get_address(name.clone()), Default::default());
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([]))
        );

        /* Another account can register again*/
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));
        assert_eq!(
            contract.set_address(name.clone(), default_accounts.bob),
            Ok(())
        );
        assert_eq!(contract.get_owner(name.clone()), default_accounts.bob);
        assert_eq!(contract.get_address(name.clone()), default_accounts.bob);
        assert_eq!(contract.release(name.clone()), Ok(()));
        assert_eq!(contract.get_owner(name.clone()), Default::default());
        assert_eq!(contract.get_address(name), Default::default());
    }

    #[ink::test]
    fn controller_separation_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        contract.register(name.clone(), None, false).unwrap();

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        /* Caller is not controller, `set_all_records` should fail */
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_all_records(
                name.clone(),
                Vec::from([("twitter".to_string(), "@newtest".to_string())])
            ),
            Err(Error::CallerIsNotController)
        );

        // Caller is controller, `set_all_records` should pass
        set_next_caller(accounts.alice);
        assert_eq!(
            contract.set_all_records(
                name,
                Vec::from([("twitter".to_string(), "@newtest".to_string())])
            ),
            Ok(())
        );
    }

    #[ink::test]
    fn set_address_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        // Caller is controller, set_address will be successful
        set_next_caller(accounts.alice);
        assert_eq!(contract.set_address(name.clone(), accounts.bob), Ok(()));
        assert_eq!(contract.get_address(name), accounts.bob);
    }

    #[ink::test]
    fn transfer_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None, false), Ok(()));

        // Test transfer of owner.
        assert_eq!(contract.transfer(name.clone(), accounts.bob), Ok(()));

        assert_eq!(
            contract.get_owned_names_of_address(accounts.alice),
            Some(Vec::from([]))
        );
        assert_eq!(
            contract.get_owned_names_of_address(accounts.bob),
            Some(Vec::from([name.clone()]))
        );

        // Alice is not the controller anymore
        assert_eq!(
            contract.set_controller(name.clone(), accounts.bob),
            Err(Error::CallerIsNotOwner)
        );

        // Controller is bob, alice `set_address` should fail.
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        set_next_caller(accounts.bob);
        // Now owner is bob, `set_address` should be successful.
        assert_eq!(contract.set_address(name.clone(), accounts.eve), Ok(()));
        assert_eq!(contract.get_address(name), accounts.eve);
    }

    #[ink::test]
    fn metadata_works() {
        let accounts = default_accounts();
        let key = String::from("twitter");
        let value = String::from("@test");
        let records = Vec::from([(key.clone(), value.clone())]);

        let domain_name = "test".to_string();

        set_next_caller(accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(domain_name.clone(), None, false), Ok(()));

        assert_eq!(
            contract.set_all_records(domain_name.clone(), records.clone()),
            Ok(())
        );
        assert_eq!(
            contract
                .get_metadata_by_key(domain_name.clone(), key.clone())
                .unwrap(),
            value
        );

        /* Confirm idempotency */
        assert_eq!(
            contract.set_all_records(domain_name.clone(), records),
            Ok(())
        );
        assert_eq!(
            contract
                .get_metadata_by_key(domain_name.clone(), key)
                .unwrap(),
            value
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
            contract.get_metadata(domain_name).unwrap(),
            Vec::from([("twitter".to_string(), "@newtest".to_string())])
        );
    }

    #[ink::test]
    fn add_reserved_domains_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let reserved_name = String::from("AlephZero");
        let list = vec![(reserved_name.clone(), Some(accounts.alice))];

        assert!(contract.add_reserved_domains(list).is_ok());

        assert_eq!(
            contract.get_domain_status(reserved_name),
            DomainStatus::Reserved(accounts.alice),
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.add_reserved_domains(vec![]),
            Err(Error::CallerIsNotOwner)
        );
    }

    #[ink::test]
    fn remove_reserved_domains_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let reserved_name = String::from("AlephZero");
        let list = vec![(reserved_name.clone(), Some(accounts.alice))];
        assert!(contract.add_reserved_domains(list).is_ok());

        assert_eq!(
            contract.get_domain_status(reserved_name.clone()),
            DomainStatus::Reserved(accounts.alice),
        );

        assert!(contract
            .remove_reserved_domain(vec![reserved_name.clone()])
            .is_ok());

        assert_ne!(
            contract.get_domain_status(reserved_name),
            DomainStatus::Reserved(accounts.alice),
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.remove_reserved_domain(vec![]),
            Err(Error::CallerIsNotOwner)
        );
    }

    #[ink::test]
    fn claim_reserved_domain_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let name = String::from("bob");
        let reserved_list = vec![(name.clone(), Some(accounts.bob))];
        contract
            .add_reserved_domains(reserved_list)
            .expect("Failed to add reserved domain");

        // Non-reserved domain cannot be claimed
        assert_eq!(
            contract.claim_reserved_domain("abcd".to_string()),
            Err(Error::NotReservedDomain),
        );

        // Non-authorised user cannot claim reserved domain
        assert_eq!(
            contract.claim_reserved_domain(name.clone()),
            Err(Error::NotAuthorised),
        );

        // Authorised user can claim domain reserved for them
        set_next_caller(accounts.bob);
        assert!(contract.claim_reserved_domain(name.clone()).is_ok());

        assert_eq!(
            contract.get_domain_status(name),
            DomainStatus::Registered(accounts.bob),
        );
    }

    #[ink::test]
    fn get_domain_status_works() {
        let accounts = default_accounts();
        let reserved_list = vec![("bob".to_string(), Some(accounts.bob))];
        let mut contract = DomainNameService::new(None, None, [0u8; 32], Some(reserved_list), None);

        set_value_transferred::<DefaultEnvironment>(160_u128.pow(12));
        contract
            .register("alice".to_string(), None, false)
            .expect("failed to register domain");

        assert_eq!(
            contract.get_domain_status("alice".to_string()),
            DomainStatus::Registered(accounts.alice)
        );

        assert_eq!(
            contract.get_domain_status("bob".to_string()),
            DomainStatus::Reserved(accounts.bob)
        );

        assert_eq!(
            contract.get_domain_status("david".to_string()),
            DomainStatus::Available
        );

        assert_eq!(
            contract.get_domain_status("".to_string()),
            DomainStatus::Unavailable
        );
    }

    // TODO: Finish this test once we get cross-contract testing working
    // #[ink::test]
    // fn whitelist_phase_works() {
    //     // 1. Init (whitelist-phase)

    //     // 2. Verify an empty proof fails

    //     // 3. Verify that an invalid proof fails

    //     // 4. Verify that valid proof works and the domain is registered

    //     // 5. Verify a user can claim only one domain during whitelist-phase

    //     // 6. Verify `release()` fails

    //     // 7. Verify `transfer()` fails

    //     // 8. Verify `switch_to_public_phase()` works
    // }
}
