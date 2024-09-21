use serde::{Deserialize, Serialize};

use crate::blockchain::{Block, Transaction};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Version { version: u32, best_height: u64 },
    GetBlocks,
    Inv { items: Vec<InventoryItem> },
    GetData { items: Vec<InventoryItem> },
    Block(Block),
    Transaction(Transaction),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InventoryItem {
    Block(Vec<u8>),       // Block hash
    Transaction(Vec<u8>), // Transaction ID
}
