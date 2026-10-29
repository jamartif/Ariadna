#!/bin/sh
# Un clone no activa .githooks hasta que core.hooksPath apunta ahí.
# Este script lo deja en la config LOCAL de este repositorio.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
if [ ! -f "$root/.githooks/pre-commit" ]; then
    echo "No encuentro $root/.githooks/pre-commit" >&2
    exit 1
fi
chmod +x "$root/.githooks/pre-commit"
git -C "$root" config --local core.hooksPath .githooks
actual=$(git -C "$root" config --local --get core.hooksPath)
if [ "$actual" != ".githooks" ]; then
    echo "core.hooksPath no quedó en .githooks (vale: ${actual:-vacío})." >&2
    exit 1
fi
echo "core.hooksPath=.githooks en el clon de Ariadna."
echo "Cada commit de este repo pasa por gitleaks. Si no lo tienes: brew install gitleaks"
echo "Ariadna no toca la config de los demás repositorios que abras."
