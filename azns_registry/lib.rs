#![cfg_attr(not(feature = "std"), no_std)]

mod address_dict;

#[util_macros::azns_contract(Ownable2Step[
    Error = Error::NotAdmin
])]
#[util_macros::azns_contract(Upgradable)]
#[ink::contract]
mod azns_registry {
    use crate::address_dict::AddressDict;
    use ink::env::call::FromAccountId;
    use ink::env::hash::CryptoHash;
    use ink::prelude::string::{String, ToString};
    use ink::prelude::vec::Vec;
    use ink::storage::traits::ManualKey;
    use ink::storage::{Lazy, Mapping};
    use interfaces::art_zero_traits::*;
    use interfaces::psp34_standard::*;

    use azns_fee_calculator::FeeCalculatorRef;
    use azns_merkle_verifier::MerkleVerifierRef;
    use azns_name_checker::NameCheckerRef;

    const YEAR: u64 = match cfg!(test) {
        true => 60,                         // For testing purpose
        false => 365 * 24 * 60 * 60 * 1000, // Year in milliseconds
    };

    pub type Result<T> = core::result::Result<T, Error>;

    /// Different states of a name
    #[derive(scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo, Debug, PartialEq))]
    pub enum NameStatus {
        /// Name is registered with the given AddressDict
        Registered(AddressDict),
        /// Name is reserved for the given address
        Reserved(Option<AccountId>),
        /// Name is available for purchase
        Available,
        /// Name has invalid characters/length
        Unavailable,
    }

    /// Emitted whenever a new name is registered.
    #[ink(event)]
    pub struct Register {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: AccountId,
        registration_timestamp: u64,
        expiration_timestamp: u64,
    }

    #[ink(event)]
    pub struct FeeReceived {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        from: AccountId,
        #[ink(topic)]
        referrer: Option<String>,
        referrer_addr: Option<AccountId>,
        received_fee: Balance,
        forwarded_referrer_fee: Balance,
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

    /// Emitted whenever controller changes.
    #[ink(event)]
    pub struct SetController {
        #[ink(topic)]
        name: String,
        from: AccountId,
        #[ink(topic)]
        old_controller: Option<AccountId>,
        #[ink(topic)]
        new_controller: AccountId,
    }

    #[ink(event)]
    pub struct SetPrimaryName {
        #[ink(topic)]
        account: AccountId,
        #[ink(topic)]
        primary_name: Option<String>,
    }

    #[ink(event)]
    pub struct RecordsUpdated {
        #[ink(topic)]
        name: String,
        from: AccountId,
    }

    /// Event emitted when a token transfer occurs.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        #[ink(topic)]
        id: Id,
    }

    /// Event emitted when a token approve occurs.
    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        operator: AccountId,
        #[ink(topic)]
        id: Option<Id>,
        approved: bool,
    }

    /// Emitted when switching from whitelist-phase to public-phase
    #[ink(event)]
    pub struct PublicPhaseActivated;

    /// Emitted when a name is reserved or removed from the reservation list
    #[ink(event)]
    pub struct Reserve {
        #[ink(topic)]
        name: String,
        #[ink(topic)]
        account_id: Option<AccountId>,
        action: bool,
    }

    #[ink(storage)]
    pub struct Registry {
        /// Admin of the contract can perform root operations
        admin: AccountId,
        /// Two-step ownership transfer AccountId
        pending_admin: Option<AccountId>,
        /// TLD
        tld: String,
        /// Base URI
        base_uri: String,
        /// Total supply (including expired names)
        total_supply: Balance,
        /// Maximum record (in bytes) a name can be associated with
        records_size_limit: Option<u32>,

        /// Contract which verifies the validity of a name
        name_checker: Option<NameCheckerRef>,
        /// Contract which calculates the name price
        fee_calculator: Option<FeeCalculatorRef>,

        /// Names which can be claimed only by the specified user
        reserved_names: Mapping<String, Option<AccountId>, ManualKey<100>>,
        /// Mapping from owner to operator approvals.
        operator_approvals: Mapping<(AccountId, AccountId, Option<Id>), (), ManualKey<101>>,

        /// Mapping from name to addresses associated with it
        name_to_address_dict: Mapping<String, AddressDict, ManualKey<200>>,
        /// Mapping from name to its registration period (registration_timestamp, expiration_timestamp)
        name_to_period: Mapping<String, (u64, u64), ManualKey<202>>,
        /// Records
        records: Mapping<String, Vec<(String, String)>, ManualKey<201>>,

        /// All names an address owns
        owner_to_name_count: Mapping<AccountId, u128, ManualKey<300>>,
        owner_to_names: Mapping<(AccountId, u128), String, ManualKey<301>>,
        name_to_owner_index: Mapping<String, u128, ManualKey<302>>,

        /// All names an address controls
        controller_to_name_count: Mapping<AccountId, u128, ManualKey<310>>,
        controller_to_names: Mapping<(AccountId, u128), String, ManualKey<311>>,
        name_to_controller_index: Mapping<String, u128, ManualKey<312>>,

        /// All names that resolve to the given address
        resolving_to_name_count: Mapping<AccountId, u128, ManualKey<320>>,
        resolving_to_names: Mapping<(AccountId, u128), String, ManualKey<321>>,
        name_to_resolving_index: Mapping<String, u128, ManualKey<323>>,

        /// Primary name record
        /// IMPORTANT NOTE: This mapping may be out-of-date, since we don't update it when a resolved address changes, or when a name is withdrawn.
        /// Only use the get_primary_name
        address_to_primary_name: Mapping<AccountId, String, ManualKey<399>>,

        /// Merkle Verifier used to identifiy whitelisted addresses
        whitelisted_address_verifier: Lazy<Option<MerkleVerifierRef>, ManualKey<999>>,
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
        /// This call requires the caller to be a controller of the name
        CallerIsNotController,
        /// Returned if caller did not send a required fee
        FeeNotPaid,
        /// Returned if name is empty
        NameEmpty,
        /// Record with the key doesn't exist
        RecordNotFound,
        /// Withdraw failed
        WithdrawFailed,
        /// Insufficient balance in the contract
        InsufficientBalance,
        /// No resolved address found
        NoResolvedAddress,
        /// A user can claim only one name during the whitelist-phase
        AlreadyClaimed,
        /// The merkle proof is invalid
        InvalidMerkleProof,
        /// The given name is reserved and cannot to be bought
        CannotBuyReservedName,
        /// Cannot claim a non-reserved name. Consider buying it.
        NotReservedName,
        /// User is not authorised to perform the said task
        NotAuthorised,
        /// Zero address is not allowed
        ZeroAddress,
        /// Records size limit exceeded
        RecordsOverflow,
        /// Thrown when fee_calculator doesn't return a names' price
        FeeError(azns_fee_calculator::Error),
        /// Given operation can only be performed during the whitelist-phase
        OnlyDuringWhitelistPhase,
        /// Given operation cannot be performed during the whitelist-phase
        RestrictedDuringWhitelistPhase,
    }

    impl Registry {
        /// Creates a new AZNS contract.
        #[ink(constructor)]
        pub fn new(
            admin: AccountId,
            name_checker_addr: Option<AccountId>,
            fee_calculator_addr: Option<AccountId>,
            merkle_verifier_addr: Option<AccountId>,
            tld: String,
            base_uri: String,
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
                pending_admin: None,
                name_checker,
                fee_calculator,
                name_to_address_dict: Mapping::default(),
                name_to_period: Mapping::default(),
                owner_to_name_count: Default::default(),
                owner_to_names: Default::default(),
                name_to_owner_index: Default::default(),
                records: Default::default(),
                address_to_primary_name: Default::default(),
                controller_to_name_count: Default::default(),
                controller_to_names: Default::default(),
                name_to_controller_index: Default::default(),
                resolving_to_name_count: Default::default(),
                resolving_to_names: Default::default(),
                name_to_resolving_index: Default::default(),
                whitelisted_address_verifier: Default::default(),
                reserved_names: Default::default(),
                operator_approvals: Default::default(),
                tld,
                base_uri,
                records_size_limit: None,
                total_supply: 0,
            };

            // Initialize address verifier
            contract
                .whitelisted_address_verifier
                .set(&whitelisted_address_verifier);

            // No Whitelist phase
            if merkle_verifier_addr == None {
                Self::env().emit_event(PublicPhaseActivated {});
            }

            contract
        }

        /// Register specific name on behalf of some other address.
        /// Pay the fee, but forward the ownership of the name to the provided recipient
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

            // The name must not be a reserved name
            if self.reserved_names.contains(&name) {
                return Err(Error::CannotBuyReservedName);
            }

            // If in whitelist-phase; Verify that the caller is whitelisted
            if self.is_whitelist_phase() {
                let caller = self.env().caller();

                // Recipient must be the same as caller incase of whitelist-phase
                if recipient != caller {
                    return Err(Error::RestrictedDuringWhitelistPhase);
                }

                // Verify this is the first claim of the user
                if self.owner_to_name_count.contains(caller) {
                    return Err(Error::AlreadyClaimed);
                }

                // Verify the proof
                if !self.verify_proof(caller, merkle_proof) {
                    return Err(Error::InvalidMerkleProof);
                }
            }

            let (base_price, premium, discount, referrer_addr) =
                self.get_name_price(name.clone(), recipient, years_to_register, referrer.clone())?;
            let price = base_price + premium - discount;

            /* Make sure the register is paid for */
            let transferred = self.env().transferred_value();
            if transferred < price {
                return Err(Error::FeeNotPaid);
            } else if transferred > price {
                let caller = self.env().caller();
                let change = transferred - price;

                if self.env().transfer(caller, change).is_err() {
                    return Err(Error::WithdrawFailed);
                }
            }

            let expiry_time = self.env().block_timestamp() + YEAR * years_to_register as u64;
            self.register_name(&name, &recipient, expiry_time)?;

            // Pay the referrer_addr (if present) after successful registration
            if let Some(usr) = referrer_addr {
                if self.env().transfer(usr, discount).is_err() {
                    return Err(Error::WithdrawFailed);
                }
            }

            self.env().emit_event(FeeReceived {
                name,
                from: self.env().caller(),
                referrer,
                referrer_addr,
                received_fee: price - discount,
                forwarded_referrer_fee: discount,
            });

            Ok(())
        }

        /// Register specific name with caller as owner.
        ///
        /// NOTE: Whitelisted addresses can buy one name during the whitelist phase by submitting its proof
        #[ink(message, payable)]
        pub fn register(
            &mut self,
            name: String,
            years_to_register: u8,
            referrer: Option<String>,
            merkle_proof: Option<Vec<[u8; 32]>>,
            set_as_primary_name: bool,
        ) -> Result<()> {
            self.register_on_behalf_of(
                name.clone(),
                self.env().caller(),
                years_to_register,
                referrer,
                merkle_proof,
            )?;
            if set_as_primary_name {
                self.set_primary_name(Some(name))?;
            }
            Ok(())
        }

        /// Allows users to claim their reserved name at zero cost
        #[ink(message)]
        pub fn claim_reserved_name(&mut self, name: String) -> Result<()> {
            let caller = self.env().caller();

            let Some(user) = self.reserved_names.get(&name) else {
                return Err(Error::NotReservedName);
            };

            if Some(caller) != user {
                return Err(Error::NotAuthorised);
            }

            let expiry_time = self.env().block_timestamp() + YEAR;
            self.register_name(&name, &caller, expiry_time)
                .and_then(|_| {
                    // Remove the name from the list once claimed
                    self.reserved_names.remove(&name);
                    self.env().emit_event(Reserve {
                        name,
                        account_id: Some(caller),
                        action: false,
                    });
                    Ok(())
                })
        }

        /// Release name from registration.
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
        pub fn transfer(
            &mut self,
            to: AccountId,
            name: String,
            keep_records: bool,
            keep_controller: bool,
            keep_resolving: bool,
            data: Vec<u8>,
        ) -> core::result::Result<(), PSP34Error> {
            self.transfer_name(
                to,
                &name,
                keep_records,
                keep_controller,
                keep_resolving,
                &data,
            )
        }

        /// Removes the associated state of expired-names from storage
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

        /// Set primary name of an address (reverse record)
        /// @note if name is set to None then the primary-name for the caller will be removed (if exists)
        #[ink(message)]
        pub fn set_primary_name(&mut self, primary_name: Option<String>) -> Result<()> {
            let address = self.env().caller();

            match &primary_name {
                Some(name) => {
                    let resolved = self.get_address_dict_ref(&name)?.resolved;

                    /* Ensure the target name resolves to the address */
                    if resolved != address {
                        return Err(Error::NoResolvedAddress);
                    }
                    self.address_to_primary_name.insert(address, name);
                }
                None => self.address_to_primary_name.remove(address),
            };

            self.env().emit_event(SetPrimaryName {
                account: address,
                primary_name,
            });
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
            let old_controller = address_dict.controller;
            address_dict.set_controller(new_controller);
            self.name_to_address_dict.insert(&name, &address_dict);

            /* Remove the name from the old controller */
            self.remove_name_from_controller(&caller, &name);

            /* Add the name to the new controller */
            self.add_name_to_controller(&new_controller, &name);

            self.env().emit_event(SetController {
                name,
                from: caller,
                old_controller: Some(old_controller),
                new_controller,
            });

            Ok(())
        }

        #[ink(message)]
        pub fn reset_resolved_address(&mut self, names: Vec<String>) -> Result<()> {
            let caller = self.env().caller();

            for name in names.iter() {
                let mut address_dict = self.get_address_dict_ref(&name)?;
                let owner = address_dict.owner;
                let resolved = address_dict.resolved;

                if resolved != caller {
                    return Err(Error::NotAuthorised);
                }
                if resolved != owner {
                    address_dict.set_resolved(owner);

                    /* Remove the name from the old resolved address */
                    self.remove_name_from_resolving(&resolved, &name);

                    /* Add the name to the new resolved address */
                    self.add_name_to_resolving(&owner, &name);

                    self.env().emit_event(SetAddress {
                        name: name.to_string(),
                        from: caller,
                        old_address: Some(resolved),
                        new_address: owner,
                    });
                }
            }
            Ok(())
        }

        #[ink(message)]
        pub fn reset_controller(&mut self, names: Vec<String>) -> Result<()> {
            let caller = self.env().caller();

            for name in names.iter() {
                let mut address_dict = self.get_address_dict_ref(&name)?;
                let owner = address_dict.owner;
                let controller = address_dict.controller;

                if controller != caller {
                    return Err(Error::NotAuthorised);
                }
                if controller != owner {
                    address_dict.set_controller(owner);

                    /* Remove the name from the old controller address */
                    self.remove_name_from_controller(&controller, &name);

                    /* Add the name to the new controller address */
                    self.add_name_to_controller(&owner, &name);

                    self.env().emit_event(SetController {
                        name: name.to_string(),
                        from: caller,
                        old_controller: Some(controller),
                        new_controller: owner,
                    });
                }
            }
            Ok(())
        }

        #[ink(message)]
        pub fn update_records(
            &mut self,
            name: String,
            records: Vec<(String, Option<String>)>,
            remove_rest: bool,
        ) -> Result<()> {
            let caller: AccountId = Self::env().caller();
            self.ensure_controller(&caller, &name)?;

            use ink::prelude::collections::BTreeMap;

            let mut data = BTreeMap::new();

            if !remove_rest {
                self.get_records_ref(&name)
                    .into_iter()
                    .for_each(|(key, val)| {
                        data.insert(key, val);
                    });
            }

            records.into_iter().for_each(|(key, val)| {
                match val {
                    Some(v) => data.insert(key, v),
                    None => data.remove(&key),
                };
            });

            let updated_records: Vec<(String, String)> = data.into_iter().collect();
            self.records.insert(&name, &updated_records);

            self.ensure_records_under_limit(&name)?;

            self.env().emit_event(RecordsUpdated { name, from: caller });
            Ok(())
        }

        /// Returns the current status of the name
        #[ink(message)]
        pub fn get_name_status(&self, names: Vec<String>) -> Vec<NameStatus> {
            let status = |name: String| {
                if let Ok(user) = self.get_address_dict_ref(&name) {
                    NameStatus::Registered(user)
                } else if let Some(user) = self.reserved_names.get(&name) {
                    NameStatus::Reserved(user)
                } else if self.is_name_allowed(&name) {
                    NameStatus::Available
                } else {
                    NameStatus::Unavailable
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
        #[ink(message, selector = 0xd259f7ba)]
        pub fn get_address(&self, name: String) -> Result<AccountId> {
            self.get_address_dict_ref(&name).map(|x| x.resolved)
        }

        #[ink(message)]
        pub fn get_registration_period(&self, name: String) -> Result<(u64, u64)> {
            self.get_registration_period_ref(&name)
        }

        /// Gets all records
        #[ink(message)]
        pub fn get_all_records(&self, name: String) -> Vec<(String, String)> {
            self.get_records_ref(&name)
        }

        /// Gets an arbitrary record by key
        #[ink(message)]
        pub fn get_record(&self, name: String, key: String) -> Result<String> {
            let info = self.get_records_ref(&name);
            match info.iter().find(|tuple| tuple.0 == key) {
                Some(val) => Ok(val.clone().1),
                None => Err(Error::RecordNotFound),
            }
        }

        /// Returns all names the address owns
        #[ink(message)]
        pub fn get_owned_names_of_address(&self, owner: AccountId) -> Vec<String> {
            let count = self.get_owner_to_name_count(owner);

            (0..count)
                .filter_map(|idx| {
                    let name = self.owner_to_names.get((owner, idx)).expect("Infallible");
                    match self.has_name_expired(&name) {
                        Ok(false) => Some(name),
                        _ => None,
                    }
                })
                .collect()
        }

        #[ink(message)]
        pub fn get_controlled_names_of_address(&self, controller: AccountId) -> Vec<String> {
            let count = self.get_controller_to_name_count(controller);

            (0..count)
                .filter_map(|idx| {
                    let name = self
                        .controller_to_names
                        .get((controller, idx))
                        .expect("Infallible");
                    match self.has_name_expired(&name) {
                        Ok(false) => Some(name),
                        _ => None,
                    }
                })
                .collect()
        }

        #[ink(message)]
        pub fn get_resolving_names_of_address(&self, address: AccountId) -> Vec<String> {
            let count = self.get_resolving_to_name_count(address);

            (0..count)
                .filter_map(|idx| {
                    let name = self
                        .resolving_to_names
                        .get((address, idx))
                        .expect("Infallible");
                    match self.has_name_expired(&name) {
                        Ok(false) => Some(name),
                        _ => None,
                    }
                })
                .collect()
        }

        #[ink(message)]
        pub fn get_primary_name(&self, address: AccountId) -> Result<String> {
            /* Get the naive primary name of the address */
            let Some(primary_name) = self.address_to_primary_name.get(address) else {
                /* No primary name set */
                return Err(Error::NoResolvedAddress);
            };

            /* Check that the primary name actually resolves to the claimed address */
            let resolved_address = self.get_address(primary_name.clone());
            if resolved_address != Ok(address) {
                /* Resolved address is no longer valid */
                return Err(Error::NoResolvedAddress);
            }

            Ok(primary_name)
        }

        #[ink(message)]
        pub fn get_primary_domain(&self, address: AccountId) -> Option<String> {
            self.get_primary_name(address)
                .map(|name| name + "." + &self.tld)
                .ok()
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
                    .flatten()
                    .collect();

            set.into_iter().collect()
        }

        // @note count includes expired names as well
        #[ink(message)]
        pub fn get_owner_to_name_count(&self, user: AccountId) -> u128 {
            self.owner_to_name_count.get(user).unwrap_or(0)
        }

        // @note count includes expired names as well
        #[ink(message)]
        pub fn get_controller_to_name_count(&self, user: AccountId) -> u128 {
            self.controller_to_name_count.get(user).unwrap_or(0)
        }

        // @note count includes expired names as well
        #[ink(message)]
        pub fn get_resolving_to_name_count(&self, user: AccountId) -> u128 {
            self.resolving_to_name_count.get(user).unwrap_or(0)
        }

        #[ink(message)]
        pub fn get_records_size_limit(&self) -> Option<u32> {
            self.records_size_limit
        }

        #[ink(message)]
        pub fn get_tld(&self) -> String {
            self.tld.clone()
        }

        #[ink(message)]
        pub fn get_base_uri(&self) -> String {
            self.base_uri.clone()
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
        pub fn withdraw(
            &mut self,
            beneficiary: Option<AccountId>,
            value: Option<Balance>,
        ) -> Result<()> {
            self.ensure_admin()?;

            let beneficiary = beneficiary.unwrap_or(self.env().caller());
            let value = value.unwrap_or(self.env().balance());

            if beneficiary == [0u8; 32].into() {
                return Err(Error::ZeroAddress);
            }

            if value > self.env().balance() {
                return Err(Error::InsufficientBalance);
            }
            if self.env().transfer(beneficiary, value).is_err() {
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
        /// Reserve name name for specific addresses
        // @dev (name, None) denotes that the name is reserved but not tied to any address yet
        #[ink(message)]
        pub fn add_reserved_names(&mut self, set: Vec<(String, Option<AccountId>)>) -> Result<()> {
            self.ensure_admin()?;

            for (name, addr) in set.iter() {
                if name.is_empty() {
                    return Err(Error::NameEmpty);
                }
                if self.has_name_expired(name) == Ok(false) {
                    return Err(Error::NameAlreadyExists);
                }

                self.reserved_names.insert(&name, addr);
                self.env().emit_event(Reserve {
                    name: name.clone(),
                    account_id: *addr,
                    action: true,
                });
            }
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Remove given names from the list of reserved names
        #[ink(message)]
        pub fn remove_reserved_name(&mut self, set: Vec<String>) -> Result<()> {
            self.ensure_admin()?;

            set.iter().for_each(|name| {
                if self.reserved_names.contains(name) {
                    self.reserved_names.remove(name);
                    self.env().emit_event(Reserve {
                        name: name.clone(),
                        account_id: None,
                        action: false,
                    });
                }
            });
            Ok(())
        }

        /// (ADMIN-OPERATION)
        /// Update the limit of records allowed to store per name
        #[ink(message)]
        pub fn set_records_size_limit(&mut self, limit: Option<u32>) -> Result<()> {
            self.ensure_admin()?;
            self.records_size_limit = limit;
            Ok(())
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
            /* Ensure that the address has the right to control the target name */
            let AddressDict {
                owner, controller, ..
            } = self.get_address_dict_ref(&name)?;

            if address != &controller && address != &owner {
                Err(Error::CallerIsNotController)
            } else {
                Ok(())
            }
        }

        fn ensure_records_under_limit(&self, name: &str) -> Result<()> {
            let size = self.records.size(name).unwrap_or(0);
            let limit = self.records_size_limit.unwrap_or(u32::MAX);

            match size <= limit {
                true => Ok(()),
                false => Err(Error::RecordsOverflow),
            }
        }

        fn register_name(&mut self, name: &str, recipient: &AccountId, expiry: u64) -> Result<()> {
            match self.has_name_expired(&name) {
                Ok(false) => return Err(Error::NameAlreadyExists), // Name is already registered
                Ok(true) => self.remove_name(&name), // Clean the expired name state first
                _ => (),                             // Name is available
            }

            if recipient == &[0u8; 32].into() {
                return Err(Error::ZeroAddress);
            }

            let registration = self.env().block_timestamp();

            let address_dict = AddressDict::new(recipient.clone());
            self.name_to_address_dict.insert(name, &address_dict);
            self.name_to_period.insert(name, &(registration, expiry));

            /* Update convenience mapping for owned names */
            self.add_name_to_owner(recipient, name);

            /* Update convenience mapping for controlled names */
            self.add_name_to_controller(recipient, name);

            /* Update convenience mapping for resolved names */
            self.add_name_to_resolving(recipient, name);

            self.total_supply += 1;

            /* Emit register event */
            Self::env().emit_event(Register {
                name: name.to_string(),
                from: *recipient,
                registration_timestamp: registration,
                expiration_timestamp: expiry,
            });

            self.env().emit_event(Transfer {
                from: None,
                to: Some(*recipient),
                id: name.to_string().into(),
            });

            Ok(())
        }

        fn remove_name(&mut self, name: &str) {
            let Ok(address_dict) = self.get_address_dict_ref(&name) else {
                return;
            };

            self.name_to_address_dict.remove(name);
            self.name_to_period.remove(name);
            self.records.remove(name);

            self.remove_name_from_owner(&address_dict.owner, &name);
            self.remove_name_from_controller(&address_dict.controller, &name);
            self.remove_name_from_resolving(&address_dict.resolved, &name);

            self.total_supply -= 1;

            self.env().emit_event(Transfer {
                from: Some(address_dict.owner),
                to: None,
                id: name.to_string().into(),
            });
        }

        fn transfer_name(
            &mut self,
            to: AccountId,
            name: &str,
            keep_records: bool,
            keep_controller: bool,
            keep_resolving: bool,
            data: &Vec<u8>,
        ) -> core::result::Result<(), PSP34Error> {
            // Transfer is disabled during the whitelist-phase
            if self.is_whitelist_phase() {
                return Err(PSP34Error::Custom(
                    "transfer disabled during whitelist phase".to_string(),
                ));
            }

            if to == [0u8; 32].into() {
                return Err(PSP34Error::Custom("Zero address".to_string()));
            }

            let id: Id = name.to_string().into();
            let mut address_dict = self
                .get_address_dict_ref(&name)
                .map_err(|_| PSP34Error::TokenNotExists)?;

            let AddressDict {
                owner,
                controller,
                resolved,
            } = address_dict;

            // Ensure the caller is authorised to transfer the name
            let caller = self.env().caller();
            if caller != owner && !self.allowance(owner, caller, Some(id.clone())) {
                return Err(PSP34Error::NotApproved);
            }

            address_dict.owner = to;
            self.remove_name_from_owner(&owner, &name);
            self.add_name_to_owner(&to, &name);

            if !keep_controller {
                address_dict.controller = to;
                self.remove_name_from_controller(&controller, &name);
                self.add_name_to_controller(&to, &name);
            }

            if !keep_resolving {
                address_dict.resolved = to;
                self.remove_name_from_resolving(&resolved, &name);
                self.add_name_to_resolving(&to, &name);
            }

            if !keep_records {
                self.records.remove(name);
            }

            self.name_to_address_dict.insert(name, &address_dict);
            self.operator_approvals
                .remove((&owner, &caller, &Some(id.clone())));

            self.safe_transfer_check(&caller, &owner, &to, &id, &data)?;

            Self::env().emit_event(Transfer {
                from: Some(owner),
                to: Some(to),
                id,
            });

            Ok(())
        }

        #[cfg_attr(test, allow(unused))]
        fn safe_transfer_check(
            &mut self,
            operator: &AccountId,
            from: &AccountId,
            to: &AccountId,
            id: &Id,
            data: &Vec<u8>,
        ) -> core::result::Result<(), PSP34Error> {
            // @dev This is disabled during tests due to the use of `invoke_contract()` not being
            // supported (tests end up panicking).
            #[cfg(not(test))]
            {
                use ink::env::call::{build_call, ExecutionInput, Selector};

                const BEFORE_RECEIVED_SELECTOR: [u8; 4] = [0xBB, 0x7D, 0xF7, 0x80];

                let result = build_call::<Environment>()
                    .call(*to)
                    .call_flags(ink::env::CallFlags::default().set_allow_reentry(true))
                    .exec_input(
                        ExecutionInput::new(Selector::new(BEFORE_RECEIVED_SELECTOR))
                            .push_arg(operator)
                            .push_arg(from)
                            .push_arg(id)
                            .push_arg::<Vec<u8>>(data.clone()),
                    )
                    .returns::<core::result::Result<(), u8>>()
                    .params()
                    .try_invoke();

                match result {
                    Ok(v) => {
                        ink::env::debug_println!(
                            "Received return value \"{:?}\" from contract {:?}",
                            v.clone()
                                .expect("Call should be valid, don't expect a `LangError`."),
                            from
                        );
                        assert_eq!(
                            v.clone()
                                .expect("Call should be valid, don't expect a `LangError`."),
                            Ok(()),
                            "The recipient contract at {to:?} does not accept token transfers.\n
                            Expected: Ok(()), Got {v:?}"
                        )
                    }
                    Err(e) => {
                        match e {
                            ink::env::Error::CodeNotFound | ink::env::Error::NotCallable => {
                                // Our recipient wasn't a smart contract, so there's nothing more for
                                // us to do
                                ink::env::debug_println!(
                                    "Recipient at {:?} from is not a smart contract ({:?})",
                                    from,
                                    e
                                );
                            }
                            _ => {
                                // We got some sort of error from the call to our recipient smart
                                // contract, and as such we must revert this call
                                panic!("Got error \"{e:?}\" while trying to call {from:?}")
                            }
                        }
                    }
                }
            }

            Ok(())
        }

        /// Adds a name to owners' collection
        fn add_name_to_owner(&mut self, owner: &AccountId, name: &str) {
            let name = name.to_string();
            let count = self.get_owner_to_name_count(*owner);

            self.owner_to_names.insert((owner, &count), &name);
            self.name_to_owner_index.insert(&name, &count);
            self.owner_to_name_count.insert(owner, &(count + 1));
        }

        /// Adds a name to controllers' collection
        fn add_name_to_controller(&mut self, controller: &AccountId, name: &str) {
            let name = name.to_string();
            let count = self.get_controller_to_name_count(*controller);

            self.controller_to_names.insert((controller, &count), &name);
            self.name_to_controller_index.insert(&name, &count);
            self.controller_to_name_count
                .insert(controller, &(count + 1));
        }

        /// Adds a name to resolvings' collection
        fn add_name_to_resolving(&mut self, resolving: &AccountId, name: &str) {
            let name = name.to_string();
            let count = self.get_resolving_to_name_count(*resolving);

            self.resolving_to_names.insert((resolving, &count), &name);
            self.name_to_resolving_index.insert(&name, &count);
            self.resolving_to_name_count.insert(resolving, &(count + 1));
        }

        /// Deletes a name from owner
        fn remove_name_from_owner(&mut self, owner: &AccountId, name: &str) {
            let idx = self.name_to_owner_index.get(name).expect("Infallible");
            let count = self.get_owner_to_name_count(*owner);

            // if name is not stored at the last index
            if idx != count - 1 {
                // swap last index item to pos:idx
                let last_name = self
                    .owner_to_names
                    .get((owner, (count - 1)))
                    .expect("Infallible");
                self.owner_to_names.insert((owner, idx), &last_name);
                self.name_to_owner_index.insert(&last_name, &idx);
            }

            // remove last index
            self.owner_to_names.remove((owner, count - 1));
            self.name_to_owner_index.remove(name);
            self.owner_to_name_count.insert(owner, &(count - 1));
        }

        /// Deletes a name from controllers' collection
        fn remove_name_from_controller(&mut self, controller: &AccountId, name: &str) {
            let idx = self.name_to_controller_index.get(name).expect("Infallible");
            let count = self.get_controller_to_name_count(*controller);

            // if name is not stored at the last index
            if idx != count - 1 {
                // swap last index item to pos:idx
                let last_name = self
                    .controller_to_names
                    .get((controller, (count - 1)))
                    .expect("Infallible");
                self.controller_to_names
                    .insert((controller, idx), &last_name);
                self.name_to_controller_index.insert(&last_name, &idx);
            }

            // remove last index
            self.controller_to_names.remove((controller, count - 1));
            self.name_to_controller_index.remove(name);
            self.controller_to_name_count
                .insert(controller, &(count - 1));
        }

        /// Deletes a name from resolvings' collection
        fn remove_name_from_resolving(&mut self, resolving: &AccountId, name: &str) {
            let idx = self.name_to_resolving_index.get(name).expect("Infallible");
            let count = self.get_resolving_to_name_count(*resolving);

            // if name is not stored at the last index
            if idx != count - 1 {
                // swap last index item to pos:idx
                let last_name = self
                    .resolving_to_names
                    .get((resolving, (count - 1)))
                    .expect("Infallible");
                self.resolving_to_names.insert((resolving, idx), &last_name);
                self.name_to_resolving_index.insert(&last_name, &idx);
            }

            // remove last index
            self.resolving_to_names.remove((resolving, count - 1));
            self.name_to_resolving_index.remove(name);
            self.resolving_to_name_count.insert(resolving, &(count - 1));

            /* Check if the resolved address had this name set as the primary name */
            /* If yes -> clear it */
            if self.address_to_primary_name.get(resolving) == Some(name.to_string()) {
                self.address_to_primary_name.remove(resolving);

                self.env().emit_event(SetPrimaryName {
                    account: *resolving,
                    primary_name: None,
                });
            }
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
            let mut referrer_addr = None;

            // Only in public phase
            if !self.is_whitelist_phase() {
                if let Some(referrer_name) = referrer {
                    if self.validate_referrer(recipient, referrer_name.clone()) {
                        referrer_addr = Some(self.get_address(referrer_name).unwrap());
                        discount = 5 * price / 100; // 5% discount
                    }
                }
            }

            Ok((base_price, premium, discount, referrer_addr))
        }

        #[ink(message)]
        pub fn validate_referrer(&self, recipient: AccountId, referrer_name: String) -> bool {
            self.get_address_dict_ref(&referrer_name)
                .map_or(false, |x| {
                    recipient != x.owner && recipient != x.controller && recipient != x.resolved
                })
        }

        fn get_address_dict_ref(&self, name: &str) -> Result<AddressDict> {
            self.name_to_address_dict
                .get(name)
                .filter(|_| self.has_name_expired(name) == Ok(false))
                .ok_or(Error::NameDoesntExist)
        }

        fn get_records_ref(&self, name: &str) -> Vec<(String, String)> {
            self.records
                .get(name)
                .filter(|_| self.has_name_expired(name) == Ok(false))
                .unwrap_or_default()
        }

        fn get_registration_period_ref(&self, name: &str) -> Result<(u64, u64)> {
            self.name_to_period.get(name).ok_or(Error::NameDoesntExist)
        }

        fn has_name_expired(&self, name: &str) -> Result<bool> {
            match self.name_to_period.get(name) {
                Some((_, expiry)) => Ok(expiry <= self.env().block_timestamp()),
                None => Err(Error::NameDoesntExist),
            }
        }

        fn get_static_attribute_ref(&self, name: &str, key: &str) -> Option<String> {
            match key {
                "TLD" => Some(self.tld.clone()),
                "Length" => Some(name.chars().count().to_string()),
                "Registration" => Some(match self.get_registration_period_ref(&name) {
                    Ok(period) => period.0.to_string(),
                    _ => String::new(),
                }),
                "Expiration" => Some(match self.get_registration_period_ref(&name) {
                    Ok(period) => period.1.to_string(),
                    _ => String::new(),
                }),
                _ => None,
            }
        }
    }

    impl PSP34 for Registry {
        // TLD is our collection id
        #[ink(message)]
        fn collection_id(&self) -> Id {
            let id = ".".to_string() + &self.tld.to_ascii_uppercase() + " Domains";
            id.into()
        }

        #[ink(message)]
        fn balance_of(&self, owner: AccountId) -> u32 {
            self.get_owned_names_of_address(owner).len() as u32
        }

        #[ink(message)]
        fn owner_of(&self, id: Id) -> Option<AccountId> {
            id.try_into().map_or(None, |name| self.get_owner(name).ok())
        }

        #[ink(message)]
        fn allowance(&self, owner: AccountId, operator: AccountId, id: Option<Id>) -> bool {
            if id.is_some() && self.operator_approvals.contains(&(owner, operator, None)) {
                return true;
            }
            self.operator_approvals.contains(&(owner, operator, id))
        }

        #[ink(message)]
        fn approve(
            &mut self,
            operator: AccountId,
            id: Option<Id>,
            approved: bool,
        ) -> core::result::Result<(), PSP34Error> {
            let mut caller = self.env().caller();

            if operator == [0u8; 32].into() {
                return Err(PSP34Error::Custom("Zero address".to_string()));
            }

            if let Some(id) = &id {
                let owner = self
                    .owner_of(id.clone())
                    .ok_or(PSP34Error::TokenNotExists)?;

                if approved && owner == operator {
                    return Err(PSP34Error::SelfApprove);
                }

                if owner != caller && !self.allowance(owner, caller, None) {
                    return Err(PSP34Error::NotApproved);
                };
                caller = owner;
            }

            match approved {
                true => {
                    self.operator_approvals
                        .insert((&caller, &operator, &id), &());
                }
                false => self.operator_approvals.remove((&caller, &operator, &id)),
            }

            // Emit event
            self.env().emit_event(Approval {
                owner: caller,
                operator,
                id,
                approved,
            });

            Ok(())
        }

        #[ink(message)]
        fn transfer(
            &mut self,
            to: AccountId,
            id: Id,
            data: Vec<u8>,
        ) -> core::result::Result<(), PSP34Error> {
            let name: String = id.try_into()?;
            self.transfer_name(to, &name, false, false, false, &data)
        }

        #[ink(message)]
        fn total_supply(&self) -> Balance {
            self.total_supply
        }
    }

    impl PSP34Enumerable for Registry {
        #[ink(message)]
        fn owners_token_by_index(
            &self,
            owner: AccountId,
            index: u128,
        ) -> core::result::Result<Id, PSP34Error> {
            let tokens = self.get_owned_names_of_address(owner);

            match tokens.get(index as usize) {
                Some(name) => Ok(name.clone().into()),
                None => Err(PSP34Error::TokenNotExists),
            }
        }

        #[ink(message)]
        fn token_by_index(&self, _index: u128) -> core::result::Result<Id, PSP34Error> {
            Err(PSP34Error::Custom("Not Supported".to_string()))
        }
    }

    impl PSP34Metadata for Registry {
        #[ink(message)]
        fn get_attribute(&self, id: Id, key: Vec<u8>) -> Option<Vec<u8>> {
            match TryInto::<String>::try_into(id) {
                Ok(name) => {
                    let Ok(key) = String::from_utf8(key) else {
                        return None;
                    };

                    self.get_static_attribute_ref(&name, &key)
                        .map(|s| s.into_bytes())
                }
                Err(_) => None,
            }
        }
    }

    impl Psp34Traits for Registry {
        #[ink(message)]
        fn get_owner(&self) -> AccountId {
            self.admin
        }

        #[ink(message)]
        fn token_uri(&self, token_id: Id) -> String {
            let name: core::result::Result<String, _> = token_id.try_into();

            match name {
                Ok(name) => self.base_uri.clone() + &name + &String::from(".json"),
                _ => String::new(),
            }
        }

        #[ink(message)]
        fn set_base_uri(&mut self, uri: String) -> core::result::Result<(), ArtZeroError> {
            self.ensure_admin()
                .map_err(|_| ArtZeroError::Custom("Not Authorised".to_string()))?;

            if uri.len() == 0 {
                return Err(ArtZeroError::Custom("Zero length string".to_string()));
            }
            self.base_uri = uri;
            Ok(())
        }

        #[ink(message)]
        fn get_attribute_count(&self) -> u32 {
            4
        }

        #[ink(message)]
        fn get_attribute_name(&self, index: u32) -> String {
            let attr = match index {
                0 => "TLD",
                1 => "Length",
                2 => "Registration",
                3 => "Expiration",
                _ => "",
            };
            attr.into()
        }

        #[ink(message)]
        fn get_attributes(&self, token_id: Id, attributes: Vec<String>) -> Vec<String> {
            let name: String = match token_id
                .try_into()
                .map_err(|_| ArtZeroError::Custom("TokenNotFound".to_string()))
            {
                Ok(name) => name,
                _ => return Default::default(),
            };

            attributes
                .into_iter()
                .map(|key| {
                    self.get_static_attribute_ref(&name, &key)
                        .unwrap_or_default()
                })
                .collect()
        }

        #[ink(message)]
        fn set_multiple_attributes(
            &mut self,
            _token_id: Id,
            _metadata: Vec<(String, String)>,
        ) -> core::result::Result<(), ArtZeroError> {
            Err(ArtZeroError::Custom("Not Supported".to_string()))
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

    fn get_test_name_service() -> Registry {
        let contract_addr: AccountId = AccountId::from([0xFF as u8; 32]);
        set_callee::<DefaultEnvironment>(contract_addr);
        Registry::new(
            default_accounts().alice,
            None,
            None,
            None,
            "azero".to_string(),
            "ipfs://05121999/".to_string(),
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

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );

        /* Now alice owns three names */
        /* getting all owned names should return all three */
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            vec![name, name2, name3]
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

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* Register bar under bob, but set controller to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.set_controller(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* Now alice owns three names */
        /* getting all owned names should return all three */
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            vec![name, name2, name3]
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

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_next_caller(default_accounts.charlie);
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* getting all names should return first only */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name.clone()]
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.set_address(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* getting all names should return all three */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name3.clone(), name.clone()]
        );

        set_next_caller(default_accounts.charlie);
        assert_eq!(
            contract.set_controller(name2.clone(), default_accounts.alice),
            Ok(())
        );

        /* getting all names should return all three */
        assert_eq!(
            contract.get_names_of_address(default_accounts.alice),
            vec![name3, name2, name]
        );
    }

    #[ink::test]
    fn resolving_to_names_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name2.clone(), 1, None, None, false),
            Ok(())
        );

        /* getting all names should return first two */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            vec![name.clone(), name2.clone()]
        );

        /* Register bar under bob, but set resolved address to alice */
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name3.clone(), 1, None, None, false),
            Ok(())
        );
        assert_eq!(
            contract.set_address(name3.clone(), default_accounts.alice),
            Ok(())
        );

        /* Now all three names resolve to alice's address */
        /* getting all resolving names should return all three names */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            vec![name.clone(), name2.clone(), name3.clone()]
        );

        /* Remove the pointer to alice */
        assert_eq!(contract.set_address(name3, default_accounts.bob), Ok(()));

        /* getting all resolving names should return first two names */
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            vec![name, name2]
        );
    }

    #[ink::test]
    fn set_primary_name_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("foo");
        let name3 = String::from("bar");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name2, 1, None, None, false), Ok(()));

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name3, 1, None, None, false), Ok(()));

        /* Now alice owns three names */
        /* Set the primary name for alice's address to name 1 */
        contract.set_primary_name(Some(name.clone())).unwrap();

        /* Now the primary name should resolve to alice's address */
        assert_eq!(
            contract.get_primary_name(default_accounts.alice),
            Ok(name.clone())
        );

        /* Change the resolved address of the first name to bob, invalidating the primary name claim */
        contract
            .set_address(name.clone(), default_accounts.bob)
            .unwrap();

        /* Now the primary name should not resolve to anything */
        assert_eq!(
            contract.get_primary_name(default_accounts.alice),
            Err(Error::NoResolvedAddress)
        );

        /* Set bob's primary name */
        set_next_caller(default_accounts.bob);
        contract.set_primary_name(Some(name.clone())).unwrap();

        /* Now the primary name should not resolve to anything */
        assert_eq!(contract.get_primary_name(default_accounts.bob), Ok(name));
    }

    #[ink::test]
    fn register_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.get_owned_names_of_address(default_accounts.alice),
            Vec::from([name.clone()])
        );
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name, 1, None, None, false),
            Err(Error::NameAlreadyExists)
        );

        // Reserved names cannot be registered
        let reserved_name = String::from("AlephZero");
        let reserved_list = vec![(reserved_name.clone(), Some(default_accounts.alice))];
        contract
            .add_reserved_names(reserved_list)
            .expect("Failed to reserve name");

        assert_eq!(
            contract.register(reserved_name, 1, None, None, false),
            Err(Error::CannotBuyReservedName)
        );
    }

    #[ink::test]
    fn register_with_set_primary_name_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name.clone(), 1, None, None, true), Ok(()));

        assert_eq!(contract.get_primary_name(default_accounts.alice), Ok(name));
    }

    #[ink::test]
    fn register_excess_fee_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();
        let contract_addr = contract.env().account_id();

        set_account_balance::<DefaultEnvironment>(default_accounts.alice, 2000);
        transfer_in::<DefaultEnvironment>(1234);
        assert_eq!(contract.register(name.clone(), 1, None, None, true), Ok(()));

        assert_eq!(
            get_account_balance::<DefaultEnvironment>(default_accounts.alice),
            Ok(1000)
        );

        assert_eq!(
            get_account_balance::<DefaultEnvironment>(contract_addr),
            Ok(1000)
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
        let fees = 1000;
        set_next_caller(default_accounts.bob);
        set_account_balance::<DefaultEnvironment>(default_accounts.bob, fees);
        transfer_in::<DefaultEnvironment>(fees);
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));

        // Alice (admin) withdraws the funds
        set_next_caller(default_accounts.alice);

        let balance_before =
            get_account_balance::<DefaultEnvironment>(default_accounts.alice).unwrap();
        assert_eq!(contract.withdraw(None, Some(fees)), Ok(()));
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
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));

        set_next_caller(default_accounts.bob);
        assert_eq!(contract.withdraw(None, None), Err(Error::NotAdmin));
    }

    #[ink::test]
    fn reverse_search_works() {
        let default_accounts = default_accounts();
        let name = String::from("test");
        let name2 = String::from("test2");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name, 1, None, None, false), Ok(()));
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(contract.register(name2, 1, None, None, false), Ok(()));
        assert!(contract
            .get_owned_names_of_address(default_accounts.alice)
            .contains(&String::from("test")));
        assert!(contract
            .get_owned_names_of_address(default_accounts.alice)
            .contains(&String::from("test2")));
    }

    #[ink::test]
    fn register_empty_reverts() {
        let default_accounts = default_accounts();
        let name = String::from("");

        set_next_caller(default_accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
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

        set_value_transferred::<DefaultEnvironment>(1000);
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

        set_value_transferred::<DefaultEnvironment>(1000);
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
            Vec::from([name.clone()])
        );
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            Vec::from([name.clone()])
        );
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Vec::from([name.clone()])
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
            Vec::<String>::new()
        );
        assert_eq!(
            contract.get_controlled_names_of_address(default_accounts.alice),
            Vec::<String>::new()
        );
        assert_eq!(
            contract.get_resolving_names_of_address(default_accounts.alice),
            Vec::<String>::new()
        );

        /* Another account can register again*/
        set_next_caller(default_accounts.bob);
        set_value_transferred::<DefaultEnvironment>(1000);
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
        set_value_transferred::<DefaultEnvironment>(1000);
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();

        // Caller is not controller, `set_address` should fail.
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.set_address(name.clone(), accounts.bob),
            Err(Error::CallerIsNotController)
        );

        /* Caller is not controller, `update_records` should fail */
        set_next_caller(accounts.bob);
        assert_eq!(
            contract.update_records(
                name.clone(),
                Vec::from([("twitter".to_string(), None)]),
                false,
            ),
            Err(Error::CallerIsNotController)
        );

        // Caller is controller, `update_records` should pass
        set_next_caller(accounts.alice);
        assert_eq!(
            contract.update_records(name, Vec::from([("twitter".to_string(), None)]), false),
            Ok(())
        );
    }

    #[ink::test]
    fn set_address_works() {
        let accounts = default_accounts();
        let name = String::from("test");

        set_next_caller(accounts.alice);

        let mut contract = get_test_name_service();
        set_value_transferred::<DefaultEnvironment>(1000);
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
        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name.clone(), 1, None, None, false),
            Ok(())
        );

        // Test transfer of owner.
        assert_eq!(
            contract.transfer(accounts.bob, name.clone(), false, false, false, vec![]),
            Ok(())
        );

        assert_eq!(
            contract.get_owned_names_of_address(accounts.alice),
            Vec::<String>::new()
        );
        assert_eq!(
            contract.get_owned_names_of_address(accounts.bob),
            Vec::from([name.clone()])
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
    fn records_works() {
        let accounts = default_accounts();
        let key = String::from("twitter");
        let value = String::from("@test");
        let records = Vec::from([(key.clone(), Some(value.clone()))]);

        let name_name = "test".to_string();

        set_next_caller(accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name_name.clone(), 1, None, None, false),
            Ok(())
        );

        assert_eq!(
            contract.update_records(name_name.clone(), records.clone(), false),
            Ok(())
        );
        assert_eq!(
            contract.get_record(name_name.clone(), key.clone()).unwrap(),
            value
        );

        /* Confirm idempotency */
        assert_eq!(
            contract.update_records(name_name.clone(), records, true),
            Ok(())
        );
        assert_eq!(contract.get_record(name_name.clone(), key).unwrap(), value);

        /* Confirm overwriting */
        assert_eq!(
            contract.update_records(
                name_name.clone(),
                Vec::from([("twitter".to_string(), Some("@newtest".to_string()))]),
                false,
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_all_records(name_name),
            Vec::from([("twitter".to_string(), "@newtest".to_string())])
        );
    }

    #[ink::test]
    fn set_record_works() {
        let accounts = default_accounts();
        let key = String::from("twitter");
        let value = String::from("@test");

        let name_name = "test".to_string();

        set_next_caller(accounts.alice);
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        assert_eq!(
            contract.register(name_name.clone(), 1, None, None, false),
            Ok(())
        );

        assert_eq!(
            contract.update_records(
                name_name.clone(),
                vec![(key.clone(), Some(value.clone()))],
                false,
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_record(name_name.clone(), key.clone()).unwrap(),
            value
        );

        /* Confirm idempotency */
        assert_eq!(
            contract.update_records(
                name_name.clone(),
                vec![(key.clone(), Some(value.clone()))],
                false,
            ),
            Ok(())
        );
        assert_eq!(contract.get_record(name_name.clone(), key).unwrap(), value);

        /* Confirm overwriting */
        assert_eq!(
            contract.update_records(
                name_name.clone(),
                vec![("twitter".to_string(), Some("@newtest".to_string()))],
                false,
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_all_records(name_name),
            Vec::from([("twitter".to_string(), "@newtest".to_string())])
        );
    }

    #[ink::test]
    fn update_records_works() {
        let name = "test".to_string();
        let mut contract = get_test_name_service();

        set_value_transferred::<DefaultEnvironment>(1000);
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();

        // add initial records values
        assert_eq!(
            contract.update_records(
                name.clone(),
                vec![
                    ("@facebook".to_string(), Some("alice_zuk".to_string())),
                    ("@instagram".to_string(), Some("alice_zuk".to_string())),
                    ("@twitter".to_string(), Some("alice_musk".to_string())),
                ],
                true
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_all_records(name.clone()),
            vec![
                ("@facebook".to_string(), "alice_zuk".to_string()),
                ("@instagram".to_string(), "alice_zuk".to_string()),
                ("@twitter".to_string(), "alice_musk".to_string()),
            ]
        );

        // add 1 new record
        // remove 1 existing record
        // update 1 existing record
        assert_eq!(
            contract.update_records(
                name.clone(),
                vec![
                    ("@reddit".to_string(), Some("alice_tut".to_string())),
                    ("@instagram".to_string(), None),
                    ("@twitter".to_string(), Some("elon_musk".to_string()))
                ],
                false,
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_all_records(name.clone()),
            vec![
                ("@facebook".to_string(), "alice_zuk".to_string()),
                ("@reddit".to_string(), "alice_tut".to_string()),
                ("@twitter".to_string(), "elon_musk".to_string()),
            ]
        );

        // add a record with flag: remove_rest
        assert_eq!(
            contract.update_records(
                name.clone(),
                vec![("@field".to_string(), Some("alice_tut".to_string()))],
                true,
            ),
            Ok(())
        );
        assert_eq!(
            contract.get_all_records(name.clone()),
            vec![("@field".to_string(), "alice_tut".to_string())],
        );
    }

    #[ink::test]
    fn records_limit_works() {
        let mut contract = get_test_name_service();
        let name = "alice".to_string();
        let records = vec![
            ("@twitter".to_string(), Some("alice_musk".to_string())),
            ("@facebook".to_string(), Some("alice_zuk".to_string())),
            ("@instagram".to_string(), Some("alice_zuk".to_string())),
        ];

        contract.set_records_size_limit(Some(41)).unwrap();
        assert_eq!(contract.get_records_size_limit(), Some(41));

        set_value_transferred::<DefaultEnvironment>(1000);
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();

        // With current input, records cannot be stored simultaneously
        assert_eq!(
            contract.update_records(name.clone(), records.clone(), false),
            Err(Error::RecordsOverflow)
        );

        // Storing only one works
        assert_eq!(
            contract.update_records(name.clone(), records[0..1].to_vec(), true),
            Ok(())
        );

        // Adding the second record fails
        assert_eq!(
            contract.update_records(name.clone(), records[1..3].to_vec(), false),
            Err(Error::RecordsOverflow),
        );
    }

    #[ink::test]
    fn add_reserved_names_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let reserved_name = String::from("AlephZero");
        let list = vec![(reserved_name.clone(), Some(accounts.alice))];

        assert!(contract.add_reserved_names(list).is_ok());

        assert_eq!(
            contract.get_name_status(vec![reserved_name]),
            vec![NameStatus::Reserved(Some(accounts.alice))],
        );

        // Cannot reserve already registered-name
        let name = "alice".to_string();
        set_value_transferred::<DefaultEnvironment>(1000);
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();
        assert_eq!(
            contract.add_reserved_names(vec![(name, None)]),
            Err(Error::NameAlreadyExists)
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(contract.add_reserved_names(vec![]), Err(Error::NotAdmin));
    }

    #[ink::test]
    fn remove_reserved_names_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let reserved_name = String::from("AlephZero");
        let list = vec![(reserved_name.clone(), Some(accounts.alice))];
        assert!(contract.add_reserved_names(list).is_ok());

        assert_eq!(
            contract.get_name_status(vec![reserved_name.clone()]),
            vec![NameStatus::Reserved(Some(accounts.alice))],
        );

        assert!(contract
            .remove_reserved_name(vec![reserved_name.clone()])
            .is_ok());

        assert_ne!(
            contract.get_name_status(vec![reserved_name]),
            vec![NameStatus::Reserved(Some(accounts.alice))],
        );

        // Invocation from non-admin address fails
        set_next_caller(accounts.bob);
        assert_eq!(contract.remove_reserved_name(vec![]), Err(Error::NotAdmin));
    }

    #[ink::test]
    fn claim_reserved_name_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        let name = String::from("bob");
        let reserved_list = vec![(name.clone(), Some(accounts.bob))];
        contract
            .add_reserved_names(reserved_list)
            .expect("Failed to add reserved name");

        // Non-reserved name cannot be claimed
        assert_eq!(
            contract.claim_reserved_name("abcd".to_string()),
            Err(Error::NotReservedName),
        );

        // Non-authorised user cannot claim reserved name
        assert_eq!(
            contract.claim_reserved_name(name.clone()),
            Err(Error::NotAuthorised),
        );

        // Authorised user can claim name reserved for them
        set_next_caller(accounts.bob);
        assert!(contract.claim_reserved_name(name.clone()).is_ok());

        let address_dict = AddressDict::new(accounts.bob);
        assert_eq!(
            contract.get_name_status(vec![name]),
            vec![NameStatus::Registered(address_dict)],
        );
    }

    #[ink::test]
    fn get_name_status_works() {
        let accounts = default_accounts();
        let reserved_list = vec![("bob".to_string(), Some(accounts.bob))];

        let mut contract = Registry::new(
            default_accounts().alice,
            None,
            None,
            None,
            "azero".to_string(),
            "ipfs://05121999/".to_string(),
        );

        contract.add_reserved_names(reserved_list).unwrap();

        set_value_transferred::<DefaultEnvironment>(1000);
        contract
            .register("alice".to_string(), 1, None, None, false)
            .expect("failed to register name");

        let address_dict = AddressDict::new(accounts.alice);
        assert_eq!(
            contract.get_name_status(vec!["alice".to_string()]),
            vec![NameStatus::Registered(address_dict)]
        );

        assert_eq!(
            contract.get_name_status(vec!["bob".to_string()]),
            vec![NameStatus::Reserved(Some(accounts.bob))]
        );

        assert_eq!(
            contract.get_name_status(vec!["david".to_string()]),
            vec![NameStatus::Available]
        );

        assert_eq!(
            contract.get_name_status(vec!["".to_string()]),
            vec![NameStatus::Unavailable]
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
        // Fee without discount: 1000
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
        // Fee after discount: 9950
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
    fn validate_referrer_works() {
        let default_accounts = default_accounts();
        let mut contract = get_test_name_service();

        let name = "alice".to_string();

        // Invalid name -> fails
        assert_eq!(
            contract.validate_referrer(default_accounts.alice, name.clone()),
            false
        );

        transfer_in::<DefaultEnvironment>(1000);
        contract
            .register(name.clone(), 1, None, None, false)
            .unwrap();
        contract
            .set_controller(name.clone(), default_accounts.bob)
            .unwrap();
        contract
            .set_address(name.clone(), default_accounts.eve)
            .unwrap();

        // owner: fails
        assert_eq!(
            contract.validate_referrer(default_accounts.alice, name.clone()),
            false
        );

        // controller: fails
        assert_eq!(
            contract.validate_referrer(default_accounts.bob, name.clone()),
            false
        );

        // resolved: fails
        assert_eq!(
            contract.validate_referrer(default_accounts.eve, name.clone()),
            false
        );

        // A new user: pass
        assert_eq!(
            contract.validate_referrer(default_accounts.charlie, name.clone()),
            true
        );
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
            contract.get_name_status(vec![name1.clone(), name2.clone()]),
            vec![NameStatus::Available, NameStatus::Registered(address_dict)]
        );

        assert_eq!(
            contract.get_primary_name(default_accounts().alice),
            Err(Error::NoResolvedAddress)
        );

        assert_eq!(contract.get_all_records(name1.clone()), vec![]);

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
            contract.get_name_status(vec![name1.clone(), name2.clone()]),
            vec![NameStatus::Available, NameStatus::Registered(address_dict)]
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
    fn ownable_2_step_works() {
        let accounts = default_accounts();
        let mut contract = get_test_name_service();

        assert_eq!(contract.get_admin(), accounts.alice);
        contract.transfer_ownership(Some(accounts.bob)).unwrap();

        assert_eq!(contract.get_admin(), accounts.alice);

        set_caller::<DefaultEnvironment>(accounts.bob);
        contract.accept_ownership().unwrap();
        assert_eq!(contract.get_admin(), accounts.bob);
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

    //     // 4. Verify that valid proof works and the name is registered

    //     // 5. Verify a user can claim only one name during whitelist-phase

    //     // 6. Verify `release()` fails

    //     // 7. Verify `transfer()` fails

    //     // 8. Verify `switch_to_public_phase()` works
    // }
}
