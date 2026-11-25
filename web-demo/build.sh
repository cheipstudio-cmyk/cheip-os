#!/bin/sh
# Demo web di Cheip OS: la stessa shell compilata in WebAssembly, con dati finti.
# Serve il target wasm32 (rustup target add wasm32-unknown-unknown) e wasm-bindgen-cli.
set -eu
cd "$(dirname "$0")/../shell"
cargo build --lib --release --target wasm32-unknown-unknown --no-default-features --features web
wasm-bindgen --target web --no-typescript --out-dir ../web-demo/pkg target/wasm32-unknown-unknown/release/cheip_shell.wasm
echo "Pronto: servi la cartella web-demo con un server statico"
