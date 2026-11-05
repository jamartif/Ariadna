#!/bin/sh
# A clone does not enable .githooks until core.hooksPath points there.
# This script sets it in this repository's LOCAL config.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
if [ ! -f "$root/.githooks/pre-commit" ]; then
    echo "Cannot find $root/.githooks/pre-commit" >&2
    exit 1
fi
chmod +x "$root/.githooks/pre-commit"
git -C "$root" config --local core.hooksPath .githooks
actual=$(git -C "$root" config --local --get core.hooksPath)
if [ "$actual" != ".githooks" ]; then
    echo "core.hooksPath was not set to .githooks (value: ${actual:-empty})." >&2
    exit 1
fi
echo "core.hooksPath=.githooks in the Ariadna clone."
echo "Every commit in this repo goes through gitleaks. If you do not have it: brew install gitleaks"
echo "Ariadna does not touch the config of the other repositories you open."
