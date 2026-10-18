#!/usr/bin/env bash
# Captures LokalBot on a virtual X11 desktop (Xvfb + xfwm4 with compositing),
# one PNG per page. Usage: capture-linux.sh <lokalbot> <library> <out-dir> [pages...]
set -euo pipefail
BIN="$1"; ROOT="$2"; OUT="$3"; shift 3
PAGES=("${@:-today}")
DISPLAY_NUM="${CAPTURE_DISPLAY:-:91}"
SIZE="${CAPTURE_SIZE:-1600x1000}"
SUFFIX="${CAPTURE_SUFFIX:-}"
mkdir -p "$OUT"
Xvfb "$DISPLAY_NUM" -screen 0 "${SIZE}x24" -nolisten tcp >/tmp/lokalbot-xvfb.log 2>&1 &
XVFB=$!
trap 'kill $XVFB 2>/dev/null || true' EXIT
sleep 1
DISPLAY="$DISPLAY_NUM" xsetroot -solid '#3b4a5a' 2>/dev/null || true
DISPLAY="$DISPLAY_NUM" xfwm4 --compositor=on >/tmp/lokalbot-xfwm4.log 2>&1 &
WM=$!
sleep 1
for page in "${PAGES[@]}"; do
  DISPLAY="$DISPLAY_NUM" LOKALBOT_STORAGE_ROOT="$ROOT" "$BIN" --page "$page" >/tmp/lokalbot-$page.log 2>&1 &
  APP=$!
  sleep "${CAPTURE_WAIT:-8}"
  if ! kill -0 "$APP" 2>/dev/null; then
    echo "LokalBot exited early on page $page:"; cat "/tmp/lokalbot-$page.log"; exit 1
  fi
  DISPLAY="$DISPLAY_NUM" ffmpeg -loglevel error -y -f x11grab -video_size "$SIZE" -i "$DISPLAY_NUM" -frames:v 1 "$OUT/linux-desktop-$page$SUFFIX.png"
  # Crop to the LokalBot window using its X geometry.
  WID=$(DISPLAY="$DISPLAY_NUM" xdotool search --sync --onlyvisible --name '^LokalBot$' | head -1 || true)
  if [ -n "$WID" ]; then
    eval "$(DISPLAY="$DISPLAY_NUM" xdotool getwindowgeometry --shell "$WID")"
    ffmpeg -loglevel error -y -i "$OUT/linux-desktop-$page$SUFFIX.png" -vf "crop=$WIDTH:$HEIGHT:$X:$Y" "$OUT/linux-$page$SUFFIX.png"
  fi
  kill "$APP" 2>/dev/null || true
  wait "$APP" 2>/dev/null || true
  echo "captured $page"
done
kill $WM 2>/dev/null || true
