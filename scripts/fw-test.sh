#!/usr/bin/env bash
# Layer 1 of the firmware simulation (D-025): firmware logic tested on the laptop, unlimited.
#
#   scripts/fw-test.sh            gcc unit tests, then the ESP-IDF linux-target app
#   scripts/fw-test.sh --gcc      only the gcc unit tests (no ESP-IDF needed)
#
# gcc tests: every firmware/test/test_*.c and boards/*/firmware/test/test_*.c is one program.
# Its first line names the sources it tests:  // SOURCES: firmware/components/x/x.c ...
# It is built with the address and undefined-behaviour sanitizers against the component
# headers, with firmware/test/stubs first (laptop stand-ins for board.h, esp_timer.h), and
# must print `UNIT {"file":...,"pass":N,"fail":0}` (firmware/test/unit.h) and exit 0.
#
# linux app: firmware/test/linux, shared code that needs NVS or FreeRTOS, built for ESP-IDF's
# linux target (a preview target in v6.1, so `idf.py --preview`), also with the sanitizers.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/firmware/test/build"
mkdir -p "$out"
only_gcc=false
[[ "${1:-}" == "--gcc" ]] && only_gcc=true

includes=(-I"$root/firmware/test/stubs" -I"$root/firmware/test")
for c in "$root"/firmware/components/*/; do
    includes+=(-I"$c" -I"$c/include")
done
flags=(-std=gnu17 -O1 -g -Wall -Wextra -Werror -fsanitize=address,undefined
       -fno-sanitize-recover=all -fno-omit-frame-pointer)

failed=()
total_pass=0

# Checks one test program's output: exit 0 and a UNIT line with no failures.
check() { # name, exit code, output file
    local name=$1 code=$2 log=$3 unit pass fail
    unit=$(grep -a '^UNIT {' "$log" | tail -1 || true)
    pass=$(sed -n 's/.*"pass":\([0-9]*\).*/\1/p' <<<"$unit")
    fail=$(sed -n 's/.*"fail":\([0-9]*\).*/\1/p' <<<"$unit")
    if [[ $code -eq 0 && -n "$unit" && "$fail" == 0 && "${pass:-0}" -gt 0 ]]; then
        printf '   %-28s pass %s\n' "$name" "$pass"
        total_pass=$((total_pass + pass))
    else
        printf '   %-28s FAIL (exit %s, %s)\n' "$name" "$code" "${unit:-no UNIT line}"
        grep -av '^UNIT {' "$log" | tail -20 | sed 's/^/      /'
        failed+=("$name")
    fi
}

echo "== FW-TEST (gcc, sanitizers on)"
shopt -s nullglob
for t in "$root"/firmware/test/test_*.c "$root"/boards/*/firmware/test/test_*.c; do
    name=$(basename "$t" .c)
    [[ "$t" == "$root"/boards/* ]] && name="$(basename "$(dirname "$(dirname "$(dirname "$t")")")")/$name"
    sources=$(sed -n '1s#^// SOURCES:##p' "$t")
    extra=()
    # a board's own test also sees its firmware directory (board_pins.h, main/)
    [[ "$t" == "$root"/boards/* ]] && extra=(-I"$(dirname "$(dirname "$t")")" -I"$(dirname "$(dirname "$t")")/main")
    bin="$out/${name//\//_}"
    log="$bin.log"
    srcs=()
    for s in $sources; do srcs+=("$root/$s"); done
    if ! gcc "${flags[@]}" "${includes[@]}" "${extra[@]}" "$t" "${srcs[@]}" -lm -o "$bin" >"$log" 2>&1; then
        printf '   %-28s FAIL (does not compile)\n' "$name"
        sed 's/^/      /' "$log" | head -30
        failed+=("$name")
        continue
    fi
    code=0
    timeout 60 "$bin" >"$log" 2>&1 || code=$?
    check "$name" "$code" "$log"
done

if ! $only_gcc; then
    echo "== FW-TEST (ESP-IDF linux target, sanitizers on)"
    idf=${IDF_PATH:-$HOME/esp/esp-idf-v6.1}
    app="$root/firmware/test/linux"
    log="$out/linux-build.log"
    # a changed sdkconfig.defaults needs a fresh sdkconfig (as the sim stage does)
    [[ "$app/sdkconfig.defaults" -nt "$app/sdkconfig" ]] && rm -f "$app/sdkconfig"
    if ! (cd "$app" && . "$idf/export.sh" >/dev/null 2>&1 && idf.py --preview build) >"$log" 2>&1; then
        printf '   %-28s FAIL (build; log: %s)\n' "linux app" "$log"
        tail -25 "$log" | sed 's/^/      /'
        failed+=("linux app")
    else
        elf=$(ls "$app"/build/*.elf)
        code=0
        (cd "$app/build" && timeout 120 "$elf") >"$out/linux.log" 2>&1 || code=$?
        check "linux: $(basename "$elf" .elf)" "$code" "$out/linux.log"
    fi
fi

if ((${#failed[@]})); then
    echo "== FW-TEST: FAIL (${failed[*]}); logs in $out"
    exit 1
fi
echo "== FW-TEST: PASS ($total_pass tests passed)"
