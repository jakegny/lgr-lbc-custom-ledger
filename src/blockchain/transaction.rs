use crate::utils::double_sha256;
use bincode;
use log::info;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};

/// Represents an input in a transaction, referencing a previous output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInput {
    /// The hash of the previous transaction.
    pub previous_output_hash: Vec<u8>,
    /// The index of the output in the previous transaction.
    pub previous_output_index: u32,
    /// The script that satisfies the conditions placed on the output.
    pub script_sig: Vec<u8>, // We'll define a simple format
}

/// Represents an output in a transaction, specifying value and recipient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionOutput {
    /// The amount to be transferred.
    pub amount: u64,
    /// The script defining the conditions that must be met to spend this output.
    pub script_pub_key: Vec<u8>, // We'll define a simple format
}

/// Represents a transaction, containing inputs and outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// List of inputs.
    pub inputs: Vec<TransactionInput>,
    /// List of outputs.
    pub outputs: Vec<TransactionOutput>,
    /// Optional transaction ID (hash).
    // #[serde(skip_serializing, skip_deserializing)]
    pub txid: Vec<u8>,
}

impl Transaction {
    /// Creates a new transaction.
    pub fn new(inputs: Vec<TransactionInput>, outputs: Vec<TransactionOutput>) -> Self {
        let mut tx = Transaction {
            inputs,
            outputs,
            txid: vec![],
        };
        tx.txid = tx.hash();
        tx
    }

    /// Computes the transaction hash (txid).
    pub fn hash(&self) -> Vec<u8> {
        let serialized = bincode::serialize(self).expect("Failed to serialize transaction");
        double_sha256(&serialized)
    }

    /// Signs the transaction inputs.
    pub fn sign_inputs<F>(
        &mut self,
        private_key: &SecretKey,
        get_previous_output: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], u32) -> Option<TransactionOutput>,
    {
        let secp = Secp256k1::new();
        let mut temp_tx = self.clone();

        for (i, input) in self.inputs.iter_mut().enumerate() {
            // Retrieve the previous output
            let prev_output =
                get_previous_output(&input.previous_output_hash, input.previous_output_index)
                    .ok_or("Previous output not found")?;

            // Temporarily set script_sigs to empty
            for tx_input in &mut temp_tx.inputs {
                tx_input.script_sig = Vec::new();
            }

            // Set the script_sig of the current input to the script_pub_key of the previous output
            temp_tx.inputs[i].script_sig = prev_output.script_pub_key.clone();

            // Serialize and hash the transaction
            let serialized = bincode::serialize(&temp_tx).expect("Failed to serialize transaction");
            let tx_hash = double_sha256(&serialized);
            info!("Signing input {}: tx_hash = {:?}", i, tx_hash);
            info!(
                "Previous output script_pub_key: {:?}",
                prev_output.script_pub_key
            );

            let msg = Message::from_digest_slice(&tx_hash).map_err(|e| e.to_string())?;
            let sig = secp.sign_ecdsa(&msg, private_key);

            // Serialize the signature and append SIGHASH type
            let mut sig_bytes = sig.serialize_der().to_vec();
            sig_bytes.push(1); // SIGHASH_ALL

            let pubkey_bytes = PublicKey::from_secret_key(&secp, private_key)
                .serialize()
                .to_vec();

            // Construct the scriptSig
            let script_sig = build_script_sig(&sig_bytes, &pubkey_bytes);

            // Set the scriptSig in the original transaction
            input.script_sig = script_sig;
        }

        Ok(())
    }

    /// Verifies the signatures of the transaction inputs.
    pub fn verify_inputs<F>(&self, get_previous_output: F) -> bool
    where
        F: Fn(&[u8], u32) -> Option<TransactionOutput>,
    {
        let secp = Secp256k1::new();

        for (i, input) in self.inputs.iter().enumerate() {
            // Retrieve the previous output
            let prev_output =
                match get_previous_output(&input.previous_output_hash, input.previous_output_index)
                {
                    Some(output) => output,
                    None => return false,
                };

            // Extract signature and public key from script_sig
            let (sig_bytes, pubkey_bytes) = match parse_script_sig(&input.script_sig) {
                Ok((sig, pubkey)) => (sig, pubkey),
                Err(_) => return false,
            };

            let public_key = match PublicKey::from_slice(&pubkey_bytes) {
                Ok(pk) => pk,
                Err(_) => return false,
            };

            // Hash the public key to compare with the pubkey_hash
            let pubkey_hash_computed = double_sha256(&pubkey_bytes);
            if pubkey_hash_computed != prev_output.script_pub_key {
                return false;
            }

            // Reconstruct the transaction with modified script_sigs
            let mut temp_tx = self.clone();
            for tx_input in &mut temp_tx.inputs {
                tx_input.script_sig = Vec::new();
            }
            temp_tx.inputs[i].script_sig = prev_output.script_pub_key.clone();

            // Serialize and hash the transaction
            let serialized = bincode::serialize(&temp_tx).expect("Failed to serialize transaction");
            let tx_hash = double_sha256(&serialized);

            let msg = match Message::from_digest_slice(&tx_hash) {
                Ok(m) => m,
                Err(_) => return false,
            };

            let signature = match Signature::from_der(&sig_bytes[..sig_bytes.len() - 1]) {
                Ok(s) => s,
                Err(_) => return false,
            };

            if secp.verify_ecdsa(&msg, &signature, &public_key).is_err() {
                return false;
            }
        }
        true
    }

    /// Computes the hash to be signed for a given input index.
    pub fn signature_hash(
        &self,
        input_index: usize,
        prev_script_pub_key: &[u8],
        sighash_type: u32,
    ) -> Result<Vec<u8>, String> {
        // Create a copy of the transaction with modifications
        let mut tx_clone = self.clone();

        // Empty all scriptSigs
        for input in &mut tx_clone.inputs {
            input.script_sig = Vec::new();
        }

        // Replace the scriptSig of the current input with the previous output's scriptPubKey
        tx_clone.inputs[input_index].script_sig = prev_script_pub_key.to_vec();

        // Serialize the transaction
        let serialized_tx = bincode::serialize(&tx_clone).map_err(|e| e.to_string())?;

        // Append the sighash type as a 4-byte little-endian integer
        let mut data = serialized_tx;
        data.extend_from_slice(&sighash_type.to_le_bytes());

        // Double SHA-256 hash
        let hash = double_sha256(&data);

        Ok(hash)
    }
}

/// Builds a scriptSig by pushing the signature and public key onto the stack.
fn build_script_sig(sig_bytes: &[u8], pubkey_bytes: &[u8]) -> Vec<u8> {
    let mut script_sig = Vec::new();

    // Push the signature onto the stack
    script_sig.push(sig_bytes.len() as u8);
    script_sig.extend_from_slice(sig_bytes);

    // Push the public key onto the stack
    script_sig.push(pubkey_bytes.len() as u8);
    script_sig.extend_from_slice(pubkey_bytes);

    script_sig
}

/// Parses the scriptSig to extract the signature and public key.
fn parse_script_sig(script_sig: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut offset = 0;

    // Read the length of the signature
    if offset >= script_sig.len() {
        return Err("scriptSig too short to contain signature length".to_string());
    }
    let sig_len = script_sig[offset] as usize;
    offset += 1;

    // Read the signature
    if offset + sig_len > script_sig.len() {
        return Err("scriptSig too short to contain full signature".to_string());
    }
    let sig_bytes = script_sig[offset..offset + sig_len].to_vec();
    offset += sig_len;

    // Read the length of the public key
    if offset >= script_sig.len() {
        return Err("scriptSig too short to contain public key length".to_string());
    }
    let pubkey_len = script_sig[offset] as usize;
    offset += 1;

    // Read the public key
    if offset + pubkey_len > script_sig.len() {
        return Err("scriptSig too short to contain full public key".to_string());
    }
    let pubkey_bytes = script_sig[offset..offset + pubkey_len].to_vec();

    Ok((sig_bytes, pubkey_bytes))
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;

    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_verify_inputs() {
        // Simulate UTXO set
        let mut utxo_set: HashMap<(Vec<u8>, u32), TransactionOutput> = HashMap::new();

        // Generate keys
        let secp = Secp256k1::new();
        let mut rng = OsRng::default();
        let (private_key, public_key) = secp.generate_keypair(&mut rng);

        // Compute public key hash
        let pubkey_bytes = public_key.serialize();
        let pubkey_hash = double_sha256(&pubkey_bytes);

        // Previous transaction output
        let prev_txid = vec![0u8; 32]; // Placeholder txid
        let prev_output_index = 0;
        let prev_output = TransactionOutput {
            amount: 50_000_000,
            script_pub_key: pubkey_hash.clone(),
        };
        utxo_set.insert((prev_txid.clone(), prev_output_index), prev_output.clone());

        // Transaction input
        let input = TransactionInput {
            previous_output_hash: prev_txid.clone(),
            previous_output_index: prev_output_index,
            script_sig: vec![], // Will be filled after signing
        };

        // Transaction output
        let output = TransactionOutput {
            amount: 50_000_000,
            script_pub_key: pubkey_hash.clone(),
        };

        // Create transaction
        let mut tx = Transaction::new(vec![input], vec![output]);

        // Closure to get previous output
        let get_previous_output = |txid: &[u8], index: u32| -> Option<TransactionOutput> {
            utxo_set.get(&(txid.to_vec(), index)).cloned()
        };

        // Sign inputs
        tx.sign_inputs(&private_key, &get_previous_output)
            .expect("Failed to sign inputs");

        // Verify inputs
        let is_valid = tx.verify_inputs(get_previous_output);

        assert!(is_valid);
    }
}
