#!/usr/bin/env bash
# Freeze a board revision once the owner says "freeze": tag <board>-rev<revision>-freeze
# on the draft round the owner reviewed (docs/workflow.md step 4, D-023).
# usage: scripts/freeze.sh <board>
# The revision comes from boards/<board>/board.toml. HEAD must carry a draft tag, so the
# frozen version is one the owner has seen. Local tag only.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
board=${1:?usage: scripts/freeze.sh <board>}
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
tag=$board-rev$rev-freeze
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "$tag already exists (at $(git rev-parse --short "$tag^{commit}")); bump [board] revision for a new revision" >&2
  exit 1
fi
git tag -a "$tag" -m "$board revision $rev frozen at $draft"
echo "$tag -> $draft ($(git rev-parse --short HEAD))"
