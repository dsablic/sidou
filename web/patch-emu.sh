#!/bin/sh
# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
f="$1"
. "$(dirname "$0")/patch-edit.sh"
edit 'soc.periph.skip_idle = true;' 'soc.periph.skip_idle = true; soc.write32(0x119A0, 0x4540_0203); soc.write32(0x119A4, 0x3F50_3026);'
