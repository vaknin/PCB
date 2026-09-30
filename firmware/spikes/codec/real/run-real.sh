#!/usr/bin/env bash
# Phase D.5 (D-025): every dev-board measurement behind ../RESULTS-devboard.md, in order. Raw
# answers go to ../out/real/ (gitignored). One ESP32-S3 (16 MB flash, 8 MB octal PSRAM) on
# $PORT, nothing else using it. Takes about an hour; the encode matrix is most of it.
#   real/run-real.sh [all|enc|ogg|boot|scan|finish]
# The speech input is ../out/speech.wav (made by ../run.sh from a local recording; never
# committed). It is only inside the `speech-*` builds, and `finish` flashes a build without it
# and wipes the saved OGG, so no recording stays on the board.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export PORT=${PORT:-/dev/ttyACM0}
out=$here/../out/real
mkdir -p "$out"
. "${IDF_PATH:-$HOME/esp/esp-idf-v6.1}/export.sh" >/dev/null 2>&1
cd "$here"
bench() { python bench.py -p "$PORT" "$@"; }
flash() { ./build.sh "$1" flash >/dev/null; sleep 2; bench -t 10 status >/dev/null; }

enc() {
    flash speech-fast
    bench matrix 3 "$out/enc-fast.jsonl" psram                      # 5 complexities x 3 clocks x 2 inputs
    bench matrix 1 "$out/enc-fast-int.jsonl" int 0,1,5 240 speech   # encoder state in internal RAM
    bench matrix 1 "$out/enc-fast-audio.jsonl" psram 0,1,3,5,10 240,160,80 speech,synth audio
    flash speech
    bench matrix 1 "$out/enc-default.jsonl" psram                   # IDF's default caches and -Og
}

ogg() {
    flash speech-fast
    for src in speech synth; do
        bench enc 1 240 $src psram 1 >"$out/saved-$src.json"
        bench dump "$out/chip-c1-$src.ogg"
        ffprobe -v error -show_entries stream=codec_name,sample_rate,channels:format=format_name,duration,bit_rate \
            -of compact "$out/chip-c1-$src.ogg"
        ffmpeg -v error -i "$out/chip-c1-$src.ogg" -f null - && echo "decodes cleanly"
    done
    bench erase
}

boot() {
    for v in fast fast-memtest fast-skipvalidate fast-quiet-skipvalidate plain; do
        flash $v
        bench restart 5 "$out/boot-$v.jsonl" >/dev/null
        bench wake 5 "$out/boot-$v.jsonl" >/dev/null
        bench bootlog >"$out/bootlog-$v.txt" || true
    done
}

scan() {
    flash fast
    # Only the count and the timings are kept: no names, addresses, signal strengths or channels.
    for i in 1 2 3 4 5; do
        bench -t 60 scan | python -c 'import sys, json
for line in sys.stdin:
    d = json.loads(line)
    print(json.dumps({k: d[k] for k in ("test", "error", "err", "networks", "wifi_start_ms", "scan_ms") if k in d}))' >>"$out/scan.jsonl"
    done
    bench psram >"$out/psram.json"
}

finish() {
    ./build.sh fast flash-clean >/dev/null; sleep 2; bench -t 10 status >/dev/null
    bench erase
    bench status
}

case ${1:-all} in
    all) enc; ogg; boot; scan; finish ;;
    enc | ogg | boot | scan | finish) "$1" ;;
    *) echo "usage: $0 [all|enc|ogg|boot|scan|finish]"; exit 2 ;;
esac
