// src/blockchain/proof_of_work.rs

use crate::blockchain::block::{Block, BlockHeader};
use crate::blockchain::transaction::{Transaction, TransactionOutput};
// use crate::utils::double_sha256;
use num_bigint::BigUint;
// use num_traits::Zero;
use std::time::{SystemTime, UNIX_EPOCH};

/// Miner struct that contains mining-related configurations.
pub struct Miner {
    /// The difficulty target (compact representation).
    pub bits: u32,
    /// The public key hash of the miner, used for the coinbase transaction.
    pub miner_pubkey_hash: Vec<u8>,
}

impl Miner {
    /// Creates a new miner with the specified difficulty bits.
    pub fn new(bits: u32, miner_pubkey_hash: Vec<u8>) -> Self {
        Miner {
            bits,
            miner_pubkey_hash,
        }
    }

    /// Mines a block by finding a valid nonce.
    pub fn mine_block(&self, previous_block: &Block, transactions: Vec<Transaction>) -> Block {
        // Create the coinbase transaction
        let coinbase_tx = create_coinbase_transaction(self.miner_pubkey_hash.clone(), 50_000_000);

        // Include the coinbase transaction
        let mut block_transactions = vec![coinbase_tx];
        block_transactions.extend(transactions);

        // Create the block with the transactions including the coinbase
        let mut block = Block {
            header: BlockHeader {
                version: 1,
                previous_hash: previous_block.hash(),
                merkle_root: vec![], // Will be computed below
                timestamp: current_timestamp(),
                difficulty_target: self.bits,
                nonce: 0,
            },
            transactions: block_transactions,
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
                block.header.nonce = block.header.nonce.wrapping_add(1);
                if block.header.nonce == 0 {
                    // If nonce overflows, update timestamp and recompute the Merkle root
                    block.header.timestamp = current_timestamp();
                    block.header.merkle_root = block.compute_merkle_root();
                    // .expect("Failed to compute Merkle root");
                }
            }
        }

        block
    }
}

/// Helper function to create a coinbase transaction
fn create_coinbase_transaction(recipient_pubkey_hash: Vec<u8>, amount: u64) -> Transaction {
    // Coinbase transactions have no inputs
    let inputs = vec![];

    // The output sends the reward to the miner
    let outputs = vec![TransactionOutput {
        amount,
        script_pub_key: recipient_pubkey_hash,
    }];

    let mut tx = Transaction::new(inputs, outputs);
    tx.txid = Some(tx.hash());
    tx
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
