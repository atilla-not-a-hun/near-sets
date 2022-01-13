#!/bin/bash
set -e
cd "`dirname $0`"
cargo build --all --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/*.wasm ./res/

# rebuild the deployer contract to ensure it has the most recent wasm bytes
cargo build -p deployer-contract --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/*.wasm ./res/
