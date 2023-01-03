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

        #[ink::test]
        fn root_works() {
            let root = [0xff; 32];
            let merkle_verifier = MerkleVerifier::new(root);

            assert_eq!(merkle_verifier.root(), root);
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
            let merkle_verifier = MerkleVerifier::new(root);

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
    }
}
