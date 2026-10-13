#!/bin/bash
# Crop raw full-display takes to TextEdit's text area and encode for the edit.
# Window was centered on the LG (3440x1440, 1x): text area below the ruler, x 860..2552, y 520..806.
set -euo pipefail
raw="${RAW_DIR:?set RAW_DIR}"
out="$(cd "$(dirname "$0")/.." && pwd)/assets/footage"
mkdir -p "$out"
# name:take:seconds:top — the 2026-10-13 John/Emma reshoot sat 18 px higher after re-centering.
for pair in hook:hook-2:20:520 remember:remember-john:34:502 quiet:quiet-emma:26:502:14.6 lang-de:lang-de:22:520 lang-fr:lang-fr:22:520 lang-it:lang-it-3:22:520 lang-es:lang-es:24:520; do
  IFS=: read -r name take dur top freeze <<< "$pair"
  hold=""
  # Hold the last real frame after the final accept (extra Tab presses there only inserted tabs).
  [ -n "${freeze:-}" ] && hold=",trim=0:$freeze,setpts=PTS-STARTPTS,tpad=stop_mode=clone:stop_duration=$(echo "$dur - $freeze" | bc)"
  ffmpeg -v error -y -i "$raw/$take.mov" -t "$dur" -vf "crop=1692:286:860:$top,fps=30$hold,format=yuv420p" \
    -c:v libx264 -preset slow -crf 14 -an -movflags +faststart "$out/$name.mp4"
  echo "$name <- $take ($(ffprobe -v error -show_entries format=duration -of csv=p=0 "$out/$name.mp4") s)"
done
