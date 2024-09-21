use crate::blockchain::blockchain::Blockchain;
use crate::network::message::{InventoryItem, Message};
use log::{error, info};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Peer {
    pub stream: Arc<Mutex<TcpStream>>,
    pub address: String,
}

#[derive(Clone)]
pub struct Node {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub peers: Arc<Mutex<Vec<Peer>>>,
}

impl Node {
    pub fn new(blockchain: Arc<Mutex<Blockchain>>) -> Self {
        Node {
            blockchain,
            peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn start(&self, bind_addr: &str) -> tokio::io::Result<()> {
        let listener = TcpListener::bind(bind_addr).await?;
        info!("Node listening on {}", bind_addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            info!("New connection from {}", addr);

            let peer = Peer {
                stream: Arc::new(Mutex::new(stream)),
                address: addr.to_string(),
            };

            let blockchain = Arc::clone(&self.blockchain);
            let peers = Arc::clone(&self.peers);

            tokio::spawn(async move {
                handle_connection(peer, blockchain, peers).await;
            });
        }
    }

    pub async fn connect_to_peer(&self, addr: &str) -> tokio::io::Result<()> {
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to peer {}", addr);
        println!("Connected to peer {}", addr);

        let peer = Peer {
            stream: Arc::new(Mutex::new(stream)),
            address: addr.to_string(),
        };

        let blockchain = Arc::clone(&self.blockchain);
        let peers = Arc::clone(&self.peers);

        tokio::spawn(async move {
            handle_connection(peer, blockchain, peers).await;
        });

        Ok(())
    }
}

async fn handle_connection(
    peer: Peer,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<Vec<Peer>>>,
) {
    // Add peer to the list
    {
        let mut peers_guard = peers.lock().await;
        peers_guard.push(peer.clone());
        println!("Added peer {}", peer.address);
    }

    let mut buffer = vec![0u8; 1024];

    loop {
        let n = {
            let mut stream_guard = peer.stream.lock().await;
            match stream_guard.read(&mut buffer).await {
                Ok(0) => {
                    info!("Connection closed by {}", peer.address);
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    error!("Error reading from {}: {}", peer.address, e);
                    break;
                }
            }
        }; // MutexGuard is dropped here

        // Deserialize the message
        let message: Message = match bincode::deserialize(&buffer[..n]) {
            Ok(msg) => msg,
            Err(e) => {
                error!("Failed to deserialize message from {}: {}", peer.address, e);
                continue;
            }
        };

        // Handle the message
        handle_message(message, &peer, &blockchain, &peers).await;
    }

    // Remove peer from the list
    {
        let mut peers_guard = peers.lock().await;
        peers_guard.retain(|p| p.address != peer.address);
    }
}

async fn handle_message(
    message: Message,
    peer: &Peer,
    blockchain: &Arc<Mutex<Blockchain>>,
    peers: &Arc<Mutex<Vec<Peer>>>,
) {
    match message {
        Message::Version {
            version,
            best_height,
        } => {
            // Respond with own version and blockchain height
            let blockchain_guard = blockchain.lock().await;
            let response = Message::Version {
                version: 1,
                best_height: blockchain_guard.blocks.len() as u64,
            };
            send_message(peer, response).await;
        }
        Message::GetBlocks => {
            // Send block headers
            let blockchain_guard = blockchain.lock().await;
            let block_hashes: Vec<Vec<u8>> =
                blockchain_guard.blocks.iter().map(|b| b.hash()).collect();
            let inv = Message::Inv {
                items: block_hashes.into_iter().map(InventoryItem::Block).collect(),
            };
            send_message(peer, inv).await;
        }
        Message::Inv { items } => {
            // Request missing blocks or transactions
            let blockchain_guard = blockchain.lock().await;
            let mut missing = vec![];
            for item in items {
                match item {
                    InventoryItem::Block(hash) => {
                        if !blockchain_guard.blocks.iter().any(|b| b.hash() == hash) {
                            missing.push(InventoryItem::Block(hash));
                        }
                    }
                    InventoryItem::Transaction(_txid) => {
                        // Handle transactions if needed
                    }
                }
            }
            if !missing.is_empty() {
                let get_data = Message::GetData { items: missing };
                send_message(peer, get_data).await;
            }
        }
        Message::GetData { items } => {
            // Send requested blocks or transactions
            let blockchain_guard = blockchain.lock().await;
            for item in items {
                match item {
                    InventoryItem::Block(hash) => {
                        if let Some(block) =
                            blockchain_guard.blocks.iter().find(|b| b.hash() == hash)
                        {
                            let block_message = Message::Block(block.clone());
                            send_message(peer, block_message).await;
                        }
                    }
                    InventoryItem::Transaction(_txid) => {
                        // Handle transactions if needed
                    }
                }
            }
        }
        Message::Block(block) => {
            // Validate and add the block
            info!("Received block from {}", peer.address);
            let mut blockchain_guard = blockchain.lock().await;
            if blockchain_guard.add_block(block.clone()).is_ok() {
                info!("Added new block from {}", peer.address);
                // Broadcast the new block to other peers
                broadcast_message(
                    peers,
                    Message::Inv {
                        items: vec![InventoryItem::Block(block.hash())],
                    },
                )
                .await;
            }
        }
        Message::Transaction(_tx) => {
            // Handle incoming transaction
            // You can add it to a mempool and include it in the next mined block
        }
    }
}

async fn send_message(peer: &Peer, message: Message) {
    let bytes = bincode::serialize(&message).expect("Failed to serialize message");
    println!("Sending message to {}. Bytes: {:?}", peer.address, bytes);
    let mut stream_guard = peer.stream.lock().await;

    if let Err(e) = stream_guard.write_all(&bytes).await {
        println!("Failed to send message to {}: {:?}", peer.address, e);
        error!("Failed to send message to {}: {:?}", peer.address, e);
        // Optionally handle disconnection
        return;
    }

    println!("Sent message to {}", peer.address);
    // MutexGuard is dropped here
}

pub async fn broadcast_message(peers: &Arc<Mutex<Vec<Peer>>>, message: Message) {
    let peers_guard = peers.lock().await;
    let peers_clone = peers_guard.clone(); // Clone the peers list
    drop(peers_guard); // Drop the lock before awaiting
    println!("Broadcasting message to all peers");

    for peer in peers_clone.iter() {
        let peer_clone = peer.clone();
        println!("Sending message to {}", peer_clone.address);
        send_message(&peer_clone, message.clone()).await;
    }
}
