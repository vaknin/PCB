#!/usr/bin/env bash
# Prints the path of the no-core-dump preload library (scripts/nodump.c), building it if needed.
# Use: LD_PRELOAD="$(scripts/nodump.sh)" qemu-system-xtensa ...
set -euo pipefail
src="$(dirname "$(readlink -f "$0")")/nodump.c"
out="${XDG_CACHE_HOME:-$HOME/.cache}/pcb/nodump.so"
if [[ ! -e "$out" || "$src" -nt "$out" ]]; then
    mkdir -p "$(dirname "$out")"
    gcc -shared -fPIC -O2 -o "$out.$$" "$src" && mv "$out.$$" "$out"
fi
echo "$out"
