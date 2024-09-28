use bincode::serialize;
use rand::rngs::OsRng;
use rusty_coin::blockchain::transaction::create_and_sign_transaction;
use rusty_coin::network::Message;
use rusty_coin::utils::{self, load_key_pair, save_key_pair};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::env;
use std::io::{Read, Write};
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
            if let Message::BlockchainData(blockchain) = response {
                let (sender_sk, sender_pk) =
                    load_key_pair("sender_keypair.txt").expect("Failed to load sender's key pair");
                let sender_pubkey_hash = utils::double_sha256(&sender_pk.serialize());

                // Generate or load recipient's key pair
                let secp = Secp256k1::new();
                let recipient_keypair_path = "recipient_keypair.txt";
                let (_, recipient_pk) = if Path::new(recipient_keypair_path).exists() {
                    load_key_pair(recipient_keypair_path)
                        .expect("Failed to load recipient's key pair")
                } else {
                    let mut rng = OsRng::default();
                    let recipient_sk = SecretKey::new(&mut rng);
                    let recipient_pk = PublicKey::from_secret_key(&secp, &recipient_sk);
                    save_key_pair(&recipient_sk, &recipient_pk, recipient_keypair_path)
                        .expect("Failed to save recipient's key pair");
                    (recipient_sk, recipient_pk)
                };
                let recipient_pubkey_hash = utils::double_sha256(&recipient_pk.serialize());

                // Create a test transaction
                let test_tx = create_and_sign_transaction(
                    &sender_sk,
                    &sender_pubkey_hash,
                    &recipient_pubkey_hash,
                    &blockchain.utxo_set,
                    5000,
                )
                .expect("Failed to create and sign transaction");

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
