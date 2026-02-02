#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLIST_TEMPLATE="$ROOT_DIR/macos/com.ytdlp.service.plist"
PLIST_DST="$HOME/Library/LaunchAgents/com.ytdlp.service.plist"
BIN_DIR="$HOME/bin"
BIN_DST="$BIN_DIR/yt_dlp_service"
CFG_DST="$ROOT_DIR/config.toml"

echo "[1/5] Build release..."
cd "$ROOT_DIR"
cargo build --release

echo "[2/5] Install binary -> $BIN_DST"
mkdir -p "$BIN_DIR"
cp -f "$ROOT_DIR/target/release/yt_dlp_service" "$BIN_DST"

echo "[3/5] Ensure config.toml exists -> $CFG_DST"
if [ ! -f "$CFG_DST" ]; then
  cp -f "$ROOT_DIR/config.example.toml" "$CFG_DST"
  echo "  created from config.example.toml; please edit it if needed"
fi

echo "[4/5] Install LaunchAgent plist -> $PLIST_DST"
mkdir -p "$(dirname "$PLIST_DST")"
python3 - "$PLIST_TEMPLATE" "$PLIST_DST" "$BIN_DST" "$CFG_DST" "$ROOT_DIR" <<'PY'
import sys
from pathlib import Path

template, dst, bin_path, cfg_path, workdir = sys.argv[1:]
text = Path(template).read_text(encoding="utf-8")
text = text.replace("__BIN__", bin_path)
text = text.replace("__CONFIG__", cfg_path)
text = text.replace("__WORKDIR__", workdir)
Path(dst).write_text(text, encoding="utf-8")
PY

echo "[5/5] (Re)load service..."
launchctl unload "$PLIST_DST" >/dev/null 2>&1 || true
launchctl load "$PLIST_DST"
launchctl start com.ytdlp.service

echo "Done."
echo "Logs:"
echo "  tail -f /tmp/ytdlp-service.log"
echo "  tail -f /tmp/ytdlp-service.error.log"
