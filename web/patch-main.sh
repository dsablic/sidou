#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The changes SIDOU's demo makes to fm1-emu's web/main.js (copied at the pinned revision by
# tools/web-demo). Each must apply exactly once, or the build stops: a new upstream revision
# gets a look.
#   (A first edit resolved the "downloading from <host>" status line's new URL(url) against the
#   page, as it threw for sidou.fwsc served next to it; since 1adbd85 upstream's boot no longer
#   names the host, so it went with the bump to db8d472e.)
#   1. The default clock is 240 MHz, not 100: the emulator gate measures SIDOU's audio budget at
#      240 MHz (host/crates/emu-gate), and fewer instructions a second can starve its audio in
#      emulated time. ?mhz= and the Clock menu still choose another.
#   2. Start is enabled only once its handler is attached: main.js awaits the catalog and the
#      wasm first, and a click before then did nothing (index.html starts it disabled).
#   web/patch-main.sh build/web/main.js
set -eu
f="$1"
edit() {
    n=$(grep -cF "$1" "$f" || true)
    if [ "$n" != 1 ]; then
        echo "patch-main: expected 1 line with '$1' in $f, found $n" >&2
        exit 1
    fi
    sed -i.orig "s#$(printf '%s' "$1" | sed 's/[].[*^$]/\\&/g')#$2#" "$f"
    rm -f "$f.orig"
    grep -qF "$2" "$f"
}
edit 'stored("fm1.mhz") || 100)' 'stored("fm1.mhz") || 240)'
edit 'startBtn.addEventListener("click", async () => {' 'startBtn.disabled = false; startBtn.addEventListener("click", async () => {'
