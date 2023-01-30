use ink::primitives::AccountId;

/// Stores the addresses relevant to a domain name
#[derive(scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct AddressDict {
    pub owner: AccountId,
    pub controller: AccountId,
    pub resolved: AccountId,
}

impl AddressDict {
    pub fn new(account: AccountId) -> Self {
        Self {
            owner: account,
            controller: account,
            resolved: account,
        }
    }

    pub fn set_owner(&mut self, new_owner: AccountId) -> &mut Self {
        self.owner = new_owner;
        self
    }

    pub fn set_controller(&mut self, new_controller: AccountId) -> &mut Self {
        self.controller = new_controller;
        self
    }

    pub fn set_resolved(&mut self, new_resolved: AccountId) -> &mut Self {
        self.resolved = new_resolved;
        self
    }
}
