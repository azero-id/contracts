#![cfg_attr(not(feature = "std"), no_std)]

mod address_dict;

#[ink::contract]
mod azns_registry {
    use crate::address_dict::AddressDict;
    use ink::env::call::FromAccountId;
    use ink::env::hash::CryptoHash;
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec::Vec;
    use ink::storage::traits::ManualKey;
    use ink::storage::{Lazy, Mapping};

    use azns_fee_calculator::FeeCalculatorRef;
    use azns_merkle_verifier::MerkleVerifierRef;
    use azns_name_checker::NameCheckerRef;

    const YEAR: u64 = match cfg!(test) {
        true => 60,                         // For testing purpose
        false => 365 * 24 * 60 * 60 * 1000, // Year in milliseconds
    };

    pub type Result<T> = core::result::Result<T, Error>;

    /// Different states of a domain
    #[derive(scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, Debug, PartialEq))]
    pub enum DomainStatus {
        /// Domain is registered with the given AddressDict
        Registered(AddressDict),
        /// Domain is reserved for the given address
        Reserved(Option<AccountId>),
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
        /// Admin of the contract can perform root operations
        admin: AccountId,
        /// Contract which verifies the validity of a name
        name_checker: Option<NameCheckerRef>,
        /// Contract which calculates the name price
        fee_calculator: Option<FeeCalculatorRef>,
        /// Names which can be claimed only by the specified user
        reserved_names: Mapping<String, Option<AccountId>, ManualKey<100>>,

        /// Mapping from name to addresses associated with it
        name_to_address_dict: Mapping<String, AddressDict, ManualKey<200>>,
        /// Mapping from name to its expiry timestamp
        name_to_expiry: Mapping<String, u64>,
        /// Metadata
        metadata: Mapping<String, Vec<(String, String)>, ManualKey<201>>,
        metadata_size_limit: Option<u32>,

        /// All names an address owns
        owner_to_names: Mapping<AccountId, Vec<String>, ManualKey<300>>,
        /// All names an address controls
        controller_to_names: Mapping<AccountId, Vec<String>, ManualKey<301>>,
        /// All names that resolve to the given address
        resolving_to_address: Mapping<AccountId, Vec<String>, ManualKey<302>>,
        /// Primary domain record
        /// IMPORTANT NOTE: This mapping may be out-of-date, since we don't update it when a resolved address changes, or when a domain is withdrawn.
        /// Only use the get_primary_domain
        address_to_primary_domain: Mapping<AccountId, String, ManualKey<303>>,

        /// Merkle Verifier used to identifiy whitelisted addresses
        whitelisted_address_verifier: Lazy<Option<MerkleVerifierRef>, ManualKey<999>>,

        /// TLD
        tld: String,
    }

    /// Errors that can occur upon calling this contract.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Caller not allowed to call privileged calls.
        NotAdmin,
        /// Returned if the name already exists upon registration.
        NameAlreadyExists,
        /// Returned if the name has not been registered
        NameDoesntExist,
        /// Name is (currently) now allowed
        NameNotAllowed,
        /// Returned if caller is not names' owner.
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
        /// Metadata size limit exceeded
        MetadataOverflow,
        /// Thrown when fee_calculator doesn't return a names' price
        FeeError(azns_fee_calculator::Error),
    }

    impl DomainNameService {
        /// Creates a new AZNS contract.
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            name_checker_addr: Option<AccountId>,
            fee_calculator_addr: Option<AccountId>,
            merkle_verifier_addr: Option<AccountId>,
            reserved_domains: Option<Vec<(String, Option<AccountId>)>>,
            tld: String,
            metadata_size_limit: Option<u32>,
        ) -> Self {
            // Initializing NameChecker
            let name_checker = name_checker_addr.map(|addr| NameCheckerRef::from_account_id(addr));

            // Initializing MerkleVerifier
            let whitelisted_address_verifier =
                merkle_verifier_addr.map(|addr| MerkleVerifierRef::from_account_id(addr));

            // Initializing FeeCalculator
            let fee_calculator =
                fee_calculator_addr.map(|addr| FeeCalculatorRef::from_account_id(addr));

            let mut contract = Self {
                admin,
                name_checker,
                fee_calculator,
                name_to_address_dict: Mapping::default(),
                name_to_expiry: Mapping::default(),
                owner_to_names: Default::default(),
                metadata: Default::default(),
                address_to_primary_domain: Default::default(),
                controller_to_names: Default::default(),
                resolving_to_address: Default::default(),
                whitelisted_address_verifier: Default::default(),
                reserved_names: Default::default(),
                tld,
                metadata_size_limit,
            };

            // Initialize address verifier
            contract
                .whitelisted_address_verifier
                .set(&whitelisted_address_verifier);

            // Initializing reserved domains
            if let Some(set) = reserved_domains {
                contract.add_reserved_domains(set).expect("Infallible");
            }

            // No Whitelist phase
            if merkle_verifier_addr == None {
                Self::env().emit_event(PublicPhaseActivated {});
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
            years_to_register: u8,
            referrer: Option<String>,
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

            let (base_price, premium, discount, affiliate) =
                self.get_name_price(name.clone(), recipient, years_to_register, referrer)?;
            let price = base_price + premium - discount;

            /* Make sure the register is paid for */
            let _transferred = self.env().transferred_value();
            if _transferred < price {
                return Err(Error::FeeNotPaid);
            }

            let expiry_time = self.env().block_timestamp() + YEAR * years_to_register as u64;
            self.register_domain(&name, &recipient, expiry_time)?;

            // Pay the affiliate (if present) after successful registration
            if let Some(usr) = affiliate {
                if self.env().transfer(usr, discount).is_err() {
                    return Err(Error::WithdrawFailed);
                }
            }

            Ok(())
        }

        /// Register specific name with caller as owner.
        ///
        /// NOTE: Whitelisted addresses can buy one domain during the whitelist phase by submitting its proof
        #[ink(message, payable)]
        pub fn register(
            &mut self,
            name: String,
            years_to_register: u8,
            referrer: Option<String>,
            merkle_proof: Option<Vec<[u8; 32]>>,
            set_as_primary_domain: bool,
        ) -> Result<()> {
            self.register_on_behalf_of(
                name.clone(),
                self.env().caller(),
                years_to_register,
                referrer,
                merkle_proof,
            )?;
            if set_as_primary_domain {
                self.set_primary_domain(name)?;
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

            if Some(caller) != user {
                return Err(Error::NotAuthorised);
            }

            let expiry_time = self.env().block_timestamp() + YEAR;
            self.register_domain(&name, &caller, expiry_time)
                .and_then(|_| {
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
            self.ensure_owner(&caller, &name)?;

            self.remove_name(&name);

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
            self.ensure_owner(&caller, &name)?;

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

            /* Clear metadata */
            self.metadata.remove(&name);

            Self::env().emit_event(Transfer {
                name,
                from: caller,
                old_owner: Some(caller),
                new_owner: to,
            });

            Ok(())
        }

        /// Removes the associated state of expired-domains from storage
        #[ink(message)]
        pub fn clear_expired_names(&mut self, names: Vec<String>) -> Result<u128> {
            let mut count = 0;
            names.into_iter().for_each(|name| {
                // Verify the name has expired
                if self.has_name_expired(&name) == Ok(true) {
                    self.remove_name(&name);
                    count += 1;
                }
            });
            Ok(count)
        }

        /// Set primary domain of an address (reverse record)
        #[ink(message, payable)]
        pub fn set_primary_domain(&mut self, name: String) -> Result<()> {
            let address = self.env().caller();
            let resolved = self.get_address_dict_ref(&name)?.resolved;

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

            let mut address_dict = self.get_address_dict_ref(&name)?;
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
            self.ensure_controller(&caller, &name)?;

            let mut address_dict = self.get_address_dict_ref(&name)?;
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

            self.metadata.insert(&name, &records);

            self.ensure_metadata_under_limit(&name)
        }

        /// Sets one record
        #[ink(message)]
        pub fn set_record(&mut self, name: String, record: (String, String)) -> Result<()> {
            /* Ensure that the caller is a controller */
            let caller: AccountId = Self::env().caller();
            self.ensure_controller(&caller, &name)?;

            let metadata = self.metadata.get(&name).unwrap_or_default();
            let updated_metadata = self.update_metadata(metadata, &record.0, &record.1);
            self.metadata.insert(&name, &updated_metadata);

            self.ensure_metadata_under_limit(&name)
        }

        fn update_metadata(
            &self,
            metadata: Vec<(String, String)>,
            key: &str,
            value: &str,
        ) -> Vec<(String, String)> {
            let mut found = false;
            let mut updated_metadata: Vec<(String, String)> = metadata
                .into_iter()
                .map(|(k, v)| {
                    if k == key {
                        found = true;
                        (k, value.to_string())
                    } else {
                        (k, v)
                    }
                })
                .collect();

            if !found {
                updated_metadata.push((key.to_string(), value.to_string()));
            }
            updated_metadata
        }

        /// Returns the current status of the domain
        #[ink(message)]
        pub fn get_domain_status(&self, names: Vec<String>) -> Vec<DomainStatus> {
            let status = |name: String| {
                if let Ok(user) = self.get_address_dict_ref(&name) {
                    DomainStatus::Registered(user)
                } else if let Some(user) = self.reserved_names.get(&name) {
                    DomainStatus::Reserved(user)
                } else if self.is_name_allowed(&name) {
                    DomainStatus::Available
                } else {
                    DomainStatus::Unavailable
                }
            };

            names.into_iter().map(status).collect()
        }

        /// Get the addresses related to specific name
        #[ink(message)]
        pub fn get_address_dict(&self, name: String) -> Result<AddressDict> {
            self.get_address_dict_ref(&name)
        }

        /// Get owner of specific name.
        #[ink(message)]
        pub fn get_owner(&self, name: String) -> Result<AccountId> {
            self.get_address_dict_ref(&name).map(|x| x.owner)
        }

        /// Get controller of specific name.
        #[ink(message)]
        pub fn get_controller(&self, name: String) -> Result<AccountId> {
            self.get_address_dict_ref(&name).map(|x| x.controller)
        }

        /// Get address for specific name.
        #[ink(message)]
        pub fn get_address(&self, name: String) -> Result<AccountId> {
            self.get_address_dict_ref(&name).map(|x| x.resolved)
        }

        #[ink(message)]
        pub fn get_expiry_date(&self, name: String) -> Result<u64> {
            self.name_to_expiry.get(&name).ok_or(Error::NameDoesntExist)
        }

        /// Gets all records
        #[ink(message)]
        pub fn get_records(&self, name: String) -> Result<Vec<(String, String)>> {
            self.get_metadata_ref(&name)
        }

        /// Gets an arbitrary record by key
        #[ink(message)]
        pub fn get_record(&self, name: String, key: String) -> Result<String> {
            let info = self.get_metadata_ref(&name)?;
            match info.iter().find(|tuple| tuple.0 == key) {
                Some(val) => Ok(val.clone().1),
                None => Err(Error::RecordNotFound),
            }
        }

        /// Returns all names the address owns
        #[ink(message)]
        pub fn get_owned_names_of_address(&self, owner: AccountId) -> Option<Vec<String>> {
            self.owner_to_names.get(owner).map(|names| {
                names
                    .into_iter()
                    .filter(|name| self.has_name_expired(&name) == Ok(false))
                    .collect()
            })
        }

        #[ink(message)]
        pub fn get_controlled_names_of_address(
            &self,
            controller: AccountId,
        ) -> Option<Vec<String>> {
            self.controller_to_names.get(controller).map(|names| {
                names
                    .into_iter()
                    .filter(|name| self.has_name_expired(&name) == Ok(false))
                    .collect()
            })
        }

        #[ink(message)]
        pub fn get_resolving_names_of_address(&self, address: AccountId) -> Option<Vec<String>> {
            self.resolving_to_address.get(address).map(|names| {
                names
                    .into_iter()
                    .filter(|name| self.has_name_expired(&name) == Ok(false))
                    .collect()
            })
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
            if resolved_address != Ok(address) {
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

        #[ink(message)]
        pub fn get_metadata_size_limit(&self) -> Option<u32> {
            self.metadata_size_limit
        }

        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
        }

        /// Returns `true` when contract is in whitelist-phase
        /// and `false` when it is in public-phase
        #[ink(message)]
        pub fn is_whitelist_phase(&self) -> bool {
            self.whitelisted_address_verifier.get_or_default().is_some()
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

            let Some(verifier) = &self.whitelisted_address_verifier.get_or_default() else {
                return false;
            };
            verifier.verify_proof(leaf, merkle_proof)
        }

        /// (ADMIN-OPERATION)
        /// Transfers `value` amount of tokens to the caller.
        #[ink(message)]
        pub fn withdraw(&mut self, value: Balance) -> Result<()> {
            self.ensure_admin()?;

            assert!(value <= Self::env().balance(), "insufficient funds!");

            if Self::env().transfer(Self::env().caller(), value).is_err() {
                return Err(Error::WithdrawFailed);
            }

            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Switch from whitelist-phase to public-phase
        #[ink(message)]
        pub fn switch_to_public_phase(&mut self) -> Result<()> {
            self.ensure_admin()?;

            if self.whitelisted_address_verifier.get_or_default().is_some() {
                self.whitelisted_address_verifier.set(&None);
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
            self.ensure_admin()?;

            set.iter().for_each(|(name, addr)| {
                self.reserved_names.insert(&name, addr);
            });
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Remove given names from the list of reserved domains
        #[ink(message)]
        pub fn remove_reserved_domain(&mut self, set: Vec<String>) -> Result<()> {
            self.ensure_admin()?;

            set.iter().for_each(|name| self.reserved_names.remove(name));
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Update the limit of metadata allowed to store per name
        #[ink(message)]
        pub fn set_metadata_size_limit(&mut self, limit: Option<u32>) -> Result<()> {
            self.ensure_admin()?;
            self.metadata_size_limit = limit;
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Upgrade contract code
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

        #[ink(message)]
        pub fn set_admin(&mut self, account: AccountId) -> Result<()> {
            self.ensure_admin()?;
            self.admin = account;
            Ok(())
        }

        fn ensure_admin(&self) -> Result<()> {
            if self.admin != self.env().caller() {
                Err(Error::NotAdmin)
            } else {
                Ok(())
            }
        }

        fn ensure_owner(&self, address: &AccountId, name: &str) -> Result<()> {
            let AddressDict { owner, .. } = self.get_address_dict_ref(&name)?;
            if address != &owner {
                Err(Error::CallerIsNotOwner)
            } else {
                Ok(())
            }
        }

        fn ensure_controller(&self, address: &AccountId, name: &str) -> Result<()> {
            /* Ensure that the address has the right to control the target domain */
            let AddressDict {
                owner, controller, ..
            } = self.get_address_dict_ref(&name)?;

            if address != &controller && address != &owner {
                Err(Error::CallerIsNotController)
            } else {
                Ok(())
            }
        }

        fn ensure_metadata_under_limit(&self, name: &str) -> Result<()> {
            let size = self.metadata.size(name).unwrap_or(0);
            let limit = self.metadata_size_limit.unwrap_or(u32::MAX);

            match size <= limit {
                true => Ok(()),
                false => Err(Error::MetadataOverflow),
            }
        }

        fn register_domain(
            &mut self,
            name: &str,
            recipient: &AccountId,
            expiry: u64,
        ) -> Result<()> {
            match self.has_name_expired(&name) {
                Ok(false) => return Err(Error::NameAlreadyExists), // Domain is already registered
                Ok(true) => self.remove_name(&name), // Clean the expired domain state first
                _ => (),                             // Domain is available
            }

            let address_dict = AddressDict::new(recipient.clone());
            self.name_to_address_dict.insert(name, &address_dict);
            self.name_to_expiry.insert(name, &expiry);

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

        fn remove_name(&mut self, name: &str) {
            let Ok(address_dict) = self.get_address_dict_ref(&name) else {
                return;
            };

            self.name_to_address_dict.remove(name);
            self.name_to_expiry.remove(name);
            self.metadata.remove(name);

            self.remove_name_from_owner(&address_dict.owner, &name);
            self.remove_name_from_controller(&address_dict.controller, &name);
            self.remove_name_from_resolving(&address_dict.resolved, &name);
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
                if name_checker.is_name_allowed(name.to_string()) != Ok(()) {
                    return false;
                }
            }
            true
        }

        #[ink(message)]
        pub fn get_name_price(
            &self,
            name: String,
            recipient: AccountId,
            years_to_register: u8,
            referrer: Option<String>,
        ) -> Result<(Balance, Balance, Balance, Option<AccountId>)> {
            let (base_price, premium) = match &self.fee_calculator {
                None => (1000, 0), // For unit testing only
                Some(model) => model
                    .get_name_price(name.clone(), years_to_register)
                    .map_err(|e| Error::FeeError(e))?,
            };
            let price = base_price + premium;
            let mut discount = 0;
            let mut affiliate = None;

            // Only in public phase
            if !self.is_whitelist_phase() {
                if let Some(referrer_name) = referrer {
                    let address_dict = self.get_address_dict_ref(&referrer_name);
                    if let Ok(x) = address_dict {
                        if recipient != x.owner
                            && recipient != x.controller
                            && recipient != x.resolved
                        {
                            affiliate = Some(x.resolved);
                            discount = 5 * price / 100; // 5% discount
                        }
                    }
                }
            }

            Ok((base_price, premium, discount, affiliate))
        }

        fn get_address_dict_ref(&self, name: &str) -> Result<AddressDict> {
            self.name_to_address_dict
                .get(name)
                .filter(|_| self.has_name_expired(name) == Ok(false))
                .ok_or(Error::NameDoesntExist)
        }

        fn get_metadata_ref(&self, name: &str) -> Result<Vec<(String, String)>> {
            self.metadata
                .get(name)
                .filter(|_| self.has_name_expired(name) == Ok(false))
                .ok_or(Error::NoRecordsForAddress)
        }

        fn has_name_expired(&self, name: &str) -> Result<bool> {
            match self.name_to_expiry.get(name) {
                Some(expiry) => Ok(expiry <= self.env().block_timestamp()),
                None => Err(Error::NameDoesntExist),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::azns_registry::*;
    use crate::address_dict::AddressDict;
    use ink::codegen::Env;
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
        DomainNameService::new(
            default_accounts().alice,
            None,
            None,
            None,
            None,
            "azero".to_string(),
            None,
        )
    }

    #[ink::test]
    fn owner_to_names_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );

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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* Register bar under bob, but set controller to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_next_caller(default_accounts.charlie);
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* getting all domains should return first only */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name.clone()]
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* getting all domains should return first two */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![name.clone(), name2.clone()])
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name2, 1, None, None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name3, 1, None, None, false), Ok(()));

        /* Now alice owns three domains */
        /* Set the primary domain for alice's address to domain 1 */
        contract.set_primary_domain(name.clone()).unwrap();

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
        set_next_caller(default_accounts.bob);
        contract.set_primary_domain(name.clone()).unwrap();

        /* Now the primary domain should not resolve to anything */
        assert_eq!(contract.get_primary_domain(default_accounts.bob), Ok(name));
    }

    #[ink::test]
    fn register_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name, 1, None, None, false),
            Err(Error::NameAlreadyExists)
        );

        // Reserved names cannot be registered
        let reserved_name = String::from("AlephZero");
        let reserved_list = vec![(reserved_name.clone(), Some(default_accounts.alice))];
        contract
            .add_reserved_domains(reserved_list)
            .expect("Failed to reserve domain");

        assert_eq!(
            contract.register(reserved_name, 1, None, None, false),
            Err(Error::CannotBuyReservedDomain)
        );
    }

    #[ink::test]
    fn register_with_set_primary_domain_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name.clone(), 1, None, None, true), Ok(()));

        assert_eq!(
            contract.get_primary_domain(default_accounts.alice),
            Ok(name)
        );
    }

    #[ink::test]
    fn withdraw_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        // Alice deploys the contract
        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        // Bob registers
        let fees = 160_u128 * 10_u128.pow(12);
        set_next_caller(default_accounts.bob);
        set_account_balance::<DefaultEnvironment>(default_accounts.bob, fees);
        transfer_in::<DefaultEnvironment>(fees);
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));

        // Alice (admin) withdraws the funds
        set_next_caller(default_accounts.alice);

        let balance_before =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        assert_eq!(contract.withdraw(fees), Ok(()));
        let balance_after =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();

        assert_eq!(balance_after, balance_before + fees);
    }

    #[ink::test]
    fn withdraw_only_owner() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        let _acc_balance_before_transfer: Balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));

        set_next_caller(default_accounts.bob);
        assert_eq!(
            contract.withdraw(160_u128 * 10_u128.pow(12)),
            Err(Error::NotAdmin)
        );
    }

    #[ink::test]
    fn reverse_search_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("test2");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(contract.register(name2, 1, None, None, false), Ok(()));
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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name, 1, None, None, false),
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
    //     set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
    //     assert_eq!(contract.register(name, None), Err(NameNotAllowed, false));
    // }

    #[ink::test]
    fn register_with_fee_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.register(name, 1, None, None, false),
            Err(Error::NameAlreadyExists)
        );
    }

    #[ink::test]
    fn register_without_fee_reverts() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        assert_eq!(
            contract.register(name, 1, None, None, false),
            Err(Error::FeeNotPaid)
        );
    }

    #[ink::test]
    fn release_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.set_address(name.clone(), default_accounts.alice),
            Ok(())
        );
        assert_eq!(contract.get_owner(name.clone()), Ok(default_accounts.alice));
        assert_eq!(
            contract.get_address(name.clone()),
            Ok(default_accounts.alice)
        );

        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(Vec::from([name.clone()]))
        );

        assert_eq!(contract.release(name.clone()), Ok(()));
        assert_eq!(
            contract.get_owner(name.clone()),
            Err(Error::NameDoesntExist)
        );
        assert_eq!(
            contract.get_address(name.clone()),
            Err(Error::NameDoesntExist)
        );

        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Some(Vec::from([]))
        );
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            Some(vec![])
        );
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Some(vec![])
        );

        /* Another account can register again*/
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.set_address(name.clone(), default_accounts.bob),
            Ok(())
        );
        assert_eq!(contract.get_owner(name.clone()), Ok(default_accounts.bob));
        assert_eq!(contract.get_address(name.clone()), Ok(default_accounts.bob));
        assert_eq!(contract.release(name.clone()), Ok(()));
        assert_eq!(
            contract.get_owner(name.clone()),
            Err(Error::NameDoesntExist)
        );
        assert_eq!(contract.get_address(name), Err(Error::NameDoesntExist));
    }

    #[ink::test]
    fn controller_separation_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();

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
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        // Caller is controller, set_address will be successful
        set_next_caller(accounts.alice);
        assert_eq!(contract.set_address(name.clone(), accounts.bob), Ok(()));
        assert_eq!(contract.get_address(name), Ok(accounts.bob));
    }

    #[ink::test]
    fn transfer_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

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
            Err(Error::CallerIsNotController)
        );

        // Controller is bob, alice `set_address` should fail.
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        set_next_caller(accounts.bob);
        // Now owner is bob, `set_address` should be successful.
        assert_eq!(contract.set_address(name.clone(), accounts.eve), Ok(()));
        assert_eq!(contract.get_address(name), Ok(accounts.eve));
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

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        assert_eq!(
            contract.register(domain_name.clone(), 1, None, None, false),
            Ok(())
        );

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
            contract.get_records(domain_name).unwrap(),
            Vec::from([("twitter".to_string(), "@newtest".to_string())])
        );
    }

    #[ink::test]
    fn set_record_works() {
        let accounts = default_accounts();
        let key = String::from("twitter");
        let value = String::from("@test");

        let domain_name = "test".to_string();

        set_next_caller(accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(160_u128.pow(12));
        assert_eq!(
            contract.register(domain_name.clone(), 1, None, None, false),
            Ok(())
        );

        assert_eq!(
            contract.set_record(domain_name.clone(), (key.clone(), value.clone())),
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
            contract.set_record(domain_name.clone(), (key.clone(), value.clone())),
            Ok(())
        );
        assert_eq!(
            contract.get_record(domain_name.clone(), key).unwrap(),
            value
        );

        /* Confirm overwriting */
        assert_eq!(
            contract.set_record(
                domain_name.clone(),
                ("twitter".to_string(), "@newtest".to_string()),
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_records(domain_name).unwrap(),
            Vec::from([("twitter".to_string(), "@newtest".to_string())])
        );
    }

    #[ink::test]
    fn metadata_limit_works() {
        let mut contract = get_test_name_service();
        let name = "alice".to_string();
        let records = vec![
            ("@twitter".to_string(), "alice_musk".to_string()),
            ("@facebook".to_string(), "alice_zuk".to_string()),
        ];

        contract.set_metadata_size_limit(Some(40)).unwrap();
        assert_eq!(contract.get_metadata_size_limit(), Some(40));

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();

        // With current input, both records cannot be stored simultaneously
        assert_eq!(
            contract.set_all_records(name.clone(), records.clone()),
            Err(Error::MetadataOverflow)
        );

        // Storing only one works
        assert_eq!(
            contract.set_all_records(name.clone(), records[0..1].to_vec()),
            Ok(())
        );

        // Adding the second record fails
        assert_eq!(
            contract.set_record(name.clone(), records[1].clone()),
            Err(Error::MetadataOverflow),
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
            contract.get_domain_status(vec![reserved_name]),
            vec![DomainStatus::Reserved(Some(accounts.alice))],
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(contract.add_reserved_domains(vec![]), Err(Error::NotAdmin));
    }

    #[ink::test]
    fn remove_reserved_domains_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let reserved_name = String::from("AlephZero");
        let list = vec![(reserved_name.clone(), Some(accounts.alice))];
        assert!(contract.add_reserved_domains(list).is_ok());

        assert_eq!(
            contract.get_domain_status(vec![reserved_name.clone()]),
            vec![DomainStatus::Reserved(Some(accounts.alice))],
        );

        assert!(contract
            .remove_reserved_domain(vec![reserved_name.clone()])
            .is_ok());

        assert_ne!(
            contract.get_domain_status(vec![reserved_name]),
            vec![DomainStatus::Reserved(Some(accounts.alice))],
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.remove_reserved_domain(vec![]),
            Err(Error::NotAdmin)
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

        let address_dict = AddressDict::new(accounts.bob);
        assert_eq!(
            contract.get_domain_status(vec![name]),
            vec![DomainStatus::Registered(address_dict)],
        );
    }

    #[ink::test]
    fn get_domain_status_works() {
        let accounts = default_accounts();
        let reserved_list = vec![("bob".to_string(), Some(accounts.bob))];

        let mut contract = DomainNameService::new(
            default_accounts().alice,
            None,
            None,
            None,
            Some(reserved_list),
            "azero".to_string(),
            None,
        );

        set_value_transferred::<DefaultEnvironment>(160_u128 * 10_u128.pow(12));
        contract
            .register("alice".to_string(), 1, None, None, false)
            .expect("failed to register domain");

        let address_dict = AddressDict::new(accounts.alice);
        assert_eq!(
            contract.get_domain_status(vec!["alice".to_string()]),
            vec![DomainStatus::Registered(address_dict)]
        );

        assert_eq!(
            contract.get_domain_status(vec!["bob".to_string()]),
            vec![DomainStatus::Reserved(Some(accounts.bob))]
        );

        assert_eq!(
            contract.get_domain_status(vec!["david".to_string()]),
            vec![DomainStatus::Available]
        );

        assert_eq!(
            contract.get_domain_status(vec!["".to_string()]),
            vec![DomainStatus::Unavailable]
        );
    }

    #[ink::test]
    fn referral_system_works() {
        let default_accounts = default_accounts();
        let mut contract = get_test_name_service();

        set_callee::<DefaultEnvironment>(default_accounts.eve);
        assert_eq!(contract.env().account_id(), default_accounts.eve);

        let alice = "alice".to_string();
        let bob = "bob".to_string();

        // 1. Invalid referrer name gives no discount
        let fees = 1000;
        set_next_caller(default_accounts.alice);
        set_account_balance::<DefaultEnvironment>(default_accounts.alice, fees);
        set_callee::<DefaultEnvironment>(contract.env().account_id());
        transfer_in::<DefaultEnvironment>(fees);
        assert_eq!(
            contract.register(alice.clone(), 1, Some(bob.clone()), None, false),
            Ok(())
        );

        let alice_balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();

        // Initial Balance(alice): 1000
        // Domain fee without discount: 1000
        assert_eq!(alice_balance, 0);

        // 2. Discount works
        let discount = 50;
        set_next_caller(default_accounts.bob);
        set_account_balance::<DefaultEnvironment>(default_accounts.bob, fees);
        transfer_in::<DefaultEnvironment>(fees - discount);
        assert_eq!(contract.register(bob, 1, Some(alice), None, false), Ok(()));

        let alice_balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        let bob_balance = get_account_balance::<DefaultEnvironment>(default_accounts.bob).unwrap();

        // Initial Balance (bob): 1000
        // Domain fee after discount: 9950
        assert_eq!(bob_balance, 50);

        // Affiliation payment to alice
        assert_eq!(alice_balance, 50);
    }

    #[ink::test]
    fn self_referral_not_allowed() {
        let default_accounts = default_accounts();
        let mut contract = get_test_name_service();

        set_callee::<DefaultEnvironment>(default_accounts.eve);
        assert_eq!(contract.env().account_id(), default_accounts.eve);

        let alice = "alice".to_string();
        let wonderland = "wonderland".to_string();

        // 1. Register first name without referrer
        let fees = 1000;
        set_next_caller(default_accounts.alice);
        set_account_balance::<DefaultEnvironment>(default_accounts.alice, fees);
        set_callee::<DefaultEnvironment>(contract.env().account_id());
        transfer_in::<DefaultEnvironment>(fees);
        assert_eq!(
            contract.register(alice.clone(), 1, None, None, false),
            Ok(())
        );

        // 2. Self-referral doesn't work
        set_account_balance::<DefaultEnvironment>(default_accounts.alice, fees);
        transfer_in::<DefaultEnvironment>(fees);
        assert_eq!(
            contract.register(wonderland, 1, Some(alice), None, false),
            Ok(())
        );

        let alice_balance =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();

        // No bonus received by alice
        assert_eq!(alice_balance, 0);
    }

    #[ink::test]
    fn name_expiry_works() {
        let mut contract = get_test_name_service();

        let name1 = "one-year".to_string();
        let name2 = "two-year".to_string();

        // Register name1 for one year
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name1.clone(), 1, None, None, true)
            .unwrap();

        // Register name2 for two years
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name2.clone(), 2, None, None, false)
            .unwrap();

        // (for cfg(test)) block_time = 6, year = 60
        for _ in 0..10 {
            advance_block::<DefaultEnvironment>();
        }

        let address_dict = AddressDict::new(default_accounts().alice);
        assert_eq!(
            contract.get_domain_status(vec![name1.clone(), name2.clone()]),
            vec![
                DomainStatus::Available,
                DomainStatus::Registered(address_dict)
            ]
        );

        assert_eq!(
            contract.get_primary_domain(default_accounts().alice),
            Err(Error::NoResolvedAddress)
        );

        assert_eq!(
            contract.get_records(name1.clone()),
            Err(Error::NoRecordsForAddress)
        );

        // Reverse mapping implicitly excludes expired names
        assert_eq!(
            contract.get_names_of_address(default_accounts().alice),
            vec![name2.clone()]
        );
    }

    #[ink::test]
    fn clear_expired_names_works() {
        let mut contract = get_test_name_service();

        let name1 = "one-year".to_string();
        let name2 = "two-year".to_string();

        // Register name1 for one year
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name1.clone(), 1, None, None, true)
            .unwrap();

        // Register name2 for two years
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name2.clone(), 2, None, None, false)
            .unwrap();

        // (for cfg(test)) block_time = 6, year = 60
        for _ in 0..10 {
            advance_block::<DefaultEnvironment>();
        }

        // Only the expired names are cleared
        assert_eq!(
            contract.clear_expired_names(vec![name1.clone(), name2.clone()]),
            Ok(1)
        );

        let address_dict = AddressDict::new(default_accounts().alice);
        assert_eq!(
            contract.get_domain_status(vec![name1.clone(), name2.clone()]),
            vec![
                DomainStatus::Available,
                DomainStatus::Registered(address_dict)
            ]
        );
    }

    #[ink::test]
    fn register_expired_names_works() {
        let mut contract = get_test_name_service();

        let name1 = "one-year".to_string();
        let name2 = "two-year".to_string();

        // Register name1 for one year
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name1.clone(), 1, None, None, true)
            .unwrap();

        // Register name2 for two years
        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name2.clone(), 2, None, None, false)
            .unwrap();

        // Registering an active name causes error
        set_next_caller(default_accounts().bob);
        assert_eq!(
            contract.register(name1.clone(), 1, None, None, false),
            Err(Error::NameAlreadyExists)
        );

        // (for cfg(test)) block_time = 6, year = 60
        for _ in 0..10 {
            advance_block::<DefaultEnvironment>();
        }

        // Registering an expired name works
        assert_eq!(
            contract.register(name1.clone(), 1, None, None, false),
            Ok(())
        );
    }

    #[ink::test]
    fn set_admin_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        assert_eq!(contract.get_admin(), accounts.alice);
        assert_eq!(contract.set_admin(accounts.bob), Ok(()));
        assert_eq!(contract.get_admin(), accounts.bob);

        // Now alice (not admin anymore) cannot update admin
        assert_eq!(contract.set_admin(accounts.alice), Err(Error::NotAdmin));
    }

    // TODO Need cross-contract test support
    // #[ink::test]
    // fn referral_system_inactive_during_whitelist_phase() {

    // }

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
