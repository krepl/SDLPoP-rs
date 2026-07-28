#!/usr/bin/env bash
# Builds the wasm32-unknown-unknown target and regenerates web/pkg/ via wasm-bindgen.
# Requires: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli
# --version <matching the wasm-bindgen crate version pinned in Cargo.lock>.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --target wasm32-unknown-unknown

WBG_VERSION=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep version | head -1 | cut -d'"' -f2)
INSTALLED_VERSION=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || echo "")
if [ "$INSTALLED_VERSION" != "$WBG_VERSION" ]; then
    echo "wasm-bindgen CLI version ($INSTALLED_VERSION) doesn't match Cargo.lock ($WBG_VERSION)."
    echo "Run: cargo install wasm-bindgen-cli --version $WBG_VERSION --locked"
    exit 1
fi

wasm-bindgen --target web --out-dir web/pkg --out-name sdlpop \
    target/wasm32-unknown-unknown/debug/prince.wasm

echo "Built web/pkg/. Serve web/ over HTTP (ES modules need it, not file://), e.g.:"
echo "  cd web && python3 -m http.server 8642"
