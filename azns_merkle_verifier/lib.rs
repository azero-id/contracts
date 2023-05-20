#![cfg_attr(not(feature = "std"), no_std)]

pub use self::merkle_verifier::{MerkleVerifier, MerkleVerifierRef};

#[util_macros::azns_contract(Ownable2Step[
    Error = Error::NotAdmin
])]
#[util_macros::azns_contract(Upgradable)]
#[ink::contract]
mod merkle_verifier {

    use ink::env::hash::{CryptoHash, Keccak256};
    use ink::prelude::vec::Vec;

    #[ink(storage)]
    pub struct MerkleVerifier {
        /// Admin can update the root
        admin: AccountId,
        /// Two-step ownership transfer AccountId
        pending_admin: Option<AccountId>,
        /// Stores the merkle root hash
        root: [u8; 32],
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Caller not allowed to call privileged calls.
        NotAdmin,
    }

    impl MerkleVerifier {
        #[ink(constructor)]
        pub fn new(admin: AccountId, root: [u8; 32]) -> Self {
            Self {
                admin,
                pending_admin: None,
                root,
            }
        }

        #[ink(message)]
        pub fn update_root(&mut self, new_root: [u8; 32]) -> Result<(), Error> {
            self.ensure_admin()?;
            self.root = new_root;
            Ok(())
        }

        /// Returns the merkle root
        #[ink(message)]
        pub fn root(&self) -> [u8; 32] {
            self.root
        }

        /// Verifies inclusion of leaf in the merkle tree
        // @dev leaf - It's the hashed version of the element
        #[ink(message)]
        pub fn verify_proof(&self, leaf: [u8; 32], proof: Vec<[u8; 32]>) -> bool {
            let hash = proof
                .iter()
                .fold(leaf, |acc, node| Self::compute_hash(&acc, &node));
            hash == self.root
        }

        // Sorts the node and then returns their Keccak256 hash
        fn compute_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
            // Sorted pair hashing
            let input = if left < right {
                [left.as_ref(), right].concat()
            } else {
                [right.as_ref(), left].concat()
            };

            let input = input.as_ref();
            let mut output = [0u8; 32];
            Keccak256::hash(input, &mut output);
            output
        }
    }

    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;
        use ink::env::hash::{CryptoHash, Sha2x256};
        use ink::env::test::default_accounts;
        use ink::env::DefaultEnvironment;

        #[ink::test]
        fn root_works() {
            let alice = default_accounts::<DefaultEnvironment>().alice;
            let root = [0xff; 32];
            let merkle_verifier = MerkleVerifier::new(alice, root);

            assert_eq!(merkle_verifier.root(), root);
        }

        #[ink::test]
        fn update_root_works() {
            let alice = default_accounts::<DefaultEnvironment>().alice;
            let root = [0xff; 32];
            let mut merkle_verifier = MerkleVerifier::new(alice, root);

            let new_root = [0x00; 32];
            assert_eq!(merkle_verifier.update_root(new_root), Ok(()));

            assert_eq!(merkle_verifier.root(), new_root);
        }

        #[ink::test]
        fn ownable_2_step_works() {
            let accounts = default_accounts::<DefaultEnvironment>();
            let root = [0xff; 32];
            let mut contract = MerkleVerifier::new(accounts.alice, root);

            assert_eq!(contract.get_admin(), accounts.alice);
            contract.transfer_ownership(Some(accounts.bob)).unwrap();

            assert_eq!(contract.get_admin(), accounts.alice);

            ink::env::test::set_caller::<DefaultEnvironment>(accounts.bob);
            contract.accept_ownership().unwrap();
            assert_eq!(contract.get_admin(), accounts.bob);
        }

        #[ink::test]
        fn only_admin_works() {
            let accounts = default_accounts::<DefaultEnvironment>();
            let root = [0xff; 32];
            let mut merkle_verifier = MerkleVerifier::new(accounts.alice, root);

            // Verify update_root fails
            ink::env::test::set_caller::<DefaultEnvironment>(accounts.bob);

            let new_root = [0x00; 32];
            assert_eq!(merkle_verifier.update_root(new_root), Err(Error::NotAdmin));
        }

        // Test that the param ordering should not matter
        #[ink::test]
        fn compute_hash_works() {
            let first = [0x00; 32];
            let second = [0xff; 32];

            assert_eq!(
                MerkleVerifier::compute_hash(&first, &second),
                MerkleVerifier::compute_hash(&second, &first)
            );
        }

        #[ink::test]
        fn verify_proof_works() {
            /*
                Tree structure used for tesing

                              H(ABCD)               <----- Root (Hash: Keccak256)
                          /            \
                       H(AB)          H(CD)         <----- Internal Nodes (Hash: Keccak256)
                      /     \         /    \
                    H(A)    H(B)    H(C)    H(D)    <----- Leaves (Hash: SHA2)
                     |       |       |       |
                     A      *B       C       D      <----- Items
            */

            // Items for which Merkle tree is to be constructed
            let items = ["a", "b", "c", "d"];

            // Leaves are the hash (used SHA2 here) of items
            let leaves: Vec<[u8; 32]> = items
                .iter()
                .map(|x| {
                    let mut output = [0u8; 32];
                    Sha2x256::hash(x.as_bytes(), &mut output);
                    output
                })
                .collect();

            let internal_nodes = [
                MerkleVerifier::compute_hash(&leaves[0], &leaves[1]),
                MerkleVerifier::compute_hash(&leaves[2], &leaves[3]),
            ];

            let root = MerkleVerifier::compute_hash(&internal_nodes[0], &internal_nodes[1]);

            // Create the MerkleVerifier contract
            let alice = default_accounts::<DefaultEnvironment>().alice;
            let merkle_verifier = MerkleVerifier::new(alice, root);

            // Prove leaves[1] is a part of the tree
            let leaf = leaves[1];

            // Case 1: Construct an invalid proof
            let proof = vec![leaves[0], internal_nodes[0]];
            let res = merkle_verifier.verify_proof(leaf, proof);
            assert_eq!(res, false);

            // Case 2: Construct a valid proof
            let proof = vec![leaves[0], internal_nodes[1]];
            let res = merkle_verifier.verify_proof(leaf, proof);
            assert_eq!(res, true);
        }

        #[ink::test]
        fn keccak256_works() {
            let mut hash = [0u8; 32];
            ink::env::hash::Keccak256::hash("hello".as_bytes(), &mut hash);

            assert_eq!(
                hex::decode("1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8")
                    .unwrap(),
                hash
            );
        }

        #[ink::test]
        fn sha256_works() {
            let mut hash = [0u8; 32];
            ink::env::hash::Sha2x256::hash("hello".as_bytes(), &mut hash);

            assert_eq!(
                hex::decode("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
                    .unwrap(),
                hash
            );
        }
    }
}
