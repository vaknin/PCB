#!/usr/bin/env bash
# Builds one variant of the real-hardware codec spike into build-<name>/ and optionally flashes it.
#   build.sh <name> [flash]     name: plain | speech | memtest | skipvalidate | quiet | quiet-skipvalidate
# Each name part after a '-' adds sdkconfig.<part> on top of sdkconfig.defaults ("plain" adds none).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
name=$1
defaults="sdkconfig.defaults"
IFS=- read -ra parts <<<"$name"
for p in "${parts[@]}"; do [[ $p == plain ]] || defaults+=";sdkconfig.$p"; done
. "${IDF_PATH:-$HOME/esp/esp-idf-v6.1}/export.sh" >/dev/null 2>&1
cd "$here"
args=(-B "build-$name" -D "SDKCONFIG=build-$name/sdkconfig" -D "SDKCONFIG_DEFAULTS=$defaults")
idf.py "${args[@]}" build >"build-$name.log" 2>&1 || { tail -40 "build-$name.log"; exit 1; }
mv "build-$name.log" "build-$name/build.log"
if [[ ${2:-} == flash ]]; then
    idf.py "${args[@]}" -p "${PORT:-/dev/ttyACM0}" flash 2>&1 | grep -E "Hash of data verified|Hard resetting|rror" || true
fi
