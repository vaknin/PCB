#!/usr/bin/env bash
# Render a board's top side (copper, fab, courtyard, silk, edge) to PNG for a visual check.
# usage: scripts/render.sh boards/<name> [out.png] [layers]
set -euo pipefail
dir=$1; name=$(basename "$dir")
out=${2:-$dir/kicad/render-top.png}
layers=${3:-F.Cu,B.Cu,F.Fab,F.Courtyard,F.SilkS,Edge.Cuts}
tmp=$(mktemp --suffix=.svg)
kicad-cli pcb export svg --mode-single --fit-page-to-board --exclude-drawing-sheet \
  --sketch-pads-on-fab-layers -l "$layers" -o "$tmp" "$dir/kicad/$name.kicad_pcb" 2>&1 | grep -v -e assert -e Plotted -e Done || true
rsvg-convert -w 1100 -b white "$tmp" -o "$out"
rm -f "$tmp"
echo "$out"
