/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef GT2REF_RENAME_H
#define GT2REF_RENAME_H

#include <stdio.h>
#include <stdlib.h>

#include "gt2ref_shim.h"

#define fopen gt2ref_fopen
#define fread gt2ref_fread
#define exit gt2ref_exit
#define main gt2reloc_main
#define malloc gt2ref_malloc
#define free gt2ref_gt_free
#undef stdout
#define stdout gt2ref_quiet()

#endif
