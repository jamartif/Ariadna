#!/bin/sh
# Activa los hooks versionados sin tocar otros ajustes de git.
set -e
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
chmod +x "$root/.githooks/pre-commit"
git -C "$root" config core.hooksPath .githooks
echo "Hooks activos en .githooks (pre-commit ejecuta gitleaks)."
