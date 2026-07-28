#!/usr/bin/env bash
# Compile le module en WebAssembly (cible wasm32v1-none) et affiche sa taille.
#
# On utilise wasm32v1-none (WebAssembly Core 1.0) et NON
# wasm32-unknown-unknown : cette derniere genere, avec les rustc recents, du
# WebAssembly avec reference-types que WAMR (mode MVP) refuse de charger.
# wasm32v1-none produit un binaire strictement 1.0, stdlib comprise, sur Rust
# stable. Voir .cargo/config.toml pour l'explication complete.
set -e
cd "$(dirname "$0")"

TARGET=wasm32v1-none

# Ajoute la cible si absente (sans echouer si deja presente ou hors-ligne).
rustup target add "$TARGET" 2>/dev/null || true

echo "Compilation $TARGET (release)..."
cargo build --release --target "$TARGET"

WASM="target/$TARGET/release/zephyr_rust_metrics.wasm"
if [ -f "$WASM" ]; then
    SIZE=$(stat -c%s "$WASM" 2>/dev/null || stat -f%z "$WASM")
    echo "OK : $WASM ($SIZE octets)"
    cp "$WASM" ./metrics.wasm
    echo "Copie : ./metrics.wasm"
else
    echo "ERREUR : binaire .wasm introuvable"
    exit 1
fi