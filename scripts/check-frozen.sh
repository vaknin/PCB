#!/usr/bin/env bash
# Before any order: the board's freeze tag exists and boards/<board> is unchanged since it
# (committed, uncommitted and untracked files alike; review/ excepted). D-023.
# Also: every footprint and symbol the board uses, and kicad-cli, are what the freeze recorded
# in the tag's message (scripts/lib-hashes.sh), so a library that changed under an unchanged
# board can't slip into an order.
# usage: scripts/check-frozen.sh <board>
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
board=${1:?usage: scripts/check-frozen.sh <board>}
toml=boards/$board/board.toml
[[ -f $toml ]] || { echo "FAIL: no $toml" >&2; exit 1; }
rev=$(sed -n '/^\[board\]/,/^\[/ s/^revision *= *"\([^"]*\)".*/\1/p' "$toml" | head -1)
tag=$board-rev$rev-freeze
git rev-parse -q --verify "refs/tags/$tag" >/dev/null || { echo "FAIL: no tag $tag; the owner has not frozen revision $rev" >&2; exit 1; }
dir=boards/$board
changed=$(git diff --name-only "$tag" -- "$dir" ":!$dir/review"; git ls-files --others --exclude-standard -- "$dir" ":!$dir/review")
if [[ -n $changed ]]; then
  echo "FAIL: $dir changed since $tag:" >&2
  echo "$changed" | sort -u | sed 's/^/  /' >&2
  exit 1
fi
frozen=$(git tag -l --format='%(contents)' "$tag" | sed -n '/^libraries (scripts\/lib-hashes.sh):$/,$p' | sed '1d' | sed '/^$/d')
[[ -n $frozen ]] || { echo "FAIL: $tag records no library hashes (frozen before the drift guard); re-freeze with scripts/freeze.sh" >&2; exit 1; }
now=$(scripts/lib-hashes.sh "$board") || { echo "FAIL: could not hash the libraries $board uses" >&2; exit 1; }
if [[ $frozen != "$now" ]]; then
  echo "FAIL: libraries changed since $tag (what $board would be regenerated from):" >&2
  # one line per changed item: name, then frozen -> now
  key='$1 == "kicad-cli" { print $1, $2; next } { print $1 ":" $2, $3 }'
  LC_ALL=C join -a1 -a2 -e GONE -o 0,1.2,2.2 \
    <(awk "$key" <<<"$frozen" | LC_ALL=C sort) \
    <(awk "$key" <<<"$now" | LC_ALL=C sort) |
    awk '$2 != $3 { print "  " $1 ": " $2 " -> " $3 }' >&2
  exit 1
fi
echo "OK: $dir is exactly $tag ($(git rev-parse --short "$tag^{commit}")); $(grep -c '^footprint ' <<<"$now") footprints, $(grep -c '^symbol ' <<<"$now") symbols and kicad-cli unchanged"
