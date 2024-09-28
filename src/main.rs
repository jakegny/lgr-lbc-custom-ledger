use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::sleep;

mod blockchain;
mod mining;
mod network;
mod utils;

use blockchain::blockchain::Blockchain;
use mining::worker_node::WorkerNode;
use network::node::Node;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Shared blockchain and mempool among nodes and miners
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));
    let mempool = Arc::new(Mutex::new(Vec::new()));

    // Define node addresses and ports
    let node_addresses = vec![
        (
            "127.0.0.1:8001".to_string(),
            vec!["127.0.0.1:8002".to_string()],
        ),
        (
            "127.0.0.1:8002".to_string(),
            vec!["127.0.0.1:8001".to_string()],
        ),
    ];

    // Start nodes
    for (address, peer_addresses) in node_addresses {
        let blockchain_clone = blockchain.clone();
        let mempool_clone = mempool.clone();
        let address_clone = address.clone();
        let peer_addresses_clone = peer_addresses.clone();

        task::spawn(async move {
            let listener = TcpListener::bind(&address_clone)
                .await
                .expect("Failed to bind to address");

            println!("Node is running on {}", address_clone);

            // Create node
            let node = Node::new(blockchain_clone, mempool_clone);

            // // Connect to peers
            // for peer_address in peer_addresses_clone {
            //     node.connect_to_peer(&peer_address).await;
            // }

            // Start node
            node.start(listener).await;
        });
    }

    // Wait a bit to ensure nodes are up
    sleep(Duration::from_secs(2)).await;

    // Start miners
    let miner_addresses = vec![
        ("127.0.0.1:8001".to_string(), vec![1, 2, 3, 4]),
        ("127.0.0.1:8002".to_string(), vec![5, 6, 7, 8]),
    ];

    for (node_address, miner_pubkey_hash) in miner_addresses {
        task::spawn(async move {
            // Connect to the node
            let stream = TcpStream::connect(&node_address)
                .await
                .expect("Failed to connect to node");

            println!("Miner connected to node at {}", node_address);

            // Create miner
            // let miner = Miner::new(stream, miner_pubkey_hash);
            // let worker_node = WorkerNode::new(stream, miner_pubkey_hash);

            // Start mining
            // worker_node.start().await;
        });
    }

    // Wait a bit to ensure miners are up
    sleep(Duration::from_secs(2)).await;

    // Send a test transaction to one of the nodes
    // send_test_transaction("127.0.0.1:8001").await;

    // Keep the main task alive
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
