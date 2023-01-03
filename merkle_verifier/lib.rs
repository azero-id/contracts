#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod merkle_verifier {

    use ink::env::hash::{CryptoHash, Keccak256};
    use ink::prelude::vec::Vec;

    #[ink(storage)]
    pub struct MerkleVerifier {
        /// Stores the merkle root hash
        root: [u8; 32],
    }

    impl MerkleVerifier {
        #[ink(constructor)]
        pub fn new(root: [u8; 32]) -> Self {
            Self { root }
        }

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

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;
        use rs_merkle::{algorithms::Sha256, Hasher, MerkleProof, MerkleTree};

        /// We test a simple use case of our contract.
        #[ink::test]
        fn it_works() {
            let leaf_values = ["a", "b", "c", "d", "e", "f"];
            let leaves: Vec<[u8; 32]> = leaf_values
                .iter()
                .map(|x| Sha256::hash(x.as_bytes()))
                .collect();

            let merkle_tree = MerkleTree::<Sha256>::from_leaves(&leaves);
            let indices_to_prove = vec![3, 4];
            let leaves_to_prove = leaves.get(3..5).ok_or("can't get leaves to prove").unwrap();
            let merkle_proof = merkle_tree.proof(&indices_to_prove);
            let merkle_root = merkle_tree
                .root()
                .ok_or("couldn't get the merkle root")
                .unwrap();
            // Serialize proof to pass it to the client
            let proof_bytes = merkle_proof.to_bytes();

            // Parse proof back on the client
            let proof = MerkleProof::<Sha256>::try_from(proof_bytes).unwrap();

            assert!(proof.verify(
                merkle_root,
                &indices_to_prove,
                leaves_to_prove,
                leaves.len()
            ));
        }
    }
}
