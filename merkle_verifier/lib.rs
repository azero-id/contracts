#![cfg_attr(not(feature = "std"), no_std)]

#[ink::contract]
mod merkle_verifier {
    use rs_merkle::{algorithms::Sha256, Hasher, MerkleProof};

    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[ink(storage)]
    pub struct MerkleVerifier {
        /// Stores a single `bool` value on the storage.
        root: String,
    }

    impl MerkleVerifier {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new(root: String) -> Self {
            Self { root }
        }

        /// Simply returns the current value of our `bool`.
        #[ink(message)]
        pub fn verify_proof(&self, proof_bytes: Vec<u8>) -> bool {
            let proof = MerkleProof::<Sha256>::try_from(proof_bytes);
            proof.verify(self.root)
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
