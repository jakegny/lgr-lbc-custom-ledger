use bincode::serialize;
use log::info;
use rusty_coin::network::Message;
use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

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
                info!("Blockchain: {:?}", blockchain);
            }
        }
        Err(e) => {
            eprintln!("Failed to read from node: {:?}", e);
        }
    }
}
