use rusty_coin::blockchain::blockchain::Blockchain;
use rusty_coin::network::node::Node;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Initialize the logger
    // env_logger::init();
    // Initialize the logger with default level set to "info"
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Initialize the blockchain
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));

    // Create a node
    let node = Node::new(blockchain.clone());

    // Start the node
    node.start("0.0.0.0:8333").await.unwrap();

    // Optionally, you can connect to other peers
    // TODO: this is not working - need to handle node -> node connection
    // node.connect_to_peer("127.0.0.1:8334").await.unwrap();

    // The node runs indefinitely
}
