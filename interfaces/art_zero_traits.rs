use crate::psp34_standard::Id;
use ink::prelude::{string::String, vec::Vec};
use ink::primitives::AccountId;

#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum ArtZeroError {
    Custom(String),
}

#[ink::trait_definition]
pub trait Psp34Traits {
    /// This function sets the baseURI for the NFT contract. Only Contract Owner can perform this function. baseURI is the location of the metadata files if the NFT collection use external source to keep their NFT artwork. ArtZero uses IPFS by default, the baseURI can have format like this: ipfs://<hash_ID>/
    #[ink(message)]
    fn set_base_uri(&mut self, uri: String) -> Result<(), ArtZeroError>;

    /// This function set the attributes to each NFT. Only Contract Owner can perform this function. The metadata input is an array of [(attribute, value)]. The attributes in ArtZero platform are the NFT traits.
    #[ink(message)]
    fn set_multiple_attributes(
        &mut self,
        token_id: Id,
        metadata: Vec<(String, String)>,
    ) -> Result<(), ArtZeroError>;

    /// This function returns all available attributes of each NFT
    #[ink(message)]
    fn get_attributes(&self, token_id: Id, attributes: Vec<String>) -> Vec<String>;

    /// This function return how many unique attributes in the contract
    #[ink(message)]
    fn get_attribute_count(&self) -> u32;

    /// This function return the attribute name using attribute index. Beacause attributes of an NFT can be set to anything by Contract Owner, AztZero uses this function to get all attributes of an NFT
    #[ink(message)]
    fn get_attribute_name(&self, index: u32) -> String;

    /// This function return the metadata location of an NFT. The format is baseURI/<token_id>.json
    #[ink(message)]
    fn token_uri(&self, token_id: Id) -> String;

    /// This function return the owner of the NFT Contract
    #[ink(message)]
    fn get_owner(&self) -> AccountId;
}
