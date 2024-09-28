# Rusty Coin

Rust Based Blockchain Learning Project

## How to run

1. Start up the origin node

```sh
RUST_LOG=trace cargo run --bin origin
```

2. Spin up any number of miner nodes

```sh
RUST_LOG=trace cargo run --bin rusty_coin_miner
```

3. send a transaction

```sh
cargo run --bin transaction_sender -- 127.0.0.1:8000
```
