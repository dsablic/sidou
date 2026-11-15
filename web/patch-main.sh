#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
f="$1"
. "$(dirname "$0")/patch-edit.sh"
edit 'stored("fm1.mhz") || 100)' 'stored("fm1.mhz") || 240)'
edit 'startBtn.addEventListener("click", () => start());' 'startBtn.disabled = false; startBtn.addEventListener("click", () => start());'
edit 'const sectors = trying ? trying.sectors : record?.sectors || [];' 'const sectors = trying ? trying.sectors : record?.sectors?.length || f.id !== "sidou" ? record?.sectors || [] : await sidouSeed();'
edit 'function boot(options = {}) {' 'async function sidouSeed() { const b = new Uint8Array(await (await fetch("store-seed.bin")).arrayBuffer()); const out = []; for (let o = 0; o + 4100 <= b.length; o += 4100) out.push({ at: b[o] | b[o + 1] << 8 | b[o + 2] << 16 | b[o + 3] << 24, data: b.slice(o + 4, o + 4100) }); return out; } function boot(options = {}) {'
