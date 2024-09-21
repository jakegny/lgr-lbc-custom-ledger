use rusty_coin::mining::Miner;
use rusty_coin::network::message::Message;
use rusty_coin::network::node::Node;
use rusty_coin::{blockchain::Blockchain, network::node::Peer};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Initialize the blockchain
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));

    // Create a node
    let node = Node {
        blockchain: blockchain.clone(),
        peers: Arc::new(Mutex::new(vec![])),
    };

    // Start the node
    tokio::spawn(async move {
        node.start("0.0.0.0:8333").await.unwrap();
    });

    // Connect to other peers if necessary
    // e.g., node.connect_to_peer("127.0.0.1:8334").await;

    // If this node is a miner, start mining
    miner_loop(blockchain).await; //, node.peers).await;
}

async fn miner_loop(blockchain: Arc<Mutex<Blockchain>>) {
    let miner = Miner::new(0x1e0fffff);
    loop {
        // Collect transactions (e.g., from a mempool)
        let transactions = vec![]; // Replace with actual transactions

        // Get the previous block
        let previous_block = {
            let blockchain = blockchain.lock().await;
            blockchain.blocks.last().unwrap().clone()
        };

        // Mine a new block
        let new_block = miner.mine_block(&previous_block, transactions);

        // Add the block to the blockchain
        {
            let mut blockchain = blockchain.lock().await;
            blockchain
                .add_block(new_block.clone())
                .expect("Failed to add block");
        }

        // Broadcast the new block
        // broadcast_message(&peers, Message::Block(new_block)).await;
    }
}
