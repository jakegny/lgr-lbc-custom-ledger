use crate::blockchain::merkle_tree::MerkleTree;
use crate::blockchain::transaction::Transaction;
use crate::utils::double_sha256;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_hash: Vec<u8>,
    pub merkle_root: Vec<u8>,
    pub timestamp: u64,
    pub nonce: u64,
    pub difficulty_target: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    /// Computes the hash of the block header.
    pub fn hash(&self) -> Vec<u8> {
        let serialized_header =
            bincode::serialize(&self.header).expect("Failed to serialize block header");
        // let hash = sha256d::Hash::hash(&serialized_header);
        // hash.to_vec()
        double_sha256(&serialized_header)
    }

    /// Creates a new block.
    pub fn new(
        transactions: Vec<Transaction>,
        previous_hash: Vec<u8>,
        difficulty_target: u32,
    ) -> Self {
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                previous_hash,
                merkle_root: vec![],
                // hack to make the timestamp work
                timestamp: 1727127200, // Self::current_timestamp(),
                difficulty_target,
                nonce: 0,
            },
            transactions,
        };
        block.header.merkle_root = block.compute_merkle_root();
        block
    }

    /// Computes the Merkle root of the transactions.
    pub fn compute_merkle_root(&self) -> Vec<u8> {
        let tx_hashes: Vec<Vec<u8>> = self.transactions.iter().map(|tx| tx.hash()).collect();
        let merkle_tree = MerkleTree::new(&tx_hashes).expect("Failed to create Merkle Tree");
        merkle_tree.root()
    }
}
