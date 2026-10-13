#!/bin/bash
# Crop raw full-display takes to TextEdit's text area and encode for the edit.
# Window was centered on the LG (3440x1440, 1x): text area below the ruler, x 860..2552, y 520..806.
set -euo pipefail
raw="${RAW_DIR:?set RAW_DIR}"
out="$(cd "$(dirname "$0")/.." && pwd)/assets/footage"
mkdir -p "$out"
for pair in hook:hook-2:20 remember:remember-3:22 quiet:quiet-2:17 lang-de:lang-de:22 lang-fr:lang-fr:22 lang-it:lang-it-3:22 lang-es:lang-es:24; do
  IFS=: read -r name take dur <<< "$pair"
  ffmpeg -v error -y -i "$raw/$take.mov" -t "$dur" -vf "crop=1692:286:860:520,fps=30,format=yuv420p" \
    -c:v libx264 -preset slow -crf 14 -an -movflags +faststart "$out/$name.mp4"
  echo "$name <- $take ($(ffprobe -v error -show_entries format=duration -of csv=p=0 "$out/$name.mp4") s)"
done
