#!/bin/bash
# Renders the docs/screenshots PNGs on macOS, in debug and with the AI layer.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "scripts/render-snapshots.sh only works on macOS." >&2
    exit 1
fi

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
out="${1:-$root/docs/screenshots}"
mkdir -p "$out"

echo "Building Ariadna in debug with SNAPSHOTS…"
ARIADNA_AI=1 swift build -Xswiftc -DSNAPSHOTS --product Ariadna

binary=""
for candidate in \
    ".build/debug/Ariadna" \
    ".build/arm64-apple-macosx/debug/Ariadna" \
    ".build/apple/Products/Debug/Ariadna"
do
    if [[ -f "$candidate" && -x "$candidate" ]]; then
        binary="$candidate"
        break
    fi
done
if [[ -z "$binary" ]]; then
    binary=$(find .build -type f -name Ariadna -perm -111 ! -path '*.dSYM/*' | head -n 1 || true)
fi
if [[ -z "$binary" || ! -f "$binary" ]]; then
    echo "Cannot find the Ariadna screenshots executable." >&2
    exit 1
fi

echo "Binario: $binary"
"$binary" "$out"

expected=(
    staging.png
    diff.png
    historial.png
    ramas.png
    ajustes.png
    panel-ia.png
)
for name in "${expected[@]}"; do
    file="$out/$name"
    if [[ ! -s "$file" ]]; then
        echo "Missing or empty: $file" >&2
        exit 1
    fi
done

echo "Screenshots ready in $out"
ls -lh "$out"/*.png
