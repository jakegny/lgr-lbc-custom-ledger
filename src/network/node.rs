use crate::blockchain::block::Block;
use crate::blockchain::blockchain::Blockchain;
use crate::blockchain::transaction::Transaction;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use super::message::Message;

pub struct Node {
    /// The blockchain maintained by the node.
    pub blockchain: Arc<Mutex<Blockchain>>,
    /// The mempool of unconfirmed transactions.
    pub mempool: Arc<Mutex<Vec<Transaction>>>,
    /// List of connected peers (streams).
    pub peers: Arc<Mutex<Vec<Arc<Mutex<TcpStream>>>>>,
}

impl Node {
    pub fn new(blockchain: Arc<Mutex<Blockchain>>, mempool: Arc<Mutex<Vec<Transaction>>>) -> Self {
        Node {
            blockchain,
            mempool,
            peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn start(self, listener: TcpListener) {
        let node = Arc::new(self);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("New connection from {}", addr);

                    let node_clone = node.clone();
                    tokio::spawn(async move {
                        node_clone.handle_connection(stream).await;
                    });
                }
                Err(e) => {
                    println!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    async fn handle_connection(&self, stream: TcpStream) {
        // Wrap the stream in Arc<Mutex<>> for sharing
        let stream = Arc::new(Mutex::new(stream));

        // Add the stream to the list of peers
        {
            let mut peers = self.peers.lock().await;
            peers.push(stream.clone());
        }

        // Read data from the stream
        loop {
            let mut stream_lock = stream.lock().await;
            match read_message(&mut *stream_lock).await {
                Ok(message) => {
                    drop(stream_lock); // Release the lock before handling the message
                    self.handle_message(message, &stream).await;
                }
                Err(e) => {
                    println!("Error reading from connection: {}", e);
                    break;
                }
            }
        }

        // Remove the stream from the list of peers
        {
            let mut peers = self.peers.lock().await;
            peers.retain(|peer| !Arc::ptr_eq(peer, &stream));
        }
    }

    async fn handle_message(&self, message: Message, stream: &Arc<Mutex<TcpStream>>) {
        match message {
            Message::Transaction(tx) => {
                // Handle transaction
                self.handle_incoming_transaction(tx).await;
            }
            Message::Block(block) => {
                // Handle block
                self.handle_incoming_block(block).await;
            }
            Message::GetBlockchain => {
                // Send blockchain to requester
                let blockchain = self.blockchain.lock().await;
                let serialized = bincode::serialize(&*blockchain).unwrap();
                let _ = write_message(stream, &serialized).await;
            }
            Message::BlockchainData(_) => {
                // Update local blockchain
                // let mut local_blockchain = self.blockchain.lock().await;
                // *local_blockchain = blockchain;
            }
            Message::RequestPeers => {
                // Send list of peers to requester
                // let peers = self.peers.lock().await;
                // let peer_addresses: Vec<String> = peers
                //     .iter()
                //     .map(|peer| peer.lock().unwrap().peer_addr().unwrap().to_string())
                //     .collect();
                // let message = Message::Peers(peer_addresses);
                // let serialized = bincode::serialize(&message).unwrap();
                // let _ = write_message(stream, &serialized).await;
            }
            Message::Peers(peer_addresses) => {
                // Connect to new peers
                // for peer_address in peer_addresses {
                //     self.connect_to_peer(&peer_address).await;
                // }
            }
            _ => {
                println!("Received unhandled message: {:?}", message);
            }
            Message::Ping => {
                let message = Message::Pong;
                let serialized = bincode::serialize(&message).unwrap();
                let _ = write_message(stream, &serialized).await;
            }
            Message::Pong => {}
        }
    }

    /// Handles an incoming transaction.
    pub async fn handle_incoming_transaction(&self, tx: Transaction) {
        let blockchain = self.blockchain.lock().await;
        if blockchain
            .validate_transaction(&tx, &blockchain.utxo_set)
            .is_ok()
        {
            drop(blockchain);
            let mut mempool = self.mempool.lock().await;

            // Check if transaction is already in mempool
            if !mempool.iter().any(|t| t.hash() == tx.hash()) {
                mempool.push(tx.clone());

                // Broadcast transaction to peers
                let peers = self.peers.lock().await;
                for peer in peers.iter() {
                    let message = Message::Transaction(tx.clone());
                    let serialized = bincode::serialize(&message).unwrap();
                    let _ = write_message(peer, &serialized).await;
                }
            }
        }
    }

    /// Handles an incoming block.
    pub async fn handle_incoming_block(&self, block: Block) {
        let mut blockchain = self.blockchain.lock().await;
        if blockchain.add_block(block.clone()).is_ok() {
            // Update mempool
            self.update_mempool(&block).await;

            // Broadcast block to peers
            let peers = self.peers.lock().await;
            for peer in peers.iter() {
                let message = Message::Block(block.clone());
                let serialized = bincode::serialize(&message).unwrap();
                let _ = write_message(peer, &serialized).await;
            }
        }
    }

    /// Updates the mempool by removing transactions that have been included in a block.
    async fn update_mempool(&self, block: &Block) {
        let mut mempool = self.mempool.lock().await;
        let block_txids: HashSet<Vec<u8>> = block.transactions.iter().map(|tx| tx.hash()).collect();

        mempool.retain(|tx| !block_txids.contains(&tx.hash()));
    }
}

/// Reads a message from the stream with proper framing.
async fn read_message(
    stream: &mut TcpStream,
) -> Result<Message, Box<dyn std::error::Error + Send + Sync>> {
    let mut length_bytes = [0u8; 4];
    stream.read_exact(&mut length_bytes).await?;
    let length = u32::from_be_bytes(length_bytes) as usize;

    let mut buffer = vec![0u8; length];
    stream.read_exact(&mut buffer).await?;

    let message: Message = bincode::deserialize(&buffer)?;
    Ok(message)
}

/// Writes a message to the stream with proper framing.
async fn write_message(
    stream: &Arc<Mutex<TcpStream>>,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let length = (data.len() as u32).to_be_bytes();

    let mut stream_lock = stream.lock().await;
    stream_lock.write_all(&length).await?;
    stream_lock.write_all(data).await?;
    Ok(())
}
