use serde::{Deserialize, Serialize};

use crate::blockchain::{Block, Blockchain, Transaction};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    NewBlock(Block),
    Block(Block),
    Transaction(Transaction),
    GetBlockchain,
    BlockchainData(Blockchain),
    RequestPeers,
    Peers(Vec<String>),
    Ping,
    Pong,
    BroadcastTransaction(Transaction),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InventoryItem {
    Block(Vec<u8>),       // Block hash
    Transaction(Vec<u8>), // Transaction ID
}
