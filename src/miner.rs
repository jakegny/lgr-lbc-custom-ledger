use crate::blockchain::Blockchain;
use crate::mining::proof_of_work::Miner;
use crate::network::message::Message;
use crate::network::node::{broadcast_message, Peer};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn miner_loop(
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<Vec<Peer>>>,
    miner_pubkey_hash: Vec<u8>,
) {
    let miner = Miner::new(0x1e0fffff, miner_pubkey_hash);

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
        println!("Mined new block --");

        // Add the block to the blockchain
        {
            let mut blockchain = blockchain.lock().await;
            blockchain
                .add_block(new_block.clone())
                .expect("Failed to add block");
        }
        println!("Added new block to chain --");

        // Broadcast the new block
        broadcast_message(&peers, Message::Block(new_block)).await;

        // Optionally, wait for a certain time before mining the next block
        // tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
