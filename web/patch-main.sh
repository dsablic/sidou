#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
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
