use bincode::serialize;
use rand::rngs::OsRng;
use rusty_coin::blockchain::blockchain::OutPoint;
use rusty_coin::blockchain::transaction::{Transaction, TransactionInput, TransactionOutput};
use rusty_coin::blockchain::Blockchain;
use rusty_coin::network::Message;
use rusty_coin::utils::{self, load_key_pair, save_key_pair};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;

fn main() {
    // Get the node address from command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: transaction_sender <node_address>");
        return;
    }
    let node_address = &args[1];

    // Connect to the node
    let mut stream = TcpStream::connect(node_address).expect("Failed to connect to node");

    println!("Connected to node at {} to send transaction", node_address);

    // Request the blockchain to get the UTXO set
    let message = Message::GetBlockchain;
    let serialized = serialize(&message).unwrap();
    if let Err(e) = stream.write_all(&serialized) {
        eprintln!("Failed to send RequestBlockchain: {}", e);
        return;
    }

    // Read the blockchain data
    let mut buffer = [0u8; 4096];
    match stream.read(&mut buffer) {
        Ok(size) => {
            let response: Message = bincode::deserialize(&buffer[..size]).unwrap();
            println!("Received response from node {:?}", response);
            if let Message::BlockchainData(blockchain) = response {
                println!("Received blockchain data from node: {:?}", blockchain);
                // let blockchain = Blockchain { blocks };
                let (sender_sk, sender_pk) =
                    load_key_pair("sender_keypair.txt").expect("Failed to load sender's key pair");

                // Create a test transaction
                let test_tx = create_test_transaction(&blockchain, &sender_sk, &sender_pk)
                    .expect("Failed to create test transaction");
                // Send the transaction to the node
                let message = Message::Transaction(test_tx);
                println!("Sending transaction to node at {}", node_address);
                let serialized = serialize(&message).unwrap();
                if let Err(e) = stream.write_all(&serialized) {
                    eprintln!("Failed to send transaction: {}", e);
                } else {
                    println!("Test transaction sent to node at {}", node_address);
                }
            } else {
                eprintln!("Unexpected response from node");
            }
        }
        Err(e) => {
            eprintln!("Failed to read from node: {:?}", e);
        }
    }
}

fn create_test_transaction(
    blockchain: &Blockchain,
    sender_sk: &SecretKey,
    sender_pk: &PublicKey,
) -> Option<Transaction> {
    let secp = Secp256k1::new();

    let sender_pubkey_hash = utils::double_sha256(&sender_pk.serialize());

    // Generate or load recipient's key pair
    let recipient_keypair_path = "recipient_keypair.txt";
    let (recipient_sk, recipient_pk) = if Path::new(recipient_keypair_path).exists() {
        load_key_pair(recipient_keypair_path).expect("Failed to load recipient's key pair")
    } else {
        let mut rng = OsRng::default();
        let recipient_sk = SecretKey::new(&mut rng);
        let recipient_pk = PublicKey::from_secret_key(&secp, &recipient_sk);
        save_key_pair(&recipient_sk, &recipient_pk, recipient_keypair_path)
            .expect("Failed to save recipient's key pair");
        (recipient_sk, recipient_pk)
    };
    let recipient_pubkey_hash = utils::double_sha256(&recipient_pk.serialize());

    // Get the UTXO set
    let utxo_set = blockchain.get_utxo_set();

    // Find unspent outputs belonging to the sender
    let mut input_utxos = vec![];
    let mut send_amount = 5000;

    for (outpoint, output) in utxo_set.iter() {
        if output.script_pub_key == sender_pubkey_hash {
            input_utxos.push((outpoint.clone(), output.clone()));
            // total_amount += output.amount;
            // if total_amount >= 1000 {
            //     break; // Stop when we have enough funds
            // }
        }
    }

    let fee = 1000; // Transaction fee

    // Calculate the total input value from selected UTXOs
    let total_input_value: u64 = input_utxos.iter().map(|(_, output)| output.amount).sum();
    // Calculate the change amount
    let change_amount = total_input_value - send_amount - fee;

    if input_utxos.is_empty() {
        println!("No UTXO found for the sender.");
        return None;
    }

    // Create transaction inputs
    let tx_inputs: Vec<TransactionInput> = input_utxos
        .iter()
        .map(|(outpoint, _)| {
            TransactionInput {
                previous_output_hash: outpoint.txid.clone(),
                previous_output_index: outpoint.index,
                script_sig: vec![], // Will be filled after signing
            }
        })
        .collect();

    // Create transaction outputs
    let mut tx_outputs = vec![TransactionOutput {
        amount: send_amount - fee, // Send amount minus fee
        script_pub_key: recipient_pubkey_hash,
    }];

    // Include a change output if there is change
    if change_amount > 0 {
        // Output back to the sender (change)
        tx_outputs.push(TransactionOutput {
            amount: change_amount,
            script_pub_key: sender_pubkey_hash.to_vec(),
        });
    }

    let mut tx = Transaction::new(tx_inputs, tx_outputs);

    // Sign the transaction
    let get_prev_output = |txid: &[u8], index: u32| -> Option<TransactionOutput> {
        let outpoint = OutPoint {
            txid: txid.to_vec(),
            index,
        };
        utxo_set.get(&outpoint).cloned()
    };

    tx.sign_inputs(sender_sk, get_prev_output).unwrap();

    Some(tx)
}
