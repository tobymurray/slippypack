#!/usr/bin/env bash
# Assemble a deployable static site into dist/.
#
# Everything the page needs at runtime and nothing it doesn't: no
# node_modules, no build scripts, no package manifests. dist/ is the
# whole site and can be served from any static host.
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/vendor.sh >/dev/null
[ -f pkg/slippypack_web.js ] || { echo "pkg/ missing — run scripts/build-wasm.sh first" >&2; exit 1; }

rm -rf dist
mkdir -p dist
cp index.html watch-style.json dist/
cp -r src vendor pkg dist/
rm -f dist/pkg/*.d.ts   # TypeScript declarations are for authoring, not serving

# GitHub Pages runs the published directory through Jekyll unless told
# not to, and Jekyll drops files and directories beginning with an
# underscore. Nothing here starts with one today, but wasm-bindgen's
# output has before.
touch dist/.nojekyll

echo "dist/ assembled — $(du -sh dist | cut -f1), $(find dist -type f | wc -l) files"
