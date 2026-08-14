#!/usr/bin/env bash
# Build slippypack-web to www/pkg/. Requires:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.127   (must match the crate)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(cd .. && pwd)

cargo build -p slippypack-web --target wasm32-unknown-unknown --release --manifest-path "$ROOT/Cargo.toml"
wasm-bindgen --target web --out-dir pkg "$ROOT/target/wasm32-unknown-unknown/release/slippypack_web.wasm"

# --all-features is not optional: wasm-opt 116 fails to validate this
# module without it (DECISIONS.md E-001).
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --all-features -o pkg/slippypack_web_bg.wasm pkg/slippypack_web_bg.wasm
  echo "wasm-opt -Oz applied"
else
  echo "wasm-opt not found — skipping size optimisation" >&2
fi

ls -l pkg/slippypack_web_bg.wasm
