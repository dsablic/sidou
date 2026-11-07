#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
f="$1"
. "$(dirname "$0")/patch-edit.sh"
edit 'stored("fm1.mhz") || 100)' 'stored("fm1.mhz") || 240)'
edit 'startBtn.addEventListener("click", async () => {' 'startBtn.disabled = false; startBtn.addEventListener("click", async () => {'
