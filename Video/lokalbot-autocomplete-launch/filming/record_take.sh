#!/bin/bash
# Records the main display for N seconds into the raw folder (full screen;
# crop to the writing window afterwards with crop_take.sh). Never overwrites.
set -euo pipefail
name="$1"; seconds="${2:-40}"
raw="${RAW_DIR:?set RAW_DIR}"
out="$raw/$name.mov"
[ ! -e "$out" ] || { echo "exists: $out" >&2; exit 1; }
exec screencapture -v -V "$seconds" -x -D "${DISPLAY_NUM:-1}" "$out"
