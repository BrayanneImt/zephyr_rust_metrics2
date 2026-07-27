#!/usr/bin/env bash
# Compile le module en WebAssembly et affiche sa taille.
set -e
cd "$(dirname "$0")"

rustup target add wasm32-unknown-unknown 2>/dev/null || true

echo "Compilation wasm32-unknown-unknown (release)..."
cargo build --release --target wasm32-unknown-unknown

WASM=target/wasm32-unknown-unknown/release/zephyr_rust_metrics.wasm
if [ -f "$WASM" ]; then
    SIZE=$(stat -c%s "$WASM" 2>/dev/null || stat -f%z "$WASM")
    echo "OK : $WASM ($SIZE octets)"
    cp "$WASM" ./metrics.wasm
    echo "Copie : ./metrics.wasm"
else
    echo "ERREUR : binaire .wasm introuvable"
    exit 1
fi
