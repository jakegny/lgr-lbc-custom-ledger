use crate::blockchain::block::{Block, BlockHeader};
use crate::blockchain::transaction::{Transaction, TransactionOutput};
use crate::utils::{double_sha256, load_key_pair, save_key_pair, DIFFICULTY};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Represents an outpoint, referencing a specific output in a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutPoint {
    /// Transaction ID.
    pub txid: Vec<u8>,
    /// Output index.
    pub index: u32,
}

/// Represents the blockchain, containing a list of blocks.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Blockchain {
    /// Vector of blocks in the chain.
    pub blocks: Vec<Block>,
    /// UTXO set: maps outpoints to transaction outputs.
    pub utxo_set: HashMap<OutPoint, TransactionOutput>,
}

impl Blockchain {
    /// Initializes a new blockchain with the genesis block.
    pub fn new() -> Self {
        let genesis_block = Blockchain::genesis_block();

        // Initialize the blockchain with the genesis block
        let mut blockchain = Blockchain {
            blocks: vec![genesis_block.clone()],
            utxo_set: HashMap::new(),
        };

        // Update the UTXO set with the genesis block
        blockchain.update_utxo_set(&genesis_block);

        blockchain
    }

    // Helper function to create a coinbase transaction
    fn create_genesis_coinbase_transaction() -> Transaction {
        // Load or generate key pair as before
        let sender_keypair_path = "sender_keypair.txt";
        let (_, sender_pk) = if Path::new(sender_keypair_path).exists() {
            load_key_pair(sender_keypair_path).expect("Failed to load sender's key pair")
        } else {
            let secp = Secp256k1::new();
            let mut rng = OsRng::default();
            let sender_sk = SecretKey::new(&mut rng);
            let sender_pk = PublicKey::from_secret_key(&secp, &sender_sk);
            save_key_pair(&sender_sk, &sender_pk, sender_keypair_path)
                .expect("Failed to save sender's key pair");
            (sender_sk, sender_pk)
        };
        let recipient_pubkey_bytes = sender_pk.serialize();
        let recipient_pubkey_hash = double_sha256(&recipient_pubkey_bytes); // vec![0u8; 32]; // This can be any value for the genesis block

        // Coinbase transactions have no inputs
        let inputs = vec![];

        // The output sends the reward to the recipient
        let outputs = vec![TransactionOutput {
            amount: 50_000_000, // For example, reward of 50 coins
            script_pub_key: recipient_pubkey_hash,
        }];

        Transaction::new(inputs, outputs)
    }

    pub fn get_utxo_set(&self) -> HashMap<OutPoint, TransactionOutput> {
        self.utxo_set.clone()
    }

    pub fn genesis_block() -> Block {
        // Define the genesis block parameters
        let genesis_previous_hash = vec![
            172, 45, 210, 88, 101, 36, 190, 13, 90, 67, 181, 144, 233, 50, 78, 222, 111, 12, 132,
            221, 177, 35, 158, 66, 209, 173, 92, 117, 36, 190, 207, 21,
        ];
        let genesis_difficulty_target = DIFFICULTY; //0x1f00ffff; // 0x1d00ffff; // Bitcoin's initial difficulty target

        // Create the coinbase transaction for the genesis block
        let genesis_coinbase_tx = Blockchain::create_genesis_coinbase_transaction();

        // Create the genesis block with the coinbase transaction
        let genesis_block = Block::new(
            vec![genesis_coinbase_tx],
            genesis_previous_hash,
            genesis_difficulty_target,
        );

        genesis_block
    }

    /// Adds a block to the blockchain after validation.
    pub fn add_block(&mut self, block: Block) -> Result<(), String> {
        // Validate the block
        self.validate_block(&block)?;

        // Add block to the chain
        self.blocks.push(block.clone());

        // Update the UTXO set
        self.update_utxo_set(&block);

        Ok(())
    }

    /// Validates a block before adding it to the blockchain.
    pub fn validate_block(&self, block: &Block) -> Result<(), String> {
        // Check previous block hash
        let last_block = self.blocks.last().unwrap();
        if block.header.previous_hash != last_block.hash() {
            return Err("Invalid previous block hash".to_string());
        }

        // Verify proof-of-work
        if !self.verify_proof_of_work(&block.header) {
            return Err("Invalid proof-of-work".to_string());
        }

        // Verify Merkle root
        let merkle_root = block.compute_merkle_root();
        if block.header.merkle_root != merkle_root {
            return Err("Invalid Merkle root".to_string());
        }

        // Validate transactions
        self.validate_transactions(&block.transactions)?;

        Ok(())
    }

    /// Validates the transactions in a block.
    pub fn validate_transactions(&self, transactions: &[Transaction]) -> Result<(), String> {
        let mut temp_utxo_set = self.utxo_set.clone();
        let mut spent_outpoints = HashSet::new();

        for tx in transactions {
            // Validate each transaction
            self.validate_transaction(tx, &temp_utxo_set)?;

            // Update temp UTXO set
            for input in &tx.inputs {
                let outpoint = OutPoint {
                    txid: input.previous_output_hash.clone(),
                    index: input.previous_output_index,
                };

                if !temp_utxo_set.contains_key(&outpoint) {
                    return Err("Attempted to spend a non-existent output".to_string());
                }

                if !spent_outpoints.insert(outpoint.clone()) {
                    return Err("Double spending detected in block".to_string());
                }

                temp_utxo_set.remove(&outpoint);
            }

            // Add new outputs to temp UTXO set
            let txid = tx.hash();
            for (index, output) in tx.outputs.iter().enumerate() {
                let outpoint = OutPoint {
                    txid: txid.clone(),
                    index: index as u32,
                };
                temp_utxo_set.insert(outpoint, output.clone());
            }
        }

        Ok(())
    }

    /// Validates a single transaction.
    pub fn validate_transaction(
        &self,
        tx: &Transaction,
        utxo_set: &HashMap<OutPoint, TransactionOutput>,
    ) -> Result<(), String> {
        // Skip coinbase transaction validation for simplicity
        if tx.inputs.is_empty() {
            return Ok(());
        }

        // Define closure to get previous output
        let get_previous_output = |txid: &[u8], index: u32| -> Option<TransactionOutput> {
            let outpoint = OutPoint {
                txid: txid.to_vec(),
                index,
            };
            utxo_set.get(&outpoint).cloned()
        };

        // Verify the transaction inputs using the transactions.rs method
        if !tx.verify_inputs(get_previous_output) {
            return Err("Invalid transaction signature".to_string());
        }

        // Check transaction inputs and outputs amounts
        let mut total_input_value = 0u64;
        let mut total_output_value = 0u64;

        for input in &tx.inputs {
            let outpoint = OutPoint {
                txid: input.previous_output_hash.clone(),
                index: input.previous_output_index,
            };

            let prev_output = utxo_set
                .get(&outpoint)
                .ok_or("UTXO not found".to_string())?;

            total_input_value = total_input_value
                .checked_add(prev_output.amount)
                .ok_or("Input value overflow")?;
        }

        // Check transaction outputs
        for output in &tx.outputs {
            total_output_value = total_output_value
                .checked_add(output.amount)
                .ok_or("Output value overflow")?;
        }

        // Ensure input value >= output value (no inflation)
        if total_input_value < total_output_value {
            return Err("Transaction outputs exceed inputs".to_string());
        }

        // TODO: Enforce minimum transaction fees if necessary

        Ok(())
    }

    /// Updates the UTXO set with the transactions from a block.
    fn update_utxo_set(&mut self, block: &Block) {
        for tx in &block.transactions {
            // Remove spent outputs
            for input in &tx.inputs {
                let outpoint = OutPoint {
                    txid: input.previous_output_hash.clone(),
                    index: input.previous_output_index,
                };
                self.utxo_set.remove(&outpoint);
            }

            // Add new outputs
            let txid = tx.hash();
            for (index, output) in tx.outputs.iter().enumerate() {
                let outpoint = OutPoint {
                    txid: txid.clone(),
                    index: index as u32,
                };
                self.utxo_set.insert(outpoint, output.clone());
            }
        }
    }

    /// Verifies the proof-of-work for a block header.
    fn verify_proof_of_work(&self, header: &BlockHeader) -> bool {
        let target = compact_to_target(header.difficulty_target);
        let serialized_header =
            bincode::serialize(header).expect("Failed to serialize block header");
        let hash = double_sha256(&serialized_header);
        let hash_value = BigUint::from_bytes_be(&hash);

        hash_value <= target
    }

    pub fn save_to_file(&self, filename: &str) -> Result<(), String> {
        let serialized = bincode::serialize(&self).map_err(|e| e.to_string())?;
        let mut file = File::create(filename).map_err(|e| e.to_string())?;
        file.write_all(&serialized).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_from_file(filename: &str) -> Result<Self, String> {
        let mut file = File::open(filename).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        let blockchain = bincode::deserialize(&buffer).map_err(|e| e.to_string())?;
        Ok(blockchain)
    }
}

/// Converts a compact representation of the difficulty target to a BigUint.
pub fn compact_to_target(bits: u32) -> num_bigint::BigUint {
    use num_bigint::BigUint;

    let exponent = ((bits >> 24) & 0xFF) as u32;
    let mantissa = bits & 0x007FFFFF;

    if exponent <= 3 {
        BigUint::from(mantissa >> (8 * (3 - exponent)))
    } else {
        BigUint::from(mantissa) << (8 * (exponent - 3))
    }
}
