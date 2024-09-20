use crate::blockchain::block::{Block, BlockHeader};
use crate::blockchain::transaction::Transaction;
use num_bigint::BigUint;
use std::time::{SystemTime, UNIX_EPOCH};

/// Miner struct that contains mining-related configurations.
pub struct Miner {
    /// The difficulty target (compact representation).
    pub bits: u32,
}

impl Miner {
    /// Creates a new miner with the specified difficulty bits.
    pub fn new(bits: u32) -> Self {
        Miner { bits }
    }

    /// Mines a block by finding a valid nonce.
    pub fn mine_block(&self, previous_block: &Block, transactions: Vec<Transaction>) -> Block {
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                previous_hash: previous_block.hash(),
                merkle_root: vec![],
                timestamp: current_timestamp(),
                difficulty_target: self.bits,
                nonce: 0,
            },
            transactions,
        };

        // Compute the Merkle root and set it in the block header
        block.header.merkle_root = block.compute_merkle_root();
        // .expect("Failed to compute Merkle root");

        let target = compact_to_target(block.header.difficulty_target);

        println!("Mining new block...");
        loop {
            let hash = block.hash();
            let hash_value = BigUint::from_bytes_be(&hash);

            if hash_value <= target {
                println!("Block mined! Nonce: {}", block.header.nonce);
                println!("Block hash: {:x?}", hash);
                break;
            } else {
                block.header.nonce += 1;
                if block.header.nonce == u64::MAX {
                    // If nonce overflows, update timestamp and reset nonce
                    block.header.timestamp = current_timestamp();
                    block.header.nonce = 0;
                }
            }
        }

        block
    }
}

/// Converts a compact representation of the difficulty target to a BigUint.
fn compact_to_target(bits: u32) -> BigUint {
    let exponent = ((bits >> 24) & 0xFF) as u32;
    let mantissa = bits & 0x007FFFFF;

    let mut target = BigUint::from(mantissa);

    if exponent <= 3 {
        target >>= 8 * (3 - exponent);
    } else {
        target <<= 8 * (exponent - 3);
    }

    target
}

/// Gets the current Unix timestamp in seconds.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}
