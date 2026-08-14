#!/usr/bin/env bash
# Copy the handful of dependency files the page actually loads out of
# node_modules/ and into vendor/.
#
# Why: node_modules/ is not deployable and not in the repo, so a page
# that imports from it works locally and 404s everywhere else. Vendoring
# means the dev layout and the deployed layout are the same layout, and
# the import map has one set of paths rather than one per environment.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -d node_modules ]; then
  echo "node_modules/ missing — run npm install first" >&2
  exit 1
fi

mkdir -p vendor
cp node_modules/maplibre-gl/dist/maplibre-gl.mjs   vendor/
# maplibre-gl.mjs imports this sibling chunk relatively; vendoring the
# entrypoint alone gets you a 404 at runtime and nothing at build time.
cp node_modules/maplibre-gl/dist/maplibre-gl-shared.mjs vendor/
# Loaded as a Worker, not imported, so a missing copy does not error --
# tile parsing just never happens and the map never goes idle. That
# failure looks exactly like a hang, so it is worth naming here.
cp node_modules/maplibre-gl/dist/maplibre-gl-worker.mjs vendor/
cp node_modules/maplibre-gl/dist/maplibre-gl.css   vendor/
cp node_modules/pmtiles/dist/esm/index.js          vendor/pmtiles.js
cp node_modules/fflate/esm/browser.js              vendor/fflate.js

# pmtiles' ESM entry imports "fflate" as a bare specifier; the import map
# in index.html is what resolves it. Nothing to rewrite here.
# Guard against a dependency growing a new relative import: every
# "./x.mjs" a vendored file references must itself be vendored.
missing=0
for f in vendor/*.mjs vendor/*.js; do
  while read -r ref; do
    [ -f "vendor/$ref" ] || { echo "MISSING: $f references ./$ref, which is not vendored" >&2; missing=1; }
  done < <(grep -oE '"\./[A-Za-z0-9_.-]+\.(mjs|js)"' "$f" 2>/dev/null | tr -d '"' | sed 's|^\./||' | sort -u)
done
[ "$missing" -eq 0 ] || exit 1

echo "vendored:"
ls -1 vendor/
