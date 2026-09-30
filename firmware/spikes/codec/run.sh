#!/usr/bin/env bash
# Phase D.1 codec spike (D-025): builds the app, runs it in Espressif QEMU with the module's flash
# and PSRAM (same flags as pcbgen's sim stage), copies each encoded OGG out of the flash image into
# out/, and checks it with ffprobe. The input is a local Capture recording, converted to 16 kHz mono;
# it stays out of git (public repo).
#   firmware/spikes/codec/run.sh [recording]   (default: Capture's tools/samples/two-ideas.ogg)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
src=${1:-$HOME/Projects/capture/tools/samples/two-ideas.ogg}
mkdir -p "$here/out"
ffmpeg -loglevel error -y -i "$src" -ac 1 -ar 16000 -c:a pcm_s16le "$here/out/speech.wav"

. "${IDF_PATH:-$HOME/esp/esp-idf-v6.1}/export.sh" >/dev/null 2>&1
cd "$here"
idf.py build >out/build.log 2>&1 || { tail -30 out/build.log; exit 1; }
(cd build && esptool --chip esp32s3 merge-bin --output ../out/flash.bin --pad-to-size 16MB @flash_args >/dev/null)
# A blank otadata makes the app write its entry at boot, which crashes QEMU with octal PSRAM even
# with rollback off; write the "ota_0 valid" entry first, as pcbgen's sim stage does
# (OTADATA_VALID_OTA0 in crates/pcbgen/src/sim.rs, HARDWARE_LESSONS).
otadata=$(awk '/ota_data_initial.bin/{print $1}' build/flash_args)
printf '\x01\x00\x00\x00\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x02\x00\x00\x00\x9a\x98\x43\x47' \
    | dd of=out/flash.bin bs=1 seek=$((otadata)) conv=notrunc status=none

qemu=$(ls -d "${IDF_TOOLS_PATH:-$HOME/.espressif}"/tools/qemu-xtensa/*/qemu/bin/qemu-system-xtensa | sort | tail -1)
# -m 8M and the octal flag: HARDWARE_LESSONS (QEMU 9.2.2 with octal PSRAM)
timeout 600 "$qemu" -M esp32s3 -m 8M -drive file=out/flash.bin,if=mtd,format=raw \
    -global driver=ssi_psram,property=is_octal,value=true -nographic -serial mon:stdio </dev/null \
    | tee out/qemu.log | while IFS= read -r l; do
        l=${l%$'\r'}
        [[ $l == SPIKE* || $l == *"Guru Meditation"* || $l == abort* ]] && echo "$l"
        [[ $l == "SPIKE DONE" || $l == *"Guru Meditation"* ]] && pkill -f "file=out/flash.bin" && break
    done || true

# storage partition offset from the partition table; slot n at n MiB, 16-byte header
storage=$(python3 "$IDF_PATH/components/partition_table/gen_esp32part.py" build/partition_table/partition-table.bin 2>/dev/null \
    | awk -F, '$1=="storage"{print $4}')
python3 - "$storage" <<'PY'
import struct, sys
base = int(sys.argv[1], 0)
img = open("out/flash.bin", "rb").read()
for slot in range(2):
    at = base + slot * 1024 * 1024
    if img[at:at + 8] != b"OGGSPIKE":
        print(f"slot {slot}: empty"); continue
    n, cx = struct.unpack("<Ii", img[at + 8:at + 16])
    open(f"out/opus-c{cx}.ogg", "wb").write(img[at + 16:at + 16 + n])
    print(f"slot {slot}: out/opus-c{cx}.ogg, {n} bytes")
PY
for f in out/opus-c*.ogg; do
    echo "== $f"
    ffprobe -v error -show_entries stream=codec_name,sample_rate,channels:format=format_name,duration,bit_rate -of compact "$f"
    ffmpeg -v error -i "$f" -f null - && echo "decodes cleanly"
done
