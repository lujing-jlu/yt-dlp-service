#!/bin/bash
set -euo pipefail

PLIST_DST="$HOME/Library/LaunchAgents/com.ytdlp.service.plist"

echo "Stopping/unloading service..."
launchctl stop com.ytdlp.service >/dev/null 2>&1 || true
launchctl unload "$PLIST_DST" >/dev/null 2>&1 || true

echo "Removing plist: $PLIST_DST"
rm -f "$PLIST_DST"

echo "Done."

