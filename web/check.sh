#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Serve a built demo on a free localhost port and run the smoke test (web/check.mjs) on it.
# Playwright, at the version below, and its headless Chromium are installed into
# build/web-check on first use (with the system libraries it needs when CI is set).
#   web/check.sh build/web [screenshot.png]    (tools/web-demo --check runs it)
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
site=$(cd "$1" && pwd)
shot="${2:-${SIDOU_WEB_SCREENSHOT:-$root/build/web-check/screenshot.png}}"
playwright=1.63.0
dir="$root/build/web-check"
# the SysEx code in plain Node first (no browser, no MIDI): web/sysex.test.mjs
node "$root/web/sysex.test.mjs"
# the Songs -> MIDI code against GoatTracker's own player (fixtures from gt2-ref), also in plain Node
node "$root/web/songmidi.test.mjs"
# the Library page's store work and backups against a simulated FM-1, and the site's wasm core
# against the host build of the same C (fixtures from sidou-tests' web_core test)
node "$root/web/library.test.mjs"
node "$root/web/core.test.mjs" "$site/sidou-core.wasm"
mkdir -p "$dir"
if [ "$(node -p 'require(process.argv[1]).version' "$dir/node_modules/playwright/package.json" 2>/dev/null)" != "$playwright" ]; then
    (cd "$dir" && npm install --no-audit --no-fund --silent "playwright@$playwright")
fi
deps=()
# On CI the system libraries too, unless the build image already has them (tools/docker/Dockerfile).
[ -z "${CI:-}" ] || [ -n "${SIDOU_BUILD_IMAGE:-}" ] || deps=(--with-deps)
(cd "$dir" && npx --no-install playwright install "${deps[@]}" --only-shell chromium)
cp "$root/web/check.mjs" "$dir/check.mjs"

log="$dir/serve.log"
python3 "$root/web/serve.py" "$site" 0 >"$log" 2>&1 &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
for _ in $(seq 100); do
    url=$(grep -o 'http://[^ ]*' "$log" || true)
    [ -z "$url" ] || break
    kill -0 $server 2>/dev/null || { cat "$log" >&2; exit 1; }
    sleep 0.1
done
[ -n "$url" ] || { echo "check: the server did not start" >&2; exit 1; }
echo "serving $site at $url"
node "$dir/check.mjs" "$url" "$shot"
