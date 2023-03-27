use ink::prelude::{
    string::{String, ToString},
    vec::Vec,
};
use ink::primitives::AccountId;

type Balance = u128;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub enum Id {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Bytes(Vec<u8>),
}

/// The PSP34 error type. Contract will throw one of this errors.
#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum PSP34Error {
    /// Custom error type for cases if writer of traits added own restrictions
    Custom(String),
    /// Returned if owner approves self
    SelfApprove,
    /// Returned if the caller doesn't have allowance for transferring.
    NotApproved,
    /// Returned if the owner already own the token.
    TokenExists,
    /// Returned if  the token doesn't exist
    TokenNotExists,
    /// Returned if safe transfer check fails
    SafeTransferCheckFailed(String),
}

impl TryFrom<Id> for String {
    type Error = PSP34Error;

    fn try_from(value: Id) -> core::result::Result<Self, Self::Error> {
        match value {
            Id::Bytes(bytes) => String::from_utf8(bytes)
                .map_err(|_| PSP34Error::Custom("Invalid id. Expected UTF8 string".to_string())),
            _ => Err(PSP34Error::Custom(
                "Invalid id. Expected UTF8 string".to_string(),
            )),
        }
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Self::Bytes(value.as_bytes().to_vec())
    }
}

#[ink::trait_definition]
pub trait PSP34 {
    /// Returns the collection `Id` of the NFT token.
    ///
    /// This can represents the relationship between tokens/contracts/pallets.
    #[ink(message)]
    fn collection_id(&self) -> Id;

    /// Returns the balance of the owner.
    ///
    /// This represents the amount of unique tokens the owner has.
    #[ink(message)]
    fn balance_of(&self, owner: AccountId) -> u32;

    /// Returns the owner of the token if any.
    #[ink(message)]
    fn owner_of(&self, id: Id) -> Option<AccountId>;

    /// Returns `true` if the operator is approved by the owner to withdraw `id` token.
    /// If `id` is `None`, returns `true` if the operator is approved to withdraw all owner's tokens.
    #[ink(message)]
    fn allowance(&self, owner: AccountId, operator: AccountId, id: Option<Id>) -> bool;

    /// Approves `operator` to withdraw the `id` token from the caller's account.
    /// If `id` is `None` approves or disapproves the operator for all tokens of the caller.
    ///
    /// On success a `Approval` event is emitted.
    ///
    /// # Errors
    ///
    /// Returns `SelfApprove` error if it is self approve.
    ///
    /// Returns `NotApproved` error if caller is not owner of `id`.
    #[ink(message)]
    fn approve(
        &mut self,
        operator: AccountId,
        id: Option<Id>,
        approved: bool,
    ) -> Result<(), PSP34Error>;

    /// Transfer approved or owned token from caller.
    ///
    /// On success a `Transfer` event is emitted.
    ///
    /// # Errors
    ///
    /// Returns `TokenNotExists` error if `id` does not exist.
    ///
    /// Returns `NotApproved` error if `from` doesn't have allowance for transferring.
    ///
    /// Returns `SafeTransferCheckFailed` error if `to` doesn't accept transfer.
    #[ink(message)]
    fn transfer(&mut self, to: AccountId, id: Id, data: Vec<u8>) -> Result<(), PSP34Error>;

    /// Returns current NFT total supply.
    #[ink(message)]
    fn total_supply(&self) -> Balance;
}
