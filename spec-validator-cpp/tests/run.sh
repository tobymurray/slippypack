#!/usr/bin/env bash
# tests/run.sh — orchestrate the cross-implementation check.
#
# Builds slippypack (if needed) and the C++ validator, produces a
# synthetic .upack, validates it. Also runs a corruption check to make
# sure the validator actually rejects bad bytes (not just everything).
#
# Usage:
#   bash tests/run.sh        # from spec-validator-cpp/
#   make test                # equivalent
#
# Exit 0 on success, non-zero on any failure.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$VALIDATOR_DIR/.." && pwd)"

VALIDATOR_BIN="$VALIDATOR_DIR/build/upack_validate"
TMP_DIR="$(mktemp -d -t slippypack-spec-validator.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

SYNTH_PACK="$TMP_DIR/synth.upack"
CORRUPT_PACK="$TMP_DIR/synth-corrupt.upack"

echo "=== Building slippypack CLI (release) ==="
(cd "$REPO_ROOT" && cargo build --release --bin slippypack)
SLIPPYPACK_BIN="$REPO_ROOT/target/release/slippypack"

echo "=== Building C++ validator ==="
(cd "$VALIDATOR_DIR" && make --no-print-directory)

echo "=== 1. Build synthetic pack ==="
"$SLIPPYPACK_BIN" make --source synthetic --out "$SYNTH_PACK" --timestamp 0
ls -la "$SYNTH_PACK"

echo "=== 2. Validate (expect exit 0) ==="
"$VALIDATOR_BIN" "$SYNTH_PACK"

echo "=== 3. Corrupt one byte in the header → expect exit 1 ==="
cp "$SYNTH_PACK" "$CORRUPT_PACK"
# Flip a byte at offset 10 (inside pack_uuid — doesn't affect index/
# tile decoding but invalidates the CRC).
printf '\x00' | dd of="$CORRUPT_PACK" bs=1 seek=10 count=1 conv=notrunc 2>/dev/null
if "$VALIDATOR_BIN" "$CORRUPT_PACK" 2>&1 > /dev/null; then
  echo "FAIL: validator accepted a corrupted pack"
  exit 1
else
  echo "OK: validator rejected corrupted pack"
fi

echo "=== ALL CHECKS PASSED ==="
