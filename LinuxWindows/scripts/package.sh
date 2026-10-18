#!/usr/bin/env bash
# Builds release binaries and packs them for the current platform into dist/.
# Linux: LokalBot-<version>-linux-x86_64.tar.gz (with install.sh)
# Windows (Git Bash or CI): LokalBot-<version>-windows-x86_64.zip
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release --locked -p lokalbot-app -p lokalbot-cli
mkdir -p dist
case "$(uname -s)" in
  Linux)
    NAME="LokalBot-$VERSION-linux-x86_64"
    STAGE="dist/$NAME"
    rm -rf "$STAGE" && mkdir -p "$STAGE/icons"
    cp target/release/lokalbot target/release/lokalbot-cli "$STAGE/"
    cp packaging/linux/com.lokalbot.LokalBot.desktop packaging/linux/install.sh "$STAGE/"
    cp packaging/icons/lokalbot-*.png "$STAGE/icons/"
    cp ../LICENSE "$STAGE/LICENSE" 2>/dev/null || true
    tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
    sha256sum "dist/$NAME.tar.gz" > "dist/$NAME.tar.gz.sha256"
    echo "dist/$NAME.tar.gz"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    NAME="LokalBot-$VERSION-windows-x86_64"
    STAGE="dist/$NAME"
    rm -rf "$STAGE" && mkdir -p "$STAGE"
    cp target/release/lokalbot.exe target/release/lokalbot-cli.exe "$STAGE/"
    cp ../LICENSE "$STAGE/LICENSE.txt" 2>/dev/null || true
    (cd dist && 7z a -tzip "$NAME.zip" "$NAME" >/dev/null)
    (cd dist && sha256sum "$NAME.zip" > "$NAME.zip.sha256")
    echo "dist/$NAME.zip"
    ;;
  *) echo "unsupported platform: $(uname -s)"; exit 1 ;;
esac
