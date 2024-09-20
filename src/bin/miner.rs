use rusty_coin::blockchain::{
    blockchain::OutPoint, Blockchain, Transaction, TransactionInput, TransactionOutput,
};
use rusty_coin::mining::Miner;
use rusty_coin::utils::double_sha256;
use secp256k1::{rand, Secp256k1, SecretKey};
use std::collections::HashMap;

fn main() {
    // Initialize the blockchain
    let mut blockchain = Blockchain::new();

    // Create a miner with a lower difficulty for testing
    let miner = Miner::new(0x1e0fffff);

    // Generate keys for two users
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    // User 1 (miner)
    let (miner_private_key, miner_public_key) = secp.generate_keypair(&mut rng);
    let miner_pubkey_bytes = miner_public_key.serialize();
    let miner_pubkey_hash = double_sha256(&miner_pubkey_bytes);

    // User 2
    let (user_private_key, user_public_key) = secp.generate_keypair(&mut rng);
    let user_pubkey_bytes = user_public_key.serialize();
    let user_pubkey_hash = double_sha256(&user_pubkey_bytes);

    // Create a coinbase transaction to pay the miner (User 1)
    let coinbase_tx = create_coinbase_transaction(miner_pubkey_hash.clone(), 50_000_000);
    let transactions = vec![coinbase_tx.clone()];

    // Get the previous block (genesis block)
    let previous_block = blockchain.blocks.last().unwrap();
    println!("{:?}", previous_block);

    // Mine the first block with the coinbase transaction
    let new_block = miner.mine_block(previous_block, transactions);

    // Validate and add the new block to the blockchain
    blockchain
        .add_block(new_block)
        .expect("Failed to add block to blockchain");

    println!(
        "Mined first block. Blockchain now has {} blocks",
        blockchain.blocks.len()
    );

    // Create a transaction from Miner (User 1) to User 2
    let tx = create_and_sign_transaction(
        &secp,
        &miner_private_key,
        &miner_pubkey_hash,
        &user_pubkey_hash,
        &blockchain.utxo_set,
        50_000_000, // Amount to transfer
    )
    .expect("Failed to create and sign transaction");

    // Mine a new block with the transaction
    let transactions = vec![tx];
    let previous_block = blockchain.blocks.last().unwrap();
    let new_block = miner.mine_block(previous_block, transactions);

    // Validate and add the new block to the blockchain
    blockchain
        .add_block(new_block)
        .expect("Failed to add block to blockchain");

    println!(
        "Mined second block. Blockchain now has {} blocks",
        blockchain.blocks.len()
    );
}

fn create_coinbase_transaction(recipient_pubkey_hash: Vec<u8>, amount: u64) -> Transaction {
    // Coinbase transactions have no inputs
    let inputs = vec![];

    // The output sends the reward to the miner
    let outputs = vec![TransactionOutput {
        amount,
        script_pub_key: recipient_pubkey_hash,
    }];

    Transaction::new(inputs, outputs)
}

fn create_and_sign_transaction(
    secp: &Secp256k1<secp256k1::All>,
    sender_private_key: &SecretKey,
    sender_pubkey_hash: &Vec<u8>,
    recipient_pubkey_hash: &Vec<u8>,
    utxo_set: &HashMap<OutPoint, TransactionOutput>,
    amount: u64,
) -> Result<Transaction, String> {
    // Find UTXOs owned by the sender
    let mut accumulated = 0u64;
    let mut inputs = vec![];
    let mut used_outpoints = vec![];

    for (outpoint, output) in utxo_set.iter() {
        if &output.script_pub_key == sender_pubkey_hash {
            accumulated += output.amount;
            inputs.push(TransactionInput {
                previous_output_hash: outpoint.txid.clone(),
                previous_output_index: outpoint.index,
                script_sig: vec![], // Will be filled after signing
            });
            used_outpoints.push(outpoint.clone());

            if accumulated >= amount {
                break;
            }
        }
    }

    if accumulated < amount {
        return Err("Not enough funds".to_string());
    }

    // Create outputs
    let mut outputs = vec![];

    // Output to recipient
    outputs.push(TransactionOutput {
        amount,
        script_pub_key: recipient_pubkey_hash.clone(),
    });

    // Change back to sender if any
    if accumulated > amount {
        outputs.push(TransactionOutput {
            amount: accumulated - amount,
            script_pub_key: sender_pubkey_hash.clone(),
        });
    }

    // Create the transaction
    let mut tx = Transaction::new(inputs, outputs);

    // Closure to get previous output for signing
    let get_previous_output = |txid: &[u8], index: u32| -> Option<TransactionOutput> {
        let outpoint = OutPoint {
            txid: txid.to_vec(),
            index,
        };
        utxo_set.get(&outpoint).cloned()
    };

    // Sign the transaction inputs
    tx.sign_inputs(sender_private_key, get_previous_output)?;

    Ok(tx)
}
