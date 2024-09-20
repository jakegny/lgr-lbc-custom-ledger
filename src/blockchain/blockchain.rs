use crate::blockchain::block::{Block, BlockHeader};
use crate::blockchain::transaction::{Transaction, TransactionOutput};
use crate::utils::double_sha256;
use num_bigint::BigUint;
use secp256k1::ecdsa::Signature;
use std::collections::{HashMap, HashSet};

/// Represents the blockchain, containing a list of blocks.
pub struct Blockchain {
    /// Vector of blocks in the chain.
    pub blocks: Vec<Block>,
    /// UTXO set: maps outpoints to transaction outputs.
    pub utxo_set: HashMap<OutPoint, TransactionOutput>,
}

impl Blockchain {
    /// Initializes a new blockchain with the genesis block.
    pub fn new() -> Self {
        // Create the coinbase transaction for the genesis block
        let genesis_coinbase_tx = Self::create_genesis_coinbase_transaction();

        // Create the genesis block with the coinbase transaction
        let genesis_block = Block::new(
            vec![genesis_coinbase_tx],
            vec![0u8; 32], // Previous hash is all zeros for the genesis block
            0x1d00ffff,    // Difficulty bits for Bitcoin genesis block
        );
        let mut blockchain = Blockchain {
            blocks: vec![genesis_block.clone()],
            utxo_set: HashMap::new(),
        };

        // Process the UTXO set for the genesis block (if any transactions)
        blockchain.update_utxo_set(&genesis_block);

        blockchain
    }

    // Helper function to create a coinbase transaction
    fn create_genesis_coinbase_transaction() -> Transaction {
        // Create a dummy public key hash for the genesis block
        let recipient_pubkey_hash = vec![0u8; 32]; // This can be any value for the genesis block

        // Coinbase transactions have no inputs
        let inputs = vec![];

        // The output sends the reward to the recipient
        let outputs = vec![TransactionOutput {
            amount: 50_000_000, // For example, reward of 50 coins
            script_pub_key: recipient_pubkey_hash,
        }];

        Transaction::new(inputs, outputs)
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
    fn validate_block(&self, block: &Block) -> Result<(), String> {
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
    fn validate_transactions(&self, transactions: &[Transaction]) -> Result<(), String> {
        let mut temp_utxo_set = self.utxo_set.clone();
        let mut spent_outpoints = HashSet::new();

        for tx in transactions {
            // Skip coinbase transaction validation for simplicity
            if tx.inputs.is_empty() {
                continue;
            }

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
    fn validate_transaction(
        &self,
        tx: &Transaction,
        utxo_set: &HashMap<OutPoint, TransactionOutput>,
    ) -> Result<(), String> {
        // Check transaction inputs
        let mut total_input_value = 0u64;
        let mut total_output_value = 0u64;

        for (i, input) in tx.inputs.iter().enumerate() {
            let outpoint = OutPoint {
                txid: input.previous_output_hash.clone(),
                index: input.previous_output_index,
            };

            let prev_output = utxo_set
                .get(&outpoint)
                .ok_or("UTXO not found".to_string())?;

            // Validate signature
            let secp = secp256k1::Secp256k1::new();
            let public_key = extract_public_key(&input.script_sig)?;
            let pubkey_hash = double_sha256(&public_key.serialize());

            if &pubkey_hash[..] != prev_output.script_pub_key.as_slice() {
                return Err("Public key hash does not match script_pub_key".to_string());
            }

            // Prepare the transaction hash for signature verification
            let sighash =
                tx.signature_hash(i, &prev_output.script_pub_key, SigHashType::All as u32)?;

            let msg = secp256k1::Message::from_digest_slice(&sighash).map_err(|e| e.to_string())?;

            let signature = extract_signature(&input.script_sig)?;

            // Verify the signature
            if secp.verify_ecdsa(&msg, &signature, &public_key).is_err() {
                return Err("Invalid transaction signature".to_string());
            }

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
        // let hash = sha256d::Hash::hash(&serialized_header);
        let hash = double_sha256(&serialized_header);
        let hash_value = BigUint::from_bytes_be(&hash);

        hash_value <= target
    }
}

/// Represents an outpoint, referencing a specific output in a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OutPoint {
    /// Transaction ID.
    pub txid: Vec<u8>,
    /// Output index.
    pub index: u32,
}

/// Converts a compact representation of the difficulty target to a BigUint.
fn compact_to_target(bits: u32) -> num_bigint::BigUint {
    use num_bigint::BigUint;

    let exponent = ((bits >> 24) & 0xFF) as u32;
    let mantissa = bits & 0x007FFFFF;

    if exponent <= 3 {
        BigUint::from(mantissa >> (8 * (3 - exponent)))
    } else {
        BigUint::from(mantissa) << (8 * (exponent - 3))
    }
}

/// Extracts the public key from the scriptSig.
fn extract_public_key(script_sig: &[u8]) -> Result<secp256k1::PublicKey, String> {
    if script_sig.len() < 1 {
        return Err("scriptSig too short to contain public key".to_string());
    }
    let sig_len = script_sig[0] as usize;
    if script_sig.len() < sig_len + 1 + 1 {
        return Err("scriptSig too short to contain public key".to_string());
    }
    let pubkey_len = script_sig[sig_len + 1] as usize;
    let pubkey_start = sig_len + 2;
    let pubkey_end = pubkey_start + pubkey_len;
    if script_sig.len() < pubkey_end {
        return Err("scriptSig too short to contain full public key".to_string());
    }
    let pubkey_bytes = &script_sig[pubkey_start..pubkey_end];
    secp256k1::PublicKey::from_slice(pubkey_bytes).map_err(|e| e.to_string())
}

/// Extracts the signature from the scriptSig.
fn extract_signature(script_sig: &[u8]) -> Result<Signature, String> {
    if script_sig.len() < 1 {
        return Err("scriptSig too short to contain signature".to_string());
    }
    let sig_len = script_sig[0] as usize;
    if script_sig.len() < sig_len + 1 {
        return Err("scriptSig too short to contain full signature".to_string());
    }
    let sig_bytes = &script_sig[1..(sig_len + 1)];
    let mut sig_bytes_without_sighash = sig_bytes.to_vec();
    // Remove the sighash type byte
    sig_bytes_without_sighash.pop();
    Signature::from_der(&sig_bytes_without_sighash).map_err(|e| e.to_string())
}

/// SIGHASH types for transaction signing
#[repr(u32)]
pub enum SigHashType {
    All = 1,
    // Other SIGHASH types can be added here
}
