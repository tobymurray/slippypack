#!/usr/bin/env bash
# Build slippypack-web to www/pkg/. Requires:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.127   (must match the crate)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(cd .. && pwd)

cargo build -p slippypack-web --target wasm32-unknown-unknown --release --manifest-path "$ROOT/Cargo.toml"
wasm-bindgen --target web --out-dir pkg "$ROOT/target/wasm32-unknown-unknown/release/slippypack_web.wasm"

# wasm-opt needs the features rustc actually emitted, and NOT the ones it
# didn't. `--all-features` was the old spelling of this and it shipped a
# module no browser would load: it lets binaryen rewrite call_indirect
# into *typed function references*, whose type encoding was renumbered
# when the GC proposal was finalised. Binaryen 116 writes the modern byte
# and 108 — which is what `apt install binaryen` gives you on Ubuntu
# noble — writes the pre-standard one, so the artifact was valid on the
# machine that built it and invalid everywhere else. See DECISIONS.md
# E-001. Naming the five features costs 249 bytes (0.25%) and keeps the
# output pure MVP-plus-bulk-memory, which every binaryen encodes alike.
WASM_FEATURES=(
  --enable-bulk-memory
  --enable-sign-ext
  --enable-mutable-globals
  --enable-nontrapping-float-to-int
  --enable-multivalue
)
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz "${WASM_FEATURES[@]}" -o pkg/slippypack_web_bg.wasm pkg/slippypack_web_bg.wasm
  echo "wasm-opt -Oz applied ($(wasm-opt --version))"
else
  echo "wasm-opt not found — skipping size optimisation" >&2
fi

# Refuse to ship a module the browser will reject. This is the check that
# was missing when the invalid artifact went to production: it was built,
# uploaded and deployed without anything ever asking whether it loads.
node -e '
const fs = require("fs");
const file = "pkg/slippypack_web_bg.wasm";
const bytes = fs.readFileSync(file);
if (!WebAssembly.validate(bytes)) {
  console.error(`${file} does not validate — refusing to ship it.`);
  console.error("Usually wasm-opt emitting a post-MVP type this browser/runtime does not accept.");
  process.exit(1);
}
console.log(`${file} validates (${bytes.length} bytes)`);
'
