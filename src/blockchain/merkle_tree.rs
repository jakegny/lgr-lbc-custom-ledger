use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Computes a double SHA-256 hash.
fn double_sha256(data: &[u8]) -> Vec<u8> {
    let hash1 = Sha256::digest(data);
    let hash2 = Sha256::digest(&hash1);
    hash2.to_vec()
}

/// Represents a Merkle Tree used for hashing transactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    /// The Merkle root of the tree.
    root: Vec<u8>,
    /// The list of transaction hashes (leaf nodes).
    transactions: Vec<Vec<u8>>,
}

impl MerkleTree {
    /// Constructs a new Merkle Tree from a list of transaction hashes.
    pub fn new(transactions: &[Vec<u8>]) -> Result<Self, String> {
        if transactions.is_empty() {
            return Err("Cannot create a Merkle Tree with no transactions".to_string());
        }

        let root = MerkleTree::compute_root(transactions)?;

        Ok(MerkleTree {
            root,
            transactions: transactions.to_vec(),
        })
    }

    /// Returns the Merkle root of the tree.
    pub fn root(&self) -> Vec<u8> {
        self.root.clone()
    }

    /// Computes the Merkle root from a list of transaction hashes.
    fn compute_root(hashes: &[Vec<u8>]) -> Result<Vec<u8>, String> {
        let mut current_level = hashes.to_vec();

        while current_level.len() > 1 {
            let mut next_level = Vec::new();

            for i in (0..current_level.len()).step_by(2) {
                let left = &current_level[i];
                let right = if i + 1 < current_level.len() {
                    &current_level[i + 1]
                } else {
                    // Duplicate the last hash if the number of hashes is odd
                    &current_level[i]
                };

                let combined = [left.as_slice(), right.as_slice()].concat();
                let hash = double_sha256(&combined);
                next_level.push(hash);
            }

            current_level = next_level;
        }

        Ok(current_level[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn double_sha256(data: &[u8]) -> Vec<u8> {
        let hash1 = Sha256::digest(data);
        let hash2 = Sha256::digest(&hash1);
        hash2.to_vec()
    }

    #[test]
    fn test_merkle_tree_single_transaction() {
        let tx_hash = double_sha256(b"transaction1");
        let merkle_tree =
            MerkleTree::new(&[tx_hash.clone()]).expect("Failed to create Merkle Tree");
        assert_eq!(merkle_tree.root(), tx_hash);
    }

    #[test]
    fn test_merkle_tree_multiple_transactions() {
        let tx_hashes = vec![
            double_sha256(b"tx1"),
            double_sha256(b"tx2"),
            double_sha256(b"tx3"),
            double_sha256(b"tx4"),
        ];
        let merkle_tree = MerkleTree::new(&tx_hashes).expect("Failed to create Merkle Tree");
        assert!(!merkle_tree.root().is_empty());
    }

    #[test]
    fn test_merkle_tree_odd_number_of_transactions() {
        let tx_hashes = vec![
            double_sha256(b"tx1"),
            double_sha256(b"tx2"),
            double_sha256(b"tx3"),
        ];
        let merkle_tree = MerkleTree::new(&tx_hashes).expect("Failed to create Merkle Tree");
        assert!(!merkle_tree.root().is_empty());
    }

    #[test]
    #[should_panic(expected = "Cannot create a Merkle Tree with no transactions")]
    fn test_merkle_tree_empty_transactions() {
        let tx_hashes: Vec<Vec<u8>> = Vec::new();
        MerkleTree::new(&tx_hashes).unwrap();
    }
}
