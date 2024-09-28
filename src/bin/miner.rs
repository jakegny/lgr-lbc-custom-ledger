use bincode::serialize;
use env_logger::Builder;
use log::{error, info};
use rusty_coin::{
    blockchain::Blockchain,
    mining::{proof_of_work::Miner, worker_node::WorkerNode},
    network::Message,
    utils::DIFFICULTY,
};
use std::{
    io::{Read, Write},
    net::TcpStream,
};

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "info");
    // Initialize env_logger with custom format
    Builder::new()
        .format(|buf, record| writeln!(buf, "[{}] - {}", record.level(), record.args()))
        .filter(None, log::LevelFilter::Info) // Capture logs of level Info or higher
        .init();

    // Create miner
    let miner_pubkey_hash = vec![1, 2, 3, 4]; // Replace with actual pubkey hash
    let difficulty_bits = DIFFICULTY;
    let miner = Miner::new(difficulty_bits, miner_pubkey_hash);

    let node_address = "127.0.0.1:8000";
    info!(
        "Attempting to load blockchain from node at {}",
        node_address
    );

    // Connect to the node
    let mut stream = TcpStream::connect(node_address).expect("Failed to connect to node");

    println!("Connected to node at {} to send transaction", node_address);

    // Request the blockchain to get the UTXO set
    let message = Message::GetBlockchain;
    let serialized = serialize(&message).unwrap();
    if let Err(e) = stream.write_all(&serialized) {
        error!("Failed to send RequestBlockchain: {}", e);
        return;
    }

    let mut blockchain = Blockchain::new();

    // Read the blockchain data
    let mut buffer = [0u8; 4096];
    match stream.read(&mut buffer) {
        Ok(size) => {
            let response: Message = bincode::deserialize(&buffer[..size]).unwrap();
            if let Message::BlockchainData(_blockchain) = response {
                blockchain = _blockchain;
                info!("Successfully loaded blockchain from node {}", node_address);
            }
        }
        Err(e) => {
            error!("Failed to read from node: {:?}", e);
        }
    }

    let worker_node = WorkerNode::new(miner, Some(0), Some(blockchain));

    // Start mining
    worker_node.start(vec![node_address.to_string()]);
}
