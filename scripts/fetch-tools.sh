#!/usr/bin/env bash
# Fetch Freerouting 2.4.1 (Linux bundle with its own Java 25 runtime) into tools/.
set -euo pipefail
cd "$(dirname "$0")/.."
dest=tools/freerouting-2.4.1-linux-x64
[[ -x $dest/bin/freerouting ]] && { echo "already present: $dest"; exit 0; }
mkdir -p tools
curl -sSL -o tools/fr.zip https://github.com/freerouting/freerouting/releases/download/v2.4.1/freerouting-2.4.1-linux-x64.zip
echo "3ad5a956ab474b12f331d24195feadac90e8344b8e013c6a4ab26e203ce51519  tools/fr.zip" | sha256sum -c -
unzip -q tools/fr.zip -d tools && rm tools/fr.zip
echo "installed: $dest"
