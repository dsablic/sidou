#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# The changes SIDOU's demo makes to fm1-emu's web/main.js (copied at the pinned revision by
# tools/web-demo). Each must apply exactly once, or the build stops: a new upstream revision
# gets a look.
#   1. The status line names the host a package comes from with new URL(url), which throws for
#      a package served next to the page (sidou.fwsc): resolve it against the page.
#   2. The default clock is 240 MHz, not 100: the emulator gate measures SIDOU's audio budget at
#      240 MHz (host/crates/emu-gate), and fewer instructions a second can starve its audio in
#      emulated time. ?mhz= and the Clock menu still choose another.
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
edit 'new URL(f.package || f.installer).host' 'new URL(f.package || f.installer, location.href).host'
edit 'stored("fm1.mhz") || 100)' 'stored("fm1.mhz") || 240)'
