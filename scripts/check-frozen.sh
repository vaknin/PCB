#!/usr/bin/env bash
# Before any order: the board's freeze tag exists and boards/<board> is unchanged since it
# (committed, uncommitted and untracked files alike; review/ excepted). D-023.
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
echo "OK: $dir is exactly $tag ($(git rev-parse --short "$tag^{commit}"))"
