#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$HERE/deadass.vpk}"
echo "packing $HERE/content -> $OUT"
if command -v vpk >/dev/null 2>&1; then
  vpk create "$HERE/content" -o "$OUT"
else
  echo "vpk CLI not found; zip content as placeholder"
  (cd "$HERE/content" && zip -qr "$OUT" .)
fi
echo "wrote $OUT"
