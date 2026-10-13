#!/bin/bash
# Builds a throwaway filming copy of LokalBot whose autocomplete ghost is
# visible to screen recording (the shipping overlay sets sharingType = .none).
# Source comes from `git archive` of a revision, so the checkout and
# /Applications/LokalBot.app are never modified. The copy is re-signed to the
# installed app's designated requirement so its Accessibility and Input
# Monitoring grants apply.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
REV="${1:-HEAD}"
WORK="${FILMING_WORK:-/private/tmp/lokalbot-filming}"
SRC="$WORK/src"
DD="$WORK/DerivedData"
OUT="$WORK/LokalBot.app"
LOG="$WORK/build.log"
INSTALLED=/Applications/LokalBot.app
OVERLAY=LokalBot/Cotyping/CotypingOverlayController.swift

mkdir -p "$WORK"
rm -rf "$SRC" "$DD" "$OUT"
mkdir -p "$SRC"
git -C "$REPO" archive "$REV" | tar -x -C "$SRC"
git -C "$REPO" rev-parse "$REV" > "$WORK/revision.txt"
cp -c -R "$REPO/Vendor/." "$SRC/Vendor/"
mkdir -p "$SRC/.build"
cp -c -R "$REPO/.build/SourcePackages" "$SRC/.build/SourcePackages"

grep -q 'panel.sharingType = .none' "$SRC/$OVERLAY"
sed -i '' 's/panel.sharingType = .none/panel.sharingType = .readOnly/' "$SRC/$OVERLAY"
grep -q 'panel.sharingType = .readOnly' "$SRC/$OVERLAY"

TEAM="$(codesign -dv "$INSTALLED" 2>&1 | awk -F= '/^TeamIdentifier=/{print $2}')"
REQ="$(codesign -d -r- "$INSTALLED" 2>&1 | sed -n 's/^designated => //p')"
IDENTITY="$(codesign -dvv "$INSTALLED" 2>&1 | awk -F= '/^Authority=Developer ID Application/ && !seen {print $2; seen=1}')"
[ -n "$TEAM" ] && [ -n "$REQ" ] && [ -n "$IDENTITY" ]

cd "$SRC"
xcodegen generate >/dev/null
xcodebuild archive -project LokalBot.xcodeproj -scheme LokalBot -configuration Release \
  -archivePath "$DD/LokalBot.xcarchive" -derivedDataPath "$DD" \
  -clonedSourcePackagesDirPath "$SRC/.build/SourcePackages" \
  -destination 'generic/platform=macOS' ENABLE_HARDENED_RUNTIME=YES ARCHS=arm64 \
  DEVELOPMENT_TEAM="$TEAM" CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="Developer ID Application" \
  -quiet > "$LOG" 2>&1
/usr/bin/plutil -create xml1 "$DD/ExportOptions.plist"
/usr/bin/plutil -insert method -string developer-id "$DD/ExportOptions.plist"
/usr/bin/plutil -insert teamID -string "$TEAM" "$DD/ExportOptions.plist"
/usr/bin/plutil -insert signingStyle -string automatic "$DD/ExportOptions.plist"
xcodebuild -exportArchive -archivePath "$DD/LokalBot.xcarchive" -exportPath "$DD/export" \
  -exportOptionsPlist "$DD/ExportOptions.plist" -quiet >> "$LOG" 2>&1

APP="$DD/export/LokalBot.app"
codesign --verify --test-requirement="=$REQ" "$APP"
codesign --force --preserve-metadata=identifier,entitlements,flags,runtime \
  --requirements="=designated => $REQ" --options runtime --timestamp \
  --sign "$IDENTITY" "$APP" >> "$LOG" 2>&1
codesign --verify --deep --strict "$APP"
[ "$(codesign -d -r- "$APP" 2>&1 | sed -n 's/^designated => //p')" = "$REQ" ]
ditto "$APP" "$OUT"
echo "Built $OUT from $(cat "$WORK/revision.txt")"
