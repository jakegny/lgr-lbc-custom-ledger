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
use log::info;

/// WorkerNode struct that represents a node in the network.
pub struct WorkerNode {
    /// The miner instance used for mining blocks.
    miner: Miner,
    /// A thread-safe mempool for storing pending transactions.
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    /// A thread-safe blockchain containing all mined blocks.
    blockchain: Arc<Mutex<Blockchain>>,
    /// A list of peer addresses for network communication.
    peers: Arc<Mutex<HashSet<String>>>,
    /// The address this node is listening on.
    address: String,
    /// Listener for incoming connections.
    // listener: TcpListener,
    // /// Set of connected peers (both incoming and outgoing).
    // connections: Arc<Mutex<HashMap<String, TcpStream>>>,
}

impl WorkerNode {
    /// Creates a new WorkerNode with the given miner and peers.
    pub fn new(miner: Miner, peers: Vec<String>) -> Self {
        let peers_set = peers.into_iter().collect::<HashSet<_>>();
        WorkerNode {
            miner,
            mempool: Arc::new(Mutex::new(VecDeque::new())),
            blockchain: Arc::new(Mutex::new(Blockchain::new())),
            peers: Arc::new(Mutex::new(peers_set)),
            address: String::new(),
        }
    }

    /// Starts the worker node by initiating network listening and mining.
    pub fn start(&self, origin_port: Option<u32>) {
        let mempool = Arc::clone(&self.mempool);
        let blockchain = Arc::clone(&self.blockchain);
        let peers = Arc::clone(&self.peers);

        // Start the network listener in a separate thread.
        let port = origin_port.unwrap_or(0);
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .expect("Failed to bind to an available port");
        info!("Node started and listening on port {}", port);
        let address = self.address.clone();
        // self.listener = listener.try_clone().expect("Failed to clone listener");

        // Spawn thread to accept incoming connections
        let listener_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let mempool = Arc::clone(&mempool);
                        let blockchain = Arc::clone(&blockchain);
                        let peers = peers.clone();
                        let address = address.clone();
                        thread::spawn(move || {
                            handle_client(stream, mempool, blockchain, peers, address);
                        });
                    }
                    Err(e) => {
                        info!("Connection failed: {}", e);
                    }
                }
            }
        });
        // Start connecting to initial peers
        self.connect_to_initial_peers();

        // Start heartbeat thread
        self.start_heartbeat();

        // Start mining in the main thread.
        self.mine_loop();

        // // Wait for the listener thread to finish (it won't, but in case you add shutdown logic)
        listener_thread.join().unwrap();
    }

    /// Connects to initial peers and requests peer information.
    fn connect_to_initial_peers(&self) {
        info!("Connecting to initial peers...");
        let peers_list = {
            let peers = self.peers.lock().unwrap();
            peers.clone()
        };
        info!("peers_list: {:?}", peers_list);
        for peer in peers_list {
            let peer_address = peer.clone();
            let mempool_clone = Arc::clone(&self.mempool);
            let blockchain_clone = Arc::clone(&self.blockchain);
            let peers_clone = Arc::clone(&self.peers);
            let self_address = self.address.clone();

            thread::spawn(move || {
                connect_to_peer(
                    peer_address,
                    mempool_clone,
                    blockchain_clone,
                    peers_clone,
                    self_address,
                );
            });
        }
    }

    fn accept_connections(
        listener: TcpListener,
        connections: Arc<Mutex<HashMap<String, TcpStream>>>,
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
                    thread::spawn(move || {
                        Self::handle_connection(stream_clone, peer_address, connections_clone);
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
        let peers = Arc::clone(&self.peers);
        let self_address = self.address.clone();
        thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_secs(30)); // Ping interval
            let peers_list = {
                let peers = peers.lock().unwrap();
                peers.clone()
            };
            for peer in peers_list {
                let peer_address = peer.clone();
                let peers_clone = Arc::clone(&peers);
                let self_address = self_address.clone();
                thread::spawn(move || {
                    if let Ok(mut stream) = TcpStream::connect(&peer_address) {
                        let ping = Message::Ping;
                        let data = serialize(&ping).expect("Failed to serialize Ping");
                        if let Err(e) = stream.write_all(&data) {
                            info!("Failed to send Ping to {}: {}", peer_address, e);
                            // Remove unresponsive peer
                            peers_clone.lock().unwrap().remove(&peer_address);
                        }
                    } else {
                        info!(
                            "Failed to connect to peer {}: connection lost",
                            peer_address
                        );
                        // Remove unresponsive peer
                        peers_clone.lock().unwrap().remove(&peer_address);
                    }
                });
            }
        });
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

            // Validate and add the new block to the blockchain.
            {
                let mut blockchain = self.blockchain.lock().unwrap();
                if blockchain.add_block(new_block.clone()).is_ok() {
                    info!("New block mined and added to blockchain!");
                    // Update mempool
                    self.update_mempool(&new_block);
                    // Broadcast the new block to peers.
                    self.broadcast_block(new_block);
                } else {
                    info!("Failed to add mined block to blockchain.");
                    // Re-add transactions back to the mempool
                    let mut mempool = self.mempool.lock().unwrap();
                    mempool.extend(transactions);
                }
            }
        }
    }

    /// Broadcasts a transaction to all connected peers.
    fn broadcast_transaction(&self, transaction: Transaction) {
        let peers_list = {
            let peers = self.peers.lock().unwrap();
            peers.clone()
        };
        for peer in peers_list {
            info!("broadcast_transaction: {:?}", peer);
            if peer == self.address {
                continue;
            }
            match TcpStream::connect(&peer) {
                Ok(mut stream) => {
                    // Serialize the transaction and send it.
                    let message = Message::Transaction(transaction.clone());
                    let data = serialize(&message).expect("Failed to serialize transaction");
                    if let Err(e) = stream.write_all(&data) {
                        info!("Failed to send transaction to {}: {}", peer, e);
                    }
                }
                Err(e) => {
                    info!("Could not connect to peer {}: {}", peer, e);
                    // Optionally remove unresponsive peer
                    self.peers.lock().unwrap().remove(&peer);
                }
            }
        }
    }

    /// Broadcasts a block to all connected peers.
    fn broadcast_block(&self, block: Block) {
        let peers_list = {
            let peers = self.peers.lock().unwrap();
            peers.clone()
        };
        for peer in peers_list {
            if peer == self.address {
                continue;
            }
            match TcpStream::connect(&peer) {
                Ok(mut stream) => {
                    // Serialize the block and send it.
                    let message = Message::Block(block.clone());
                    let data = serialize(&message).expect("Failed to serialize block");
                    if let Err(e) = stream.write_all(&data) {
                        info!("Failed to send block to {}: {}", peer, e);
                    }
                }
                Err(e) => {
                    info!("Could not connect to peer {}: {}", peer, e);
                    // Optionally remove unresponsive peer
                    self.peers.lock().unwrap().remove(&peer);
                }
            }
        }
    }

    /// Updates the mempool by removing transactions that have been included in a new block.
    fn update_mempool(&self, block: &Block) {
        let mut mempool = self.mempool.lock().unwrap();
        let block_txids: HashSet<Vec<u8>> = block.transactions.iter().map(|tx| tx.hash()).collect();

        mempool.retain(|tx| !block_txids.contains(&tx.hash()));
    }

    fn handle_connection(
        mut stream: TcpStream,
        peer_address: String,
        connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    ) {
        let mut buffer = [0u8; 4096];

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
                            Self::handle_message(msg, &peer_address, &connections);
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message from {}: {}", peer_address, e);
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

    /// Connects to known peers.
    fn connect_to_peers(&self) {
        let peers_list = {
            let peers = self.peers.lock().unwrap();
            peers.clone()
        };
        for peer_address in peers_list {
            if peer_address == self.address {
                continue;
            }
            let connections_clone = Arc::clone(&self.connections);
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
                        Self::handle_connection(stream_clone, peer_address, connections_clone);
                    });
                } else {
                    eprintln!("Failed to connect to peer {}", peer_address);
                }
            });
        }
    }

    fn send_message_to_peer(
        peer_address: &String,
        message: &Message,
        connections: &Arc<Mutex<HashMap<String, TcpStream>>>,
    ) {
        let connections = connections.lock().unwrap();
        if let Some(stream) = connections.get(peer_address) {
            let mut stream = stream.try_clone().expect("Failed to clone stream");
            let data = serialize(message).expect("Failed to serialize message");
            if let Err(e) = stream.write_all(&data) {
                eprintln!("Failed to send message to {}: {}", peer_address, e);
            }
        } else {
            eprintln!("No connection to peer {}", peer_address);
        }
    }

    fn broadcast_message(
        message: &Message,
        connections: &Arc<Mutex<HashMap<String, TcpStream>>>,
        self_address: &String,
    ) {
        let peers_list = {
            let connections = connections.lock().unwrap();
            connections.keys().cloned().collect::<Vec<String>>()
        };
        for peer in peers_list {
            if &peer == self_address {
                continue;
            }
            Self::send_message_to_peer(&peer, message, connections);
        }
    }

    fn handle_message(
        &self,
        message: Message,
        peer_address: &String,
        connections: &Arc<Mutex<HashMap<String, TcpStream>>>,
    ) {
        let mempool = Arc::clone(&self.mempool);
        let blockchain = Arc::clone(&self.blockchain);
        let peers = Arc::clone(&self.peers);
        let self_address = self.address.clone();
        match message {
            Message::Transaction(transaction) => {
                info!("--Transaction-- transaction: {:?}", transaction);

                // Add the transaction to the mempool.
                {
                    let mut mempool = mempool.lock().unwrap();
                    mempool.push_back(transaction.clone());
                }
                info!("Transaction added to mempool from {}", peer_address);

                // Broadcast the transaction to peers.
                let peers_list = {
                    let peers = peers.lock().unwrap();
                    peers.clone()
                };
                for peer in peers_list {
                    if peer == self_address || peer == peer_address {
                        continue;
                    }
                    if let Ok(mut peer_stream) = TcpStream::connect(&peer) {
                        let data = serialize(&Message::BroadcastTransaction(transaction.clone()))
                            .expect("Failed to serialize transaction");
                        if let Err(e) = peer_stream.write_all(&data) {
                            info!("Failed to send transaction to {}: {}", peer, e);
                        }
                    }
                }
            }
            Message::BroadcastTransaction(transaction) => {
                info!("--Transaction-- transaction: {:?}", transaction);

                // Add the transaction to the mempool.
                {
                    let mut mempool = mempool.lock().unwrap();
                    mempool.push_back(transaction.clone());
                }
                info!("Transaction added to mempool from {}", peer_address);
            }
            Message::Block(block) => {
                info!("--Block-- block: {:?}", block);
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
                // Send the list of known peers.
                info!("--RequestPeers-- requestor: {:?}", peer_address);
                let mut peers = peers.lock().unwrap();

                // notify others of a new peer
                let peers_list = { peers.iter().cloned().collect::<Vec<String>>() };
                let response = Message::Peers(peers_list);

                // add to peers list
                let inserted = peers.insert(peer_address.clone());
                if inserted {
                    info!("added peer: {:?}", peer_address);
                }

                let data = serialize(&response).expect("Failed to serialize peers list");
                if let Err(e) = stream.write_all(&data) {
                    info!("Failed to send peers list: {}", e);
                }
            }
            Message::Peers(received_peers) => {
                // Add the received peers to the peer list.
                info!("--Peers-- Received peers: {:?}", received_peers);
                let mut peers_set = peers.lock().unwrap();
                for peer in received_peers {
                    if peer != self_address && peers_set.insert(peer.clone()) {
                        // New peer added; attempt to connect.
                        let mempool_clone = Arc::clone(&mempool);
                        let blockchain_clone = Arc::clone(&blockchain);
                        let peers_clone = Arc::clone(&peers);
                        let self_address_clone = self_address.clone();
                        thread::spawn(move || {
                            connect_to_peer(
                                peer.clone(),
                                mempool_clone,
                                blockchain_clone,
                                peers_clone,
                                self_address_clone,
                            );
                        });
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
                // if blocks.len() > blockchain.blocks.len() {
                //     blockchain.blocks = blocks;
                //     info!(
                //         "Blockchain updated with received data from {}",
                //         peer_address
                //     );
                // }
            }
            Err(e) => {
                info!("Failed to deserialize message from {}: {}", peer_address, e);
            }
        }
    }
}

/// Handles incoming client connections and processes their messages.
fn handle_client(
    mut stream: TcpStream,
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<HashSet<String>>>,
    self_address: String,
) {
    let peer_address = stream.peer_addr().unwrap().to_string();
    let mut buffer = [0; 4096];
    while let Ok(size) = stream.read(&mut buffer) {
        if size == 0 {
            break;
        }

        // Deserialize the incoming data.
        let message: Result<Message, _> = deserialize(&buffer[..size]);
        match message {
            Ok(Message::Transaction(transaction)) => {
                info!("--Transaction-- transaction: {:?}", transaction);

                // Add the transaction to the mempool.
                {
                    let mut mempool = mempool.lock().unwrap();
                    mempool.push_back(transaction.clone());
                }
                info!("Transaction added to mempool from {}", peer_address);

                // Broadcast the transaction to peers.
                let peers_list = {
                    let peers = peers.lock().unwrap();
                    peers.clone()
                };
                for peer in peers_list {
                    if peer == self_address || peer == peer_address {
                        continue;
                    }
                    if let Ok(mut peer_stream) = TcpStream::connect(&peer) {
                        let data = serialize(&Message::BroadcastTransaction(transaction.clone()))
                            .expect("Failed to serialize transaction");
                        if let Err(e) = peer_stream.write_all(&data) {
                            info!("Failed to send transaction to {}: {}", peer, e);
                        }
                    }
                }
            }
            Ok(Message::BroadcastTransaction(transaction)) => {
                info!("--Transaction-- transaction: {:?}", transaction);

                // Add the transaction to the mempool.
                {
                    let mut mempool = mempool.lock().unwrap();
                    mempool.push_back(transaction.clone());
                }
                info!("Transaction added to mempool from {}", peer_address);
            }
            Ok(Message::Block(block)) => {
                info!("--Block-- block: {:?}", block);
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
            Ok(Message::RequestPeers) => {
                // Send the list of known peers.
                info!("--RequestPeers-- requestor: {:?}", peer_address);
                let mut peers = peers.lock().unwrap();

                // notify others of a new peer
                let peers_list = { peers.iter().cloned().collect::<Vec<String>>() };
                let response = Message::Peers(peers_list);

                // add to peers list
                let inserted = peers.insert(peer_address.clone());
                if inserted {
                    info!("added peer: {:?}", peer_address);
                }

                let data = serialize(&response).expect("Failed to serialize peers list");
                if let Err(e) = stream.write_all(&data) {
                    info!("Failed to send peers list: {}", e);
                }
            }
            Ok(Message::Peers(received_peers)) => {
                // Add the received peers to the peer list.
                info!("--Peers-- Received peers: {:?}", received_peers);
                let mut peers_set = peers.lock().unwrap();
                for peer in received_peers {
                    if peer != self_address && peers_set.insert(peer.clone()) {
                        // New peer added; attempt to connect.
                        let mempool_clone = Arc::clone(&mempool);
                        let blockchain_clone = Arc::clone(&blockchain);
                        let peers_clone = Arc::clone(&peers);
                        let self_address_clone = self_address.clone();
                        thread::spawn(move || {
                            connect_to_peer(
                                peer.clone(),
                                mempool_clone,
                                blockchain_clone,
                                peers_clone,
                                self_address_clone,
                            );
                        });
                    }
                }
            }
            Ok(Message::Ping) => {
                // Respond with Pong
                info!("--Ping-- from {}", peer_address);
                let pong = Message::Pong;
                let data = serialize(&pong).expect("Failed to serialize Pong");
                if let Err(e) = stream.write_all(&data) {
                    info!("Failed to send Pong: {}", e);
                }
            }
            Ok(Message::Pong) => {
                // Received Pong, peer is alive; do nothing
            }
            Ok(Message::GetBlockchain) => {
                // Send the blockchain data to the requester.
                let blockchain = blockchain.lock().unwrap();
                let data = serialize(&Message::BlockchainData(blockchain.clone()))
                    .expect("Failed to serialize blockchain data");
                if let Err(e) = stream.write_all(&data) {
                    info!("Failed to send blockchain data: {}", e);
                }
            }
            Ok(Message::BlockchainData(blocks)) => {
                // Update the local blockchain if necessary.
                let mut blockchain = blockchain.lock().unwrap();
                // if blocks.len() > blockchain.blocks.len() {
                //     blockchain.blocks = blocks;
                //     info!(
                //         "Blockchain updated with received data from {}",
                //         peer_address
                //     );
                // }
            }
            Err(e) => {
                info!("Failed to deserialize message from {}: {}", peer_address, e);
            }
        }
    }
    // Connection closed or error occurred
    info!("Connection to {} closed", peer_address);
    peers.lock().unwrap().remove(&peer_address);
}

/// Connects to a peer and handles communication.
fn connect_to_peer(
    peer_address: String,
    mempool: Arc<Mutex<VecDeque<Transaction>>>,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<HashSet<String>>>,
    self_address: String,
) {
    if let Ok(mut stream) = TcpStream::connect(&peer_address) {
        info!("Connected to new peer: {}", peer_address);
        // Send RequestPeers message
        let request = Message::RequestPeers;
        let data = serialize(&request).expect("Failed to serialize RequestPeers");
        if let Err(e) = stream.write_all(&data) {
            info!("Failed to send RequestPeers to {}: {}", peer_address, e);
        }

        // Send our own address to the peer
        let data = serialize(&Message::Peers(vec![self_address.clone()]))
            .expect("Failed to serialize Peers message");
        info!("Sending self address to peer: {}", self_address);
        if let Err(e) = stream.write_all(&data) {
            info!("Failed to send self address to {}: {}", peer_address, e);
        }

        handle_client(stream, mempool, blockchain, peers, self_address);
    } else {
        info!("Failed to connect to peer: {}", peer_address);
        // Optionally remove the peer from the list if connection fails.
        peers.lock().unwrap().remove(&peer_address);
    }
}
