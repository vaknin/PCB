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
# One IDF build at a time on this laptop (parallel builds ran it out of memory): every build
# takes $IDF_LOCK. Flashing goes straight through esptool, so it neither builds nor needs the lock.
lock=${IDF_LOCK:-/tmp/claude-1000/-home-kivan-Projects-PCB/7c38eaf4-dc6d-44db-8660-ecb16a5b33fd/scratchpad/idf.lock}
[[ -d $(dirname "$lock") ]] || lock=${TMPDIR:-/tmp}/idf-build.lock
flock "$lock" idf.py "${args[@]}" build >"build-$name.log" 2>&1 || { tail -40 "build-$name.log"; exit 1; }
mv "build-$name.log" "build-$name/build.log"
if [[ ${2:-} == flash || ${2:-} == flash-clean ]]; then
    port=${PORT:-/dev/ttyACM0}
    # flash-clean: wipe the whole app slot first, so nothing of a bigger earlier image (the
    # speech build) stays behind the end of this one
    if [[ $2 == flash-clean ]]; then
        python -m esptool --chip esp32s3 -p "$port" -b 460800 --after no-reset erase-region 0x20000 0x300000 2>&1 |
            grep -E "erased|rror" || true
    fi
    (cd "build-$name" && python -m esptool --chip esp32s3 -p "$port" -b 460800 --before default-reset \
        --after hard-reset write-flash @flash_args 2>&1) | grep -E "Hash of data verified|Hard resetting|rror" || true
fi
