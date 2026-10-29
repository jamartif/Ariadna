#!/bin/bash
# Genera los PNG de docs/screenshots en macOS, en debug y con la capa de IA.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "scripts/render-snapshots.sh solo funciona en macOS." >&2
    exit 1
fi

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
out="${1:-$root/docs/screenshots}"
mkdir -p "$out"

echo "Compilando Ariadna en debug con SNAPSHOTS…"
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
    echo "No encuentro el ejecutable Ariadna de capturas." >&2
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
        echo "Falta o está vacío: $file" >&2
        exit 1
    fi
done

echo "Capturas listas en $out"
ls -lh "$out"/*.png
