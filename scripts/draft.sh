#!/usr/bin/env bash
# Tag a design round: <board>-draft-<n>, the next n (docs/workflow.md step 4, D-023).
# usage: scripts/draft.sh <board> ["what changed in this round"]
# Needs a clean tree, so the tag is exactly what the review page shows. Local tag only.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
board=${1:?usage: scripts/draft.sh <board> [message]}
[[ -d boards/$board ]] || { echo "no boards/$board" >&2; exit 1; }
if [[ -n $(git status --porcelain -- . ":!boards/*/review") ]]; then
  echo "commit or discard these first; a draft tag must match the tree:" >&2
  git status --short -- . ":!boards/*/review" >&2
  exit 1
fi
last=$(git tag -l "$board-draft-*" | sed "s/^$board-draft-//" | grep -E '^[0-9]+$' | sort -n | tail -1 || true)
n=$(( ${last:-0} + 1 ))
tag=$board-draft-$n
git tag -a "$tag" -m "${2:-$board design round $n}"
echo "$tag -> $(git rev-parse --short HEAD)"
