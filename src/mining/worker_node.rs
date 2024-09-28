use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::blockchain::block::Block;
use crate::blockchain::blockchain::Blockchain;
use crate::blockchain::transaction::Transaction;
use crate::mining::proof_of_work::Miner;
use crate::network::Message;
use bincode::{deserialize, serialize};
use log::{debug, error, info};

/// WorkerNode struct that represents a node in the network.
pub struct WorkerNode {
    /// The miner instance used for mining blocks.
    miner: Miner,
    /// A thread-safe mempool for storing pending transactions.
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    /// A thread-safe blockchain containing all mined blocks.
    blockchain: Arc<Mutex<Blockchain>>,
    /// The address this node is listening on.
    address: String,
    /// Listener for incoming connections.
    listener: TcpListener,
    /// Set of connected peers (both incoming and outgoing).
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
}

impl WorkerNode {
    /// Creates a new WorkerNode with the given miner and peers.
    pub fn new(miner: Miner, _port: Option<u32>) -> Self {
        let port = _port.unwrap_or(0);
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .expect("Failed to bind to an available port");

        WorkerNode {
            miner,
            mempool: Arc::new(Mutex::new(VecDeque::new())),
            blockchain: Arc::new(Mutex::new(Blockchain::new())),
            address: String::new(),
            listener,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts the worker node by initiating network listening and mining.
    pub fn start(&self, peers: Vec<String>) {
        let listener_clone = self.listener.try_clone().expect("Failed to clone listener");
        let connections_clone = Arc::clone(&self.connections);
        let mempool_clone = Arc::clone(&self.mempool);
        let blockchain_clone = Arc::clone(&self.blockchain);
        let address_clone = self.address.clone();
        thread::spawn(move || {
            WorkerNode::accept_connections(
                listener_clone,
                connections_clone,
                mempool_clone,
                blockchain_clone,
                address_clone,
            );
        });

        // // Start connecting to initial peers
        // self.connect_to_initial_peers();
        self.connect_to_peers(peers);

        // Start heartbeat thread
        self.start_heartbeat();

        // Start mining in the main thread.
        self.mine_loop();

        // // Wait for the listener thread to finish (it won't, but in case you add shutdown logic)
        // listener_thread.join().unwrap();
    }

    fn accept_connections(
        listener: TcpListener,
        connections: Arc<Mutex<HashMap<String, TcpStream>>>,
        mempool: Arc<Mutex<VecDeque<Transaction>>>,
        blockchain: Arc<Mutex<Blockchain>>,
        address: String,
    ) {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer_address = stream.peer_addr().unwrap().to_string();
                    println!("Accepted connection from {}", peer_address);

                    // Add the connection to the connections map
                    connections
                        .lock()
                        .unwrap()
                        .insert(peer_address.clone(), stream.try_clone().unwrap());

                    // Start a thread to handle communication with this peer
                    let stream_clone = stream.try_clone().unwrap();
                    let connections_clone = Arc::clone(&connections);
                    let mempool_clone = mempool.clone();
                    let blockchain_clone = blockchain.clone();
                    let address_clone = address.clone();
                    thread::spawn(move || {
                        handle_connection(
                            stream_clone,
                            peer_address,
                            connections_clone,
                            mempool_clone,
                            blockchain_clone,
                            address_clone,
                        );
                    });
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Starts the heartbeat mechanism.
    fn start_heartbeat(&self) {
        // let peers = Arc::clone(&self.peers);
        // thread::spawn(move || loop {
        //     thread::sleep(std::time::Duration::from_secs(30)); // Ping interval
        //     let peers_list = {
        //         let peers = peers.lock().unwrap();
        //         peers.clone()
        //     };
        //     for peer in peers_list {
        //         let peer_address = peer.clone();
        //         let peers_clone = Arc::clone(&peers);
        //         // let self_address = self_address.clone();
        //         thread::spawn(move || {
        //             if let Ok(mut stream) = TcpStream::connect(&peer_address) {
        //                 let ping = Message::Ping;
        //                 let data = serialize(&ping).expect("Failed to serialize Ping");
        //                 if let Err(e) = stream.write_all(&data) {
        //                     error!("Failed to send Ping to {}: {}", peer_address, e);
        //                     // Remove unresponsive peer
        //                     peers_clone.lock().unwrap().remove(&peer_address);
        //                 }
        //             } else {
        //                 error!(
        //                     "Failed to connect to peer {}: connection lost",
        //                     peer_address
        //                 );
        //                 // Remove unresponsive peer
        //                 peers_clone.lock().unwrap().remove(&peer_address);
        //             }
        //         });
        //     }
        // });
    }

    /// The main mining loop that continuously mines new blocks.
    fn mine_loop(&self) {
        loop {
            let previous_block = {
                let blockchain = self.blockchain.lock().unwrap();
                blockchain.blocks.last().cloned()
            };

            let previous_block = match previous_block {
                Some(block) => block,
                None => {
                    // Create a genesis block if the blockchain is empty.
                    Blockchain::genesis_block()
                }
            };

            // Get transactions from the mempool.
            let transactions = {
                let mut mempool = self.mempool.lock().unwrap();
                if mempool.is_empty() {
                    drop(mempool); // Release lock before sleeping
                    info!("No transactions in mempool. Waiting...");
                    thread::sleep(std::time::Duration::from_secs(5));
                    continue;
                }
                mempool.drain(..).collect::<Vec<_>>()
            };

            // Mine a new block with the transactions.
            let new_block = self.miner.mine_block(&previous_block, transactions.clone());

            broadcast_message(
                Message::NewBlock(new_block.clone()),
                self.connections.clone(),
                self.address.clone(),
            )
        }
    }

    /// Updates the mempool by removing transactions that have been included in a new block.
    fn update_mempool(&self, block: &Block) {
        let mut mempool = self.mempool.lock().unwrap();
        let block_txids: HashSet<Vec<u8>> = block.transactions.iter().map(|tx| tx.hash()).collect();

        mempool.retain(|tx| !block_txids.contains(&tx.hash()));
    }

    /// Connects to known peers.
    fn connect_to_peers(&self, peers: Vec<String>) {
        for peer_address in peers {
            if peer_address == self.address {
                continue;
            }
            let connections_clone = Arc::clone(&self.connections);
            let mempool_clone = self.mempool.clone();
            let blockchain_clone = self.blockchain.clone();
            let address_clone = self.address.clone();
            thread::spawn(move || {
                if let Ok(stream) = TcpStream::connect(&peer_address) {
                    println!("Connected to peer {}", peer_address);
                    // Add the connection to the connections map
                    connections_clone
                        .lock()
                        .unwrap()
                        .insert(peer_address.clone(), stream.try_clone().unwrap());

                    // Start a thread to handle communication with this peer
                    let stream_clone = stream.try_clone().unwrap();

                    thread::spawn(move || {
                        handle_connection(
                            stream_clone,
                            peer_address,
                            connections_clone,
                            mempool_clone,
                            blockchain_clone,
                            address_clone,
                        );
                    });
                } else {
                    eprintln!("Failed to connect to peer {}", peer_address);
                }
            });
        }
    }
}

fn broadcast_message(
    message: Message,
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    self_address: String,
) {
    let connections_map = {
        let connections = connections.lock().unwrap();
        connections
    };
    for (peer, mut stream) in connections_map.iter() {
        if peer == &self_address {
            continue;
        }
        // Serialize and send the message.
        let data = serialize(&message).expect("Failed to serialize message");
        if let Err(e) = stream.write_all(&data) {
            error!("Failed to broadcast message to {}: {}", peer, e);
        }
    }
}

fn handle_message(
    message: Message,
    peer_address: &String,
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    blockchain: Arc<Mutex<Blockchain>>,
    self_address: String,
    mut stream: TcpStream,
) {
    match message {
        Message::Transaction(transaction) => {
            debug!("--Transaction-- transaction: {:?}", transaction);

            // Add the transaction to the mempool.
            {
                let mut mempool = mempool.lock().unwrap();
                mempool.push_back(transaction.clone());
            }
            info!("Transaction added to mempool from {}", peer_address);

            broadcast_message(
                Message::BroadcastTransaction(transaction.clone()),
                connections.clone(),
                self_address.clone(),
            );
        }
        Message::BroadcastTransaction(transaction) => {
            debug!("--BroadcastTransaction-- transaction: {:?}", transaction);

            // Add the transaction to the mempool.
            {
                let mut mempool = mempool.lock().unwrap();
                // Check if the transaction is already in the mempool to avoid duplicates.
                if !mempool.iter().any(|tx| tx.hash() == transaction.hash()) {
                    mempool.push_back(transaction.clone());
                    info!("Transaction added to mempool from {}", peer_address);

                    broadcast_message(
                        Message::BroadcastTransaction(transaction.clone()),
                        connections.clone(),
                        self_address.clone(),
                    );
                } else {
                    info!("Transaction already in mempool, not adding again.");
                }
            }
        }
        Message::NewBlock(block) => {
            debug!("--NewBlock-- block: {:?}", block);
            // Validate and add the block to the blockchain.
            let mut blockchain = blockchain.lock().unwrap();
            if blockchain.add_block(block.clone()).is_ok() {
                info!("Block added to blockchain from {}", peer_address);
                // Update mempool
                let block_txids: HashSet<Vec<u8>> =
                    block.transactions.iter().map(|tx| tx.hash()).collect();
                let mut mempool = mempool.lock().unwrap();
                mempool.retain(|tx| !block_txids.contains(&tx.hash()));

                broadcast_message(
                    Message::Block(block.clone()),
                    connections.clone(),
                    self_address.clone(),
                );
            } else {
                info!("Received invalid block from {}", peer_address);
            }
        }
        Message::Block(block) => {
            debug!("--Block-- block: {:?}", block);
            // Validate and add the block to the blockchain.
            let mut blockchain = blockchain.lock().unwrap();
            if blockchain.add_block(block.clone()).is_ok() {
                info!("Block added to blockchain from {}", peer_address);
                // Update mempool
                let block_txids: HashSet<Vec<u8>> =
                    block.transactions.iter().map(|tx| tx.hash()).collect();
                let mut mempool = mempool.lock().unwrap();
                mempool.retain(|tx| !block_txids.contains(&tx.hash()));
            } else {
                info!("Received invalid block from {}", peer_address);
            }
        }
        Message::RequestPeers => {
            // Respond with the list of known peers.
            let connections = connections.lock().unwrap();
            let peers: Vec<String> = connections.keys().cloned().collect();
            let data = serialize(&Message::Peers(peers)).expect("Failed to serialize Peers");
            if let Err(e) = stream.write_all(&data) {
                info!("Failed to send Peers: {}", e);
            }
        }
        Message::Peers(received_peers) => {
            // Add the received peers to the peer list.
            info!("--Peers-- Received peers: {:?}", received_peers);
            {
                let connections_map = connections.lock().unwrap();
                for peer in received_peers {
                    // Skip if the peer is the node itself or is already in the peer list
                    if peer != self_address && !connections_map.contains_key(&peer) {
                        let connections_clone = Arc::clone(&connections);
                        let mempool_clone = Arc::clone(&mempool);
                        let blockchain_clone = Arc::clone(&blockchain);
                        let self_address_clone = self_address.clone();
                        thread::spawn(move || {
                            connect_to_peer(
                                peer.clone(),
                                connections_clone,
                                mempool_clone,
                                blockchain_clone,
                                self_address_clone,
                            );
                        });
                    }
                }
            }
        }
        Message::Ping => {
            // Respond with Pong
            info!("--Ping-- from {}", peer_address);
            let pong = Message::Pong;
            let data = serialize(&pong).expect("Failed to serialize Pong");
            if let Err(e) = stream.write_all(&data) {
                info!("Failed to send Pong: {}", e);
            }
        }
        Message::Pong => {
            // Received Pong, peer is alive; do nothing
            info!("--Pong-- from {}", peer_address);
        }
        Message::GetBlockchain => {
            // Send the blockchain data to the requester.
            let blockchain = blockchain.lock().unwrap();
            let data = serialize(&Message::BlockchainData(blockchain.clone()))
                .expect("Failed to serialize blockchain data");
            if let Err(e) = stream.write_all(&data) {
                info!("Failed to send blockchain data: {}", e);
            }
        }
        Message::BlockchainData(blocks) => {
            // Update the local blockchain if necessary.
            let mut blockchain = blockchain.lock().unwrap();
            if blocks.blocks.len() > blockchain.blocks.len() {
                blockchain.blocks = blocks.blocks;
                info!(
                    "Blockchain updated with received data from {}",
                    peer_address
                );
            }
        }
    }
}

/// Connects to a peer and handles communication.
fn connect_to_peer(
    peer_address: String,
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    blockchain: Arc<Mutex<Blockchain>>,
    self_address: String,
) {
    let connections_clone = connections.clone();
    let mempool_clone = mempool.clone();
    let blockchain_clone = blockchain.clone();
    let address_clone = self_address.clone();
    thread::spawn(move || {
        if let Ok(stream) = TcpStream::connect(&peer_address) {
            println!("Connected to peer {}", peer_address);
            // Add the connection to the connections map
            connections_clone
                .lock()
                .unwrap()
                .insert(peer_address.clone(), stream.try_clone().unwrap());

            // Start a thread to handle communication with this peer
            let stream_clone = stream.try_clone().unwrap();

            thread::spawn(move || {
                handle_connection(
                    stream_clone,
                    peer_address,
                    connections_clone,
                    mempool_clone,
                    blockchain_clone,
                    address_clone,
                );
            });
        } else {
            eprintln!("Failed to connect to peer {}", peer_address);
        }
    });
}

fn handle_connection(
    mut stream: TcpStream,
    peer_address: String,
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    blockchain: Arc<Mutex<Blockchain>>,
    self_address: String,
) {
    let mut buffer = [0u8; 8192];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                // Connection was closed by the peer
                println!("Connection closed by {}", peer_address);
                connections.lock().unwrap().remove(&peer_address);
                break;
            }
            Ok(size) => {
                // Deserialize and handle the message
                let message: Result<Message, _> = deserialize(&buffer[..size]);

                match message {
                    Ok(msg) => {
                        // Handle the received message
                        let stream_clone = stream.try_clone().expect("Failed to clone stream");
                        handle_message(
                            msg,
                            &peer_address,
                            connections.clone(),
                            mempool.clone(),
                            blockchain.clone(),
                            self_address.clone(),
                            stream_clone,
                        );
                    }
                    Err(e) => {
                        // eprintln!("Failed to deserialize message from {}: {}", peer_address, e);
                        panic!("Failed to deserialize message from {}: {}", peer_address, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read from {}: {}", peer_address, e);
                connections.lock().unwrap().remove(&peer_address);
                break;
            }
        }
    }
}
