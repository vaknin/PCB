#!/usr/bin/env bash
# The library manifest of a board: the sha256 of every footprint and symbol its KiCad files use,
# plus the kicad-cli version. scripts/freeze.sh stores it in the freeze tag's message;
# scripts/check-frozen.sh recomputes it and refuses an order if a line differs. A board file
# that didn't change can still come out different when pcbgen regenerates it from a library
# that did (a KiCad update, an edited lib/ footprint): this is the guard for that.
# usage: scripts/lib-hashes.sh <board>
# Output, sorted, one per line:
#   kicad-cli <version>
#   footprint <Lib:Name> <sha256 of the .kicad_mod | MISSING>
#   symbol <Lib:Name> <sha256 of its (symbol ...) block | MISSING>
# Libraries are found through boards/<board>/kicad/fp-lib-table and sym-lib-table:
# ${KICAD10_FOOTPRINT_DIR} (default /usr/share/kicad/footprints), ${KICAD10_SYMBOL_DIR}
# (default /usr/share/kicad/symbols), and the repo's own lib/ (read from this checkout, so a
# worktree or clone hashes its own copy). A symbol that `extends` another also hashes the parent.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
board=${1:?usage: scripts/lib-hashes.sh <board>}
kicad=boards/$board/kicad
for f in "$kicad/$board.kicad_pcb" "$kicad/$board.kicad_sch" "$kicad/fp-lib-table" "$kicad/sym-lib-table"; do
  [[ -f $f ]] || { echo "no $f: run the board's sch and pcb stages first" >&2; exit 1; }
done
fp_dir=${KICAD10_FOOTPRINT_DIR:-/usr/share/kicad/footprints}
sym_dir=${KICAD10_SYMBOL_DIR:-/usr/share/kicad/symbols}
repo=$PWD

# uri of library <name> in lib table <file>, with the variables and this checkout's lib/ filled in
uri() {
  local u
  u=$(awk -v n="$2" '
    /\(name "/ { match($0, /\(name "[^"]*"/); cur = substr($0, RSTART + 7, RLENGTH - 8) }
    /\(uri "/ && cur == n { match($0, /\(uri "[^"]*"/); print substr($0, RSTART + 6, RLENGTH - 7); exit }
  ' "$1")
  [[ -n $u ]] || return 1
  u=${u//'${KICAD10_FOOTPRINT_DIR}'/$fp_dir}
  u=${u//'${KICAD10_SYMBOL_DIR}'/$sym_dir}
  u=${u//'${KIPRJMOD}'/$repo/$kicad}
  [[ $u == */lib/footprints/* || $u == */lib/symbols/* ]] && [[ $u != "$repo"/* ]] && u=$repo/lib/${u#*/lib/}
  printf '%s\n' "$u"
}

# the top-level (symbol "<name>" ...) block of a KiCad symbol library, as KiCad formats it
symbol_block() {
  awk -v n="$2" '
    $0 == "\t(symbol \"" n "\"" { on = 1 }
    on { print }
    on && $0 == "\t)" { exit }
  ' "$1"
}

echo "kicad-cli $(kicad-cli --version)"
{
for id in $(grep -o -P '^\t\(footprint "\K[^"]+' "$kicad/$board.kicad_pcb" | sort -u); do
  lib=${id%%:*} name=${id#*:}
  dir=$(uri "$kicad/fp-lib-table" "$lib") || { echo "footprint $id MISSING"; continue; }
  f=$dir/$name.kicad_mod
  if [[ -f $f ]]; then echo "footprint $id $(sha256sum < "$f" | cut -d' ' -f1)"; else echo "footprint $id MISSING"; fi
done
for id in $(grep -o -P '\(lib_id "\K[^"]+' "$kicad/$board.kicad_sch" | sort -u); do
  lib=${id%%:*} name=${id#*:}
  file=$(uri "$kicad/sym-lib-table" "$lib") || { echo "symbol $id MISSING"; continue; }
  block=$( [[ -f $file ]] && symbol_block "$file" "$name" || true)
  [[ -n $block ]] || { echo "symbol $id MISSING"; continue; }
  parent=$(grep -o -P '^\t\t\(extends "\K[^"]+' <<<"$block" || true)
  [[ -n $parent ]] && block+=$'\n'$(symbol_block "$file" "$parent")
  echo "symbol $id $(sha256sum <<<"$block" | cut -d' ' -f1)"
done
} | sort
