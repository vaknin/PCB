#!/usr/bin/env bash
# Freeze a board revision once the owner says "freeze": tag <board>-rev<revision>-freeze
# on the draft round the owner reviewed (docs/workflow.md step 4, D-023).
# usage: scripts/freeze.sh [--dry-run] <board>
# The revision comes from boards/<board>/board.toml. HEAD must carry a draft tag, so the
# frozen version is one the owner has seen. The readiness page (the `review` stage) must be
# newer than board.toml and show nothing red (docs/workflow.md "Right the first time").
# The tag's message carries the library manifest (scripts/lib-hashes.sh: the sha256 of every
# footprint and symbol the board uses, and the kicad-cli version), which check-frozen.sh
# compares before an order. `--dry-run` runs every check and prints the message, tags nothing.
# Local tag only.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
dry=
[[ ${1:-} == --dry-run ]] && { dry=1; shift; }
board=${1:?usage: scripts/freeze.sh [--dry-run] <board>}
toml=boards/$board/board.toml
[[ -f $toml ]] || { echo "no $toml (copy templates/board.toml)" >&2; exit 1; }
rev=$(sed -n '/^\[board\]/,/^\[/ s/^revision *= *"\([^"]*\)".*/\1/p' "$toml" | head -1)
[[ -n $rev ]] || { echo "no [board] revision in $toml" >&2; exit 1; }
if [[ -n $(git status --porcelain -- . ":!boards/*/review") ]]; then
  echo "the tree has uncommitted changes; freeze a clean, reviewed draft" >&2
  exit 1
fi
draft=$(git tag --points-at HEAD -l "$board-draft-*" | head -1)
[[ -n $draft ]] || { echo "HEAD has no $board-draft-* tag: tag the round (scripts/draft.sh) and show the owner its review page first" >&2; exit 1; }
ready=boards/$board/review/readiness.json
run="cargo run --release -p $board -- review"
[[ -f $ready ]] || { echo "no $ready: make the readiness page ($run) and show it to the owner first" >&2; exit 1; }
[[ $toml -nt $ready ]] && { echo "$ready is older than $toml: re-run the review stage ($run) and show the owner the new readiness page" >&2; exit 1; }
red=$(sed -n 's/^ *"red": *\([0-9][0-9]*\).*/\1/p' "$ready" | head -1)
[[ -n $red ]] || { echo "no \"red\" count in $ready: re-run the review stage ($run)" >&2; exit 1; }
if (( red > 0 )); then
  echo "the readiness page has $red red item(s), which block freeze (boards/$board/review/readiness.html; listed under \"blocking\" in $ready):" >&2
  echo "prove each, design it out, or record the owner's acceptance in $toml ([[risk]] accepted), then re-run the review stage" >&2
  exit 1
fi
tag=$board-rev$rev-freeze
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "$tag already exists (at $(git rev-parse --short "$tag^{commit}")); bump [board] revision for a new revision" >&2
  exit 1
fi
libs=$(scripts/lib-hashes.sh "$board") || { echo "could not hash the libraries $board uses (scripts/lib-hashes.sh)" >&2; exit 1; }
if grep -q ' MISSING$' <<<"$libs"; then
  echo "$board uses library items that can't be found:" >&2
  grep ' MISSING$' <<<"$libs" | sed 's/^/  /' >&2
  exit 1
fi
msg="$board revision $rev frozen at $draft

libraries (scripts/lib-hashes.sh):
$libs"
if [[ -n $dry ]]; then
  echo "dry run: would tag $tag at $draft ($(git rev-parse --short HEAD)) with:"
  echo "$msg"
  exit 0
fi
git tag -a "$tag" -m "$msg"
echo "$tag -> $draft ($(git rev-parse --short HEAD)); $(grep -c '^footprint ' <<<"$libs") footprints and $(grep -c '^symbol ' <<<"$libs") symbols hashed"
