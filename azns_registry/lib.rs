#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod azns_registry {
    use crate::azns_registry::Error::{
        CallerIsNotController, CallerIsNotOwner, NoRecordsForAddress, NoResolvedAddress,
        RecordNotFound, WithdrawFailed,
    };
    use azns_name_checker::get_domain_price;
    use ink::env::hash::CryptoHash;
    use ink::prelude::borrow::ToOwned;
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

    /// Emitted when switching from whitelist-phase to public-phase
    #[ink(event)]
    pub struct PublicPhaseActivated;

    #[ink(storage)]
    pub struct DomainNameService {
        name_checker: Option<NameCheckerRef>,
        /// A mapping to set a controller for each address
        name_to_controller: Mapping<String, ink::primitives::AccountId>,
        /// A Stringmap to store all name to addresses mapping.
        name_to_address: Mapping<String, ink::primitives::AccountId>,
        /// A Stringmap to store all name to owners mapping.
        name_to_owner: Mapping<String, ink::primitives::AccountId>,
        /// The default address.
        default_address: ink::primitives::AccountId,
        /// Owner of the contract
        /// can withdraw funds
        owner: ink::primitives::AccountId,
        /// All names an address owns
        owner_to_names: Mapping<ink::primitives::AccountId, Vec<String>>,
        /// All names an address controls
        controller_to_names: Mapping<ink::primitives::AccountId, Vec<String>>,
        /// All names that resolve to the given address
        resolving_to_address: Mapping<ink::primitives::AccountId, Vec<String>>,
        /// Metadata
        additional_info: Mapping<String, Vec<(String, String)>>,
        /// Primary domain record
        /// IMPORTANT NOTE: This mapping may be out-of-date, since we don't update it when a resolved address changes, or when a domain is withdrawn.
        /// Only use the get_primary_domain
        address_to_primary_domain: Mapping<ink::primitives::AccountId, String>,
        /// Merkle Verifier used to identifiy whitelisted addresses
        whitelisted_address_verifier: Option<MerkleVerifierRef>,
        /// Names which can be claimed only by the specified user
        reserved_names: Mapping<String, AccountId>,
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
                    .expect("failed at instantiating the `NameCheckerRef` contract")
            });

            // Initializing MerkleVerifier
            let whitelisted_address_verifier = merkle_verifier_hash.map(|ch| {
                MerkleVerifierRef::new(merkle_root)
                    .endowment(total_balance / 4) // TODO why /4?
                    .code_hash(ch)
                    .salt_bytes(salt)
                    .instantiate()
                    .expect("failed at instantiating the `MerkleVerifierRef` contract")
            });

            let mut contract = Self {
                owner: caller,
                name_checker,
                name_to_controller: Mapping::default(),
                name_to_address: Mapping::default(),
                name_to_owner: Mapping::default(),
                default_address: Default::default(),
                owner_to_names: Default::default(),
                additional_info: Default::default(),
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

        /// Transfers `value` amount of tokens to the caller.
        /// Used for withdrawing funds from the main contract to the MultiSig
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

        /// Set primary domain of an address (reverse record)
        #[ink(message, payable)]
        pub fn set_primary_domain(
            &mut self,
            address: ink::primitives::AccountId,
            name: String,
        ) -> Result<()> {
            /* Ensure the caller controls the target name */
            self.ensure_controller(Self::env().caller(), name.clone())?;

            /* Ensure the target name resolves to something */
            let Some(resolved) = self.name_to_address.get(name.clone()) else {
                return Err(NoResolvedAddress);
             };

            /* Ensure the target name resolves to the address */
            if resolved != address {
                return Err(NoResolvedAddress);
            }

            self.address_to_primary_domain.insert(address, &name);

            Ok(())
        }

        #[ink(message)]
        pub fn get_primary_domain(&self, address: ink::primitives::AccountId) -> Result<String> {
            /* Get the naive primary domain of the address */
            let Some(primary_domain) = self.address_to_primary_domain.get(address) else {
                /* No primary domain set */
                return Err(NoResolvedAddress);
            };

            /* Check that the primary domain actually resolves to the claimed address */
            let resolved_address = self.get_address(primary_domain.clone());
            if resolved_address != address {
                /* Resolved address is no longer valid */
                return Err(NoResolvedAddress);
            }

            Ok(primary_domain)
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

        /// Register specific name on behalf of some other address.
        /// Pay the fee, but forward the ownership of the domain to the provided recipient
        ///
        /// NOTE: During the whitelist phase, use `register()` method instead.
        #[ink(message, payable)]
        pub fn register_on_behalf_of(
            &mut self,
            name: String,
            recipient: ink::primitives::AccountId,
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
        ) -> Result<()> {
            self.register_on_behalf_of(name, self.env().caller(), merkle_proof)
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

        /// (ADMIN-OPERATION)
        /// Reserve domain name for specific addresses
        // @dev (name, None) denotes that the name is reserved but not tied to any address yet
        #[ink(message)]
        pub fn add_reserved_domains(
            &mut self,
            set: Vec<(String, Option<AccountId>)>,
        ) -> Result<()> {
            if self.owner != self.env().caller() {
                return Err(CallerIsNotOwner);
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
                return Err(CallerIsNotOwner);
            }

            set.iter().for_each(|name| self.reserved_names.remove(name));
            Ok(())
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
                return Err(CallerIsNotOwner);
            }

            self.name_to_owner.remove(&name);
            self.name_to_address.remove(&name);
            self.remove_name_from_owner(caller, name.clone());
            self.name_to_controller.remove(&name);
            self.additional_info.remove(&name);

            Self::env().emit_event(Release { name, from: caller });

            Ok(())
        }

        #[ink(message)]
        pub fn get_names_of_address(&self, address: ink::primitives::AccountId) -> Vec<String> {
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

        #[ink(message)]
        pub fn get_resolving_names_of_address(
            &self,
            address: ink::primitives::AccountId,
        ) -> Option<Vec<String>> {
            self.resolving_to_address.get(address)
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
            self.ensure_controller(caller, name.clone())?;

            let old_address = self.name_to_address.get(&name);
            self.name_to_address.insert(&name, &new_address);

            /* Check if the old resolved address had this domain set as the primary domain */
            /* If yes -> clear it */
            if let Some(old_address_exists) = old_address {
                if let Some(primary_domain) = self.address_to_primary_domain.get(old_address_exists)
                {
                    if primary_domain == name {
                        self.address_to_primary_domain.remove(old_address_exists);
                    }
                }
            }

            /* Remove the name from the old resolved address */
            if let Some(old_address) = old_address {
                if let Some(names) = self.resolving_to_address.get(old_address) {
                    let mut new_names = names;
                    new_names.retain(|x| *x != name.clone());
                    self.resolving_to_address.insert(old_address, &new_names);
                }
            }

            /* Add the name to the new resolved address */
            if let Some(names) = self.controller_to_names.get(new_address) {
                let mut new_names = names;
                new_names.push(name.clone());
                self.resolving_to_address.insert(new_address, &new_names);
            } else {
                self.resolving_to_address
                    .insert(new_address, &Vec::from([name.to_string()]));
            }

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
            // Transfer is disabled during the whitelist-phase
            if self.is_whitelist_phase() {
                return Err(Error::RestrictedDuringWhitelistPhase);
            }

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
                let mut new_names = names;
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

        #[ink(message)]
        pub fn get_controlled_names_of_address(
            &self,
            controller: ink::primitives::AccountId,
        ) -> Option<Vec<String>> {
            self.controller_to_names.get(controller)
        }

        #[ink(message)]
        pub fn set_controller(
            &mut self,
            name: String,
            new_controller: ink::primitives::AccountId,
        ) -> Result<()> {
            /* Ensure caller is either controller or owner */
            let caller = Self::env().caller();
            let owner = self.get_owner_or_default(&name);
            let controller = self.get_controller_or_default(&name);

            if caller != owner && caller != controller {
                return Err(CallerIsNotOwner);
            }

            self.name_to_controller.insert(&name, &new_controller);

            /* Remove the name from the old controller */
            if let Some(names) = self.controller_to_names.get(caller) {
                let mut new_names = names;
                new_names.retain(|x| *x != name);
                self.controller_to_names.insert(controller, &new_names);
            }

            /* Add the name to the new controller */
            if let Some(names) = self.controller_to_names.get(new_controller) {
                let mut new_names = names;
                new_names.push(name);
                self.controller_to_names.insert(new_controller, &new_names);
            } else {
                self.controller_to_names
                    .insert(new_controller, &Vec::from([name.to_string()]));
            }

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

        fn ensure_admin(&mut self) -> Result<()> {
            if self.owner != self.env().caller() {
                Err(CallerIsNotOwner)
            } else {
                Ok(())
            }
        }

        /// Returns `true` when contract is in whitelist-phase
        /// and `false` when it is in public-phase
        #[ink(message)]
        pub fn is_whitelist_phase(&self) -> bool {
            self.whitelisted_address_verifier.is_some()
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
                .get(name)
                .unwrap_or(self.default_address)
        }

        /// Returns the owner given the String or the default address.
        fn get_owner_or_default(&self, name: &String) -> ink::primitives::AccountId {
            self.name_to_owner.get(name).unwrap_or(self.default_address)
        }

        /// Returns the address given the String or the default address.
        fn get_address_or_default(&self, name: String) -> ink::primitives::AccountId {
            self.name_to_address
                .get(&name)
                .unwrap_or(self.default_address)
        }

        /// Returns all names the address owns
        #[ink(message)]
        pub fn get_owned_names_of_address(
            &self,
            owner: ink::primitives::AccountId,
        ) -> Option<Vec<String>> {
            self.owner_to_names.get(owner)
        }

        /// Returns the current status of the domain
        #[ink(message)]
        pub fn get_domain_status(&self, name: String) -> DomainStatus {
            if let Some(user) = self.name_to_owner.get(&name) {
                DomainStatus::Registered(user)
            } else if let Some(user) = self.reserved_names.get(&name) {
                DomainStatus::Reserved(user)
            } else if self.is_name_allowed(&name) {
                DomainStatus::Available
            } else {
                DomainStatus::Unavailable
            }
        }

        fn register_domain(
            &mut self,
            name: &str,
            recipient: &ink::primitives::AccountId,
        ) -> Result<()> {
            /* Ensure domain is not already registered */
            if self.name_to_owner.contains(name) {
                return Err(Error::NameAlreadyExists);
            }

            /* Set domain owner */
            self.name_to_owner.insert(name, recipient);

            /* Set domain controller */
            self.name_to_controller.insert(name, recipient);

            /* Set resolved domain */
            self.name_to_address.insert(name, recipient);

            /* Update convenience mapping for owned domains */
            let previous_names = self.owner_to_names.get(recipient);
            if let Some(names) = previous_names {
                let mut new_names = names;
                new_names.push(name.to_string());
                self.owner_to_names.insert(recipient, &new_names);
            } else {
                self.owner_to_names
                    .insert(recipient, &Vec::from([name.to_string()]));
            }

            /* Update convenience mapping for controlled domains */
            if let Some(names) = self.controller_to_names.get(recipient) {
                let mut new_names = names;
                new_names.push(name.to_owned());
                self.controller_to_names.insert(recipient, &new_names);
            } else {
                self.controller_to_names
                    .insert(recipient, &Vec::from([name.to_string()]));
            }

            /* Update convenience mapping for resolved domains */
            if let Some(names) = self.resolving_to_address.get(recipient) {
                let mut new_names = names;
                new_names.push(name.to_owned());
                self.resolving_to_address.insert(recipient, &new_names);
            } else {
                self.resolving_to_address
                    .insert(recipient, &Vec::from([name.to_string()]));
            }

            /* Emit register event */
            Self::env().emit_event(Register {
                name: name.to_string(),
                from: *recipient,
            });

            Ok(())
        }

        /// Deletes a name from owner
        fn remove_name_from_owner(&mut self, owner: ink::primitives::AccountId, name: String) {
            if let Some(old_names) = self.owner_to_names.get(owner) {
                let mut new_names: Vec<String> = old_names;
                new_names.retain(|prevname| prevname.clone() != name);
                self.owner_to_names.insert(owner, &new_names);
            }
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

        /// Gets an arbitrary record by key
        #[ink(message)]
        pub fn get_record(&self, name: String, key: String) -> Result<String> {
            return if let Some(info) = self.additional_info.get(name) {
                if let Some(value) = info.iter().find(|tuple| tuple.0 == key) {
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
            self.ensure_controller(caller, name.clone())?;

            self.additional_info.insert(name, &records);

            Ok(())
        }

        /// Gets all records
        #[ink(message)]
        pub fn get_all_records(&self, name: String) -> Result<Vec<(String, String)>> {
            if let Some(info) = self.additional_info.get(name) {
                Ok(info)
            } else {
                Err(NoRecordsForAddress)
            }
        }

        fn ensure_controller(
            &self,
            address: ink::primitives::AccountId,
            name: String,
        ) -> Result<()> {
            /* Ensure that the address is a controller of the target domain */
            let controller = self.get_controller_or_default(&name);
            if address != controller {
                Err(CallerIsNotController)
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec::Vec;

    use ink::env::DefaultEnvironment;

    use ink::env::test::*;

    type Balance = u128;

    use crate::azns_registry::DomainNameService;
    use crate::azns_registry::Error::{
        CallerIsNotController, CallerIsNotOwner, FeeNotPaid, NameAlreadyExists, NameEmpty,
        NoResolvedAddress,
    };

    use super::azns_registry::Error;

    fn default_accounts() -> DefaultAccounts<DefaultEnvironment> {
        ink::env::test::default_accounts::<DefaultEnvironment>()
    }

    fn set_next_caller(caller: ink::primitives::AccountId) {
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None), Ok(()));

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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None), Ok(()));

        /* Register bar under bob, but set controller to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None), Ok(()));
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        set_next_caller(default_accounts.charlie);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None), Ok(()));

        /* getting all domains should return first only */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name.clone()]
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None), Ok(()));
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2.clone(), None), Ok(()));

        /* getting all domains should return first two */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![name.clone(), name2.clone()])
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3.clone(), None), Ok(()));
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2, None), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name3, None), Ok(()));

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
            Err(NoResolvedAddress)
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name, None), Err(NameAlreadyExists));

        // Reserved names cannot be registered
        let reserved_name = String::from("AlephZero");
        let reserved_list = vec![(reserved_name.clone(), Some(default_accounts.alice))];
        contract
            .add_reserved_domains(reserved_list)
            .expect("Failed to reserve domain");

        assert_eq!(
            contract.register(reserved_name, None),
            Err(Error::CannotBuyReservedDomain)
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
        assert_eq!(contract.register(name, None), Ok(()));
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
        assert_eq!(contract.register(name, None), Ok(()));

        set_next_caller(default_accounts.bob);
        assert_eq!(contract.withdraw(160 ^ 12), Err(CallerIsNotOwner));
    }

    #[ink::test]
    fn reverse_search_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("test2");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name, None), Ok(()));
        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name2, None), Ok(()));
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
        assert_eq!(contract.register(name, None), Err(Error::NameNotAllowed));
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
    //     assert_eq!(contract.register(name, None), Err(NameNotAllowed));
    // }

    #[ink::test]
    fn register_with_fee_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None), Ok(()));
        assert_eq!(contract.register(name, None), Err(NameAlreadyExists));
    }

    #[ink::test]
    fn register_without_fee_reverts() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        assert_eq!(contract.register(name, None), Err(FeeNotPaid));
    }

    #[ink::test]
    fn release_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(name.clone(), None), Ok(()));
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));
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
        contract.register(name.clone(), None).unwrap();

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(CallerIsNotController)
        );

        /* Caller is not controller, `set_all_records` should fail */
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_all_records(
                name.clone(),
                Vec::from([("twitter".to_string(), "@newtest".to_string())])
            ),
            Err(CallerIsNotController)
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(CallerIsNotController)
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
        assert_eq!(contract.register(name.clone(), None), Ok(()));

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

        contract.set_controller(name.clone(), accounts.bob).unwrap();
        // Controller is bob, alice `set_address` should fail.
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(CallerIsNotController)
        );

        set_next_caller(accounts.bob);
        // Now owner is bob, `set_address` should be successful.
        assert_eq!(contract.set_address(name.clone(), accounts.bob), Ok(()));
        assert_eq!(contract.get_address(name), accounts.bob);
    }

    #[ink::test]
    fn additional_data_works() {
        let accounts = default_accounts();
        let key = String::from("twitter");
        let value = String::from("@test");
        let records = Vec::from([(key.clone(), value.clone())]);

        let domain_name = "test".to_string();

        set_next_caller(accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160 ^ 12);
        assert_eq!(contract.register(domain_name.clone(), None), Ok(()));

        assert_eq!(
            contract.set_all_records(domain_name.clone(), records.clone()),
            Ok(())
        );
        assert_eq!(
            contract
                .get_record(domain_name.clone(), key.clone())
                .unwrap(),
            value
        );

        /* Confirm idempotency */
        assert_eq!(
            contract.set_all_records(domain_name.clone(), records),
            Ok(())
        );
        assert_eq!(
            contract.get_record(domain_name.clone(), key).unwrap(),
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
            contract.get_all_records(domain_name).unwrap(),
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
            azns_registry::DomainStatus::Reserved(accounts.alice),
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
            azns_registry::DomainStatus::Reserved(accounts.alice),
        );

        assert!(contract
            .remove_reserved_domain(vec![reserved_name.clone()])
            .is_ok());

        assert_ne!(
            contract.get_domain_status(reserved_name),
            azns_registry::DomainStatus::Reserved(accounts.alice),
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
            azns_registry::DomainStatus::Registered(accounts.bob),
        );
    }

    #[ink::test]
    fn get_domain_status_works() {
        let accounts = default_accounts();
        let reserved_list = vec![("bob".to_string(), Some(accounts.bob))];
        let mut contract = DomainNameService::new(None, None, [0u8; 32], Some(reserved_list), None);

        set_value_transferred::<DefaultEnvironment>(160_u128.pow(12));
        contract
            .register("alice".to_string(), None)
            .expect("failed to register domain");

        assert_eq!(
            contract.get_domain_status("alice".to_string()),
            azns_registry::DomainStatus::Registered(accounts.alice)
        );

        assert_eq!(
            contract.get_domain_status("bob".to_string()),
            azns_registry::DomainStatus::Reserved(accounts.bob)
        );

        assert_eq!(
            contract.get_domain_status("david".to_string()),
            azns_registry::DomainStatus::Available
        );

        assert_eq!(
            contract.get_domain_status("".to_string()),
            azns_registry::DomainStatus::Unavailable
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
