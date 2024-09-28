use rusty_coin::{
    mining::{proof_of_work::Miner, worker_node::WorkerNode},
    utils::DIFFICULTY,
};

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "info");
    // Initialize env_logger with custom format
    // Builder::new()
    //     .format(|buf, record| writeln!(buf, "[{}] - {}", record.level(), record.args()))
    //     .filter(None, log::LevelFilter::Info) // Capture logs of level Info or higher
    //     .init();
    env_logger::init();

    // Create miner
    let miner_pubkey_hash = vec![1, 2, 3, 4]; // Replace with actual pubkey hash
    let difficulty_bits = DIFFICULTY;
    let miner = Miner::new(difficulty_bits, miner_pubkey_hash);
    let worker_node = WorkerNode::new(miner, Some(0));

    // Start mining
    worker_node.start(vec!["127.0.0.1:8000".to_string()]);
}
