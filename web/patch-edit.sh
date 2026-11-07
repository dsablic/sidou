# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
edit() {
    n=$(grep -cF "$1" "$f" || true)
    if [ "$n" != 1 ]; then
        echo "patch: expected 1 line with '$1' in $f, found $n" >&2
        exit 1
    fi
    sed -i.orig "s#$(printf '%s' "$1" | sed 's/[].[*^$]/\\&/g')#$2#" "$f"
    rm -f "$f.orig"
    grep -qF "$2" "$f"
}
