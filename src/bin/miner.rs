// src/bin/miner.rs

use log::info;
use rusty_coin::blockchain::blockchain::Blockchain;
use rusty_coin::miner::miner_loop;
use rusty_coin::network::node::Node;
use rusty_coin::utils::double_sha256;
use secp256k1::rand::rngs::OsRng;
use secp256k1::Secp256k1;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();

    // Attempt to load the blockchain from a file
    let blockchain = if Path::new("blockchain.dat").exists() {
        match Blockchain::load_from_file("blockchain.dat") {
            Ok(bc) => {
                info!("Blockchain loaded from file.");
                bc
            }
            Err(e) => {
                info!(
                    "Failed to load blockchain: {}. Initializing new blockchain.",
                    e
                );
                Blockchain::new()
            }
        }
    } else {
        info!("No blockchain file found. Initializing new blockchain.");
        Blockchain::new()
    };

    let blockchain = Arc::new(Mutex::new(blockchain));

    // Create a node
    let node = Node::new(blockchain.clone());

    // Generate miner's keypair
    let secp = Secp256k1::new();
    let mut rng = OsRng::default();
    let (miner_private_key, miner_public_key) = secp.generate_keypair(&mut rng);

    // Compute the public key hash
    let miner_pubkey_bytes = miner_public_key.serialize();
    let miner_pubkey_hash = double_sha256(&miner_pubkey_bytes);

    // Start the node
    let node_clone = node.clone();
    tokio::spawn(async move {
        node_clone.start("0.0.0.0:8334").await.unwrap();
    });

    // Connect to other peers (e.g., the main node)
    node.connect_to_peer("127.0.0.1:8333").await.unwrap();

    // Start mining
    miner_loop(blockchain.clone(), node.peers.clone(), miner_pubkey_hash).await;

    // Before exiting, save the blockchain
    let blockchain_guard = blockchain.lock().await;
    if let Err(e) = blockchain_guard.save_to_file("blockchain.dat") {
        println!("Failed to save blockchain: {}", e);
    } else {
        println!("Blockchain saved to file.");
    }
}
